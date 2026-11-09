//! Zero-dependency reader for package manifests: `aoxn.toml` (current) and
//! `aoxn.json` (legacy).
//!
//! The compiler library (`src/`) has a strict zero-external-crate policy
//! (see SECURITY.md), so this module hand-rolls the small TOML/JSON subsets
//! entry resolution needs instead of pulling in `toml`/`serde_json`. It
//! extracts only `[project]` `main` / `types` / `exports` (with a read-only
//! `[tool.aoxn]` fallback); dependencies, registries and workspaces belong
//! to the `aoxn-pkg` crate, which uses the full `toml` crate.
//!
//! Resolution order for a bare package import with subpath `s` (None or "."
//! means the package root):
//! 1. `exports[s]` / `exports["."]` if `exports` is present
//! 2. `main` when `s` is None/"."
//! 3. fall through to the legacy `aox_modules/<name>` directory probe in
//!    `resolve_import` (this function returns `None` then).
//!
//! `exports` values may be a plain path string or a conditional object
//! (`{ default = "...", types = "..." }` / JSON `{ "default": ... }`); the
//! `"default"` entry wins, then `"types"`.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Shared entry information

#[derive(Debug, Clone)]
enum ExportValue {
    Path(String),
    Conditional {
        default: Option<String>,
        types: Option<String>,
    },
}

impl ExportValue {
    fn target(&self) -> Option<&str> {
        match self {
            ExportValue::Path(s) => Some(s),
            ExportValue::Conditional { default, types } => default.as_deref().or(types.as_deref()),
        }
    }
}

#[derive(Debug, Default)]
struct EntryInfo {
    main: Option<String>,
    types: Option<String>,
    exports: BTreeMap<String, ExportValue>,
    /// An EMPTY exports table still gates every entry (npm semantics), so
    /// presence cannot be inferred from the map being empty.
    exports_present: bool,
}

// ---------------------------------------------------------------------------
// JSON (legacy aoxn.json)

#[derive(Debug, Clone)]
#[allow(dead_code)] // Num/Array payloads are accepted by the parser, not read out.
enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Array(Vec<Json>),
    Object(BTreeMap<String, Json>),
}

impl Json {
    fn as_object(&self) -> Option<&BTreeMap<String, Json>> {
        if let Json::Object(m) = self { Some(m) } else { None }
    }
}

struct JsonParser<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> JsonParser<'a> {
    fn new(s: &'a str) -> Self {
        JsonParser { b: s.as_bytes(), i: 0 }
    }

    fn ws(&mut self) {
        while self.i < self.b.len() {
            match self.b[self.i] {
                b' ' | b'\t' | b'\n' | b'\r' => self.i += 1,
                _ => break,
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }

    fn parse_value(&mut self) -> Option<Json> {
        self.ws();
        match self.peek()? {
            b'{' => self.parse_object(),
            b'[' => self.parse_array(),
            b'"' => self.parse_string().map(Json::Str),
            b't' | b'f' => self.parse_bool(),
            b'n' => self.parse_null(),
            b'-' | b'0'..=b'9' => self.parse_number(),
            _ => None,
        }
    }

    fn parse_object(&mut self) -> Option<Json> {
        self.i += 1;
        let mut map = BTreeMap::new();
        self.ws();
        if self.peek() == Some(b'}') {
            self.i += 1;
            return Some(Json::Object(map));
        }
        loop {
            self.ws();
            if self.peek() != Some(b'"') {
                return None;
            }
            let key = self.parse_string()?;
            self.ws();
            if self.peek() != Some(b':') {
                return None;
            }
            self.i += 1;
            let val = self.parse_value()?;
            map.insert(key, val);
            self.ws();
            match self.peek()? {
                b',' => {
                    self.i += 1;
                }
                b'}' => {
                    self.i += 1;
                    return Some(Json::Object(map));
                }
                _ => return None,
            }
        }
    }

    fn parse_array(&mut self) -> Option<Json> {
        self.i += 1;
        let mut items = Vec::new();
        self.ws();
        if self.peek() == Some(b']') {
            self.i += 1;
            return Some(Json::Array(items));
        }
        loop {
            items.push(self.parse_value()?);
            self.ws();
            match self.peek()? {
                b',' => {
                    self.i += 1;
                }
                b']' => {
                    self.i += 1;
                    return Some(Json::Array(items));
                }
                _ => return None,
            }
        }
    }

    fn parse_string(&mut self) -> Option<String> {
        self.i += 1;
        let mut buf: Vec<u8> = Vec::new();
        while self.i < self.b.len() {
            let c = self.b[self.i];
            self.i += 1;
            match c {
                b'"' => return String::from_utf8(buf).ok(),
                b'\\' => {
                    let e = *self.b.get(self.i)?;
                    self.i += 1;
                    match e {
                        b'"' => buf.push(b'"'),
                        b'\\' => buf.push(b'\\'),
                        b'/' => buf.push(b'/'),
                        b'n' => buf.push(b'\n'),
                        b't' => buf.push(b'\t'),
                        b'r' => buf.push(b'\r'),
                        b'b' => buf.push(0x08),
                        b'f' => buf.push(0x0C),
                        b'u' => {
                            let h = std::str::from_utf8(self.b.get(self.i..self.i + 4)?).ok()?;
                            let cp = u32::from_str_radix(h, 16).ok()?;
                            self.i += 4;
                            if let Some(ch) = char::from_u32(cp) {
                                let mut enc = [0u8; 4];
                                buf.extend_from_slice(ch.encode_utf8(&mut enc).as_bytes());
                            }
                        }
                        _ => return None,
                    }
                }
                _ => buf.push(c),
            }
        }
        None
    }

    fn parse_bool(&mut self) -> Option<Json> {
        if self.b[self.i..].starts_with(b"true") {
            self.i += 4;
            Some(Json::Bool(true))
        } else if self.b[self.i..].starts_with(b"false") {
            self.i += 5;
            Some(Json::Bool(false))
        } else {
            None
        }
    }

    fn parse_null(&mut self) -> Option<Json> {
        if self.b[self.i..].starts_with(b"null") {
            self.i += 4;
            Some(Json::Null)
        } else {
            None
        }
    }

    fn parse_number(&mut self) -> Option<Json> {
        let start = self.i;
        if self.peek() == Some(b'-') {
            self.i += 1;
        }
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || c == b'.' || c == b'e' || c == b'E' || c == b'+' || c == b'-' {
                self.i += 1;
            } else {
                break;
            }
        }
        let s = std::str::from_utf8(self.b.get(start..self.i)?).ok()?;
        s.parse::<f64>().ok().map(Json::Num)
    }
}

fn parse_json(s: &str) -> Option<Json> {
    let s = s.strip_prefix('\u{FEFF}').unwrap_or(s);
    let mut p = JsonParser::new(s);
    let v = p.parse_value()?;
    p.ws();
    if p.i != p.b.len() {
        return None;
    }
    Some(v)
}

// ---------------------------------------------------------------------------
// TOML (current aoxn.toml)
//
// This is a deliberately small TOML reader: tables, dotted/quoted keys,
// basic/literal strings (single- and multi-line), booleans, numbers, arrays
// and inline tables. It is enough to walk a pyproject-shaped manifest and
// skip sections it does not care about. Unknown syntax makes the manifest
// unreadable, which falls back to the legacy directory probe rather than
// failing a build, matching the JSON reader's leniency.

#[derive(Debug, Clone)]
#[allow(dead_code)] // values are parsed generically; only strings/tables are read out
enum Toml {
    Str(String),
    Bool(bool),
    Num(f64),
    Bare(String),
    Array(Vec<Toml>),
    Table(BTreeMap<String, Toml>),
}

impl Toml {
    fn as_str(&self) -> Option<&str> {
        if let Toml::Str(s) = self { Some(s) } else { None }
    }
    fn as_table(&self) -> Option<&BTreeMap<String, Toml>> {
        if let Toml::Table(m) = self { Some(m) } else { None }
    }
    fn get(&self, key: &str) -> Option<&Toml> {
        self.as_table()?.get(key)
    }
}

struct TomlParser<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> TomlParser<'a> {
    fn new(s: &'a str) -> Self {
        TomlParser { b: s.as_bytes(), i: 0 }
    }

    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }

    fn starts_with(&self, prefix: &[u8]) -> bool {
        self.b.get(self.i..).is_some_and(|rest| rest.starts_with(prefix))
    }

    fn eat(&mut self, prefix: &[u8]) -> bool {
        if self.starts_with(prefix) {
            self.i += prefix.len();
            true
        } else {
            false
        }
    }

    fn skip_inline_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\r')) {
            self.i += 1;
        }
    }

    fn skip_trivia(&mut self) {
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\r' | b'\n') => self.i += 1,
                Some(b'#') => {
                    while let Some(c) = self.peek() {
                        self.i += 1;
                        if c == b'\n' {
                            break;
                        }
                    }
                }
                _ => break,
            }
        }
    }

    fn parse_doc(mut self) -> Option<Toml> {
        let mut root = Toml::Table(BTreeMap::new());
        let mut current: Vec<String> = Vec::new();
        let mut ignored = 0usize;
        loop {
            self.skip_trivia();
            if self.i >= self.b.len() {
                break;
            }
            if self.peek() == Some(b'[') {
                let array_of_tables = self.b.get(self.i + 1) == Some(&b'[');
                self.i += if array_of_tables { 2 } else { 1 };
                self.skip_inline_ws();
                let path = self.parse_key_path()?;
                self.skip_inline_ws();
                let close: &[u8] = if array_of_tables { b"]]" } else { b"]" };
                if !self.eat(close) {
                    return None;
                }
                if array_of_tables {
                    // We never read arrays of tables (authors, dependencies
                    // as tables, ...), so park their keys under a private
                    // table and keep walking.
                    ignored += 1;
                    current = vec!["__ignored".to_string(), ignored.to_string()];
                } else {
                    current = path;
                }
                ensure_table_path(&mut root, &current)?;
            } else {
                let key = self.parse_key_path()?;
                self.skip_inline_ws();
                if !self.eat(b"=") {
                    return None;
                }
                self.skip_trivia();
                let value = self.parse_value()?;
                insert_path(&mut root, &current, &key, value)?;
            }
        }
        Some(root)
    }

    fn parse_key_path(&mut self) -> Option<Vec<String>> {
        let mut parts = Vec::new();
        loop {
            self.skip_inline_ws();
            parts.push(self.parse_key_segment()?);
            self.skip_inline_ws();
            if self.peek() == Some(b'.') {
                self.i += 1;
                continue;
            }
            break;
        }
        Some(parts)
    }

    fn parse_key_segment(&mut self) -> Option<String> {
        match self.peek()? {
            b'"' => self.parse_basic_string(),
            b'\'' => self.parse_literal_string(),
            _ => {
                let start = self.i;
                // TOML bare keys: A-Z a-z 0-9 _ -. Dots separate key parts,
                // so they must NOT be consumed here (`[tool.aoxn]` is two
                // segments, not one).
                while let Some(c) = self.peek() {
                    if c.is_ascii_alphanumeric() || c == b'_' || c == b'-' {
                        self.i += 1;
                    } else {
                        break;
                    }
                }
                if self.i == start {
                    return None;
                }
                String::from_utf8(self.b[start..self.i].to_vec()).ok()
            }
        }
    }

    fn parse_value(&mut self) -> Option<Toml> {
        self.skip_inline_ws();
        match self.peek()? {
            b'"' => {
                if self.starts_with(b"\"\"\"") {
                    self.parse_multiline_basic_string().map(Toml::Str)
                } else {
                    self.parse_basic_string().map(Toml::Str)
                }
            }
            b'\'' => {
                if self.starts_with(b"'''") {
                    self.parse_multiline_literal_string().map(Toml::Str)
                } else {
                    self.parse_literal_string().map(Toml::Str)
                }
            }
            b'[' => self.parse_array(),
            b'{' => self.parse_inline_table(),
            b't' if self.starts_with(b"true") => {
                self.i += 4;
                Some(Toml::Bool(true))
            }
            b'f' if self.starts_with(b"false") => {
                self.i += 5;
                Some(Toml::Bool(false))
            }
            _ => self.parse_bare_token(),
        }
    }

    fn parse_basic_string(&mut self) -> Option<String> {
        self.i += 1; // opening quote
        let mut buf: Vec<u8> = Vec::new();
        loop {
            let c = self.peek()?;
            self.i += 1;
            match c {
                b'"' => return String::from_utf8(buf).ok(),
                b'\n' => return None,
                b'\\' => {
                    let e = self.peek()?;
                    self.i += 1;
                    match e {
                        b'"' => buf.push(b'"'),
                        b'\\' => buf.push(b'\\'),
                        b'b' => buf.push(0x08),
                        b'f' => buf.push(0x0C),
                        b'n' => buf.push(b'\n'),
                        b'r' => buf.push(b'\r'),
                        b't' => buf.push(b'\t'),
                        b'u' => {
                            let cp = self.read_hex(4)?;
                            self.push_char(&mut buf, cp);
                        }
                        b'U' => {
                            let cp = self.read_hex(8)?;
                            self.push_char(&mut buf, cp);
                        }
                        _ => return None,
                    }
                }
                _ => buf.push(c),
            }
        }
    }

    fn parse_multiline_basic_string(&mut self) -> Option<String> {
        self.i += 3;
        if self.peek() == Some(b'\n') {
            self.i += 1;
        } else if self.peek() == Some(b'\r') && self.b.get(self.i + 1) == Some(&b'\n') {
            self.i += 2;
        }
        let mut buf: Vec<u8> = Vec::new();
        loop {
            if self.starts_with(b"\"\"\"") {
                self.i += 3;
                return String::from_utf8(buf).ok();
            }
            let c = self.peek()?;
            self.i += 1;
            match c {
                b'\\' => {
                    let e = self.peek()?;
                    self.i += 1;
                    match e {
                        b'"' => buf.push(b'"'),
                        b'\\' => buf.push(b'\\'),
                        b'b' => buf.push(0x08),
                        b'f' => buf.push(0x0C),
                        b'n' => buf.push(b'\n'),
                        b'r' => buf.push(b'\r'),
                        b't' => buf.push(b'\t'),
                        b'\n' | b'\r' => {
                            while matches!(self.peek(), Some(b' ' | b'\t' | b'\r' | b'\n')) {
                                self.i += 1;
                            }
                        }
                        b'u' => {
                            let cp = self.read_hex(4)?;
                            self.push_char(&mut buf, cp);
                        }
                        b'U' => {
                            let cp = self.read_hex(8)?;
                            self.push_char(&mut buf, cp);
                        }
                        _ => return None,
                    }
                }
                _ => buf.push(c),
            }
        }
    }

    fn parse_literal_string(&mut self) -> Option<String> {
        self.i += 1;
        let start = self.i;
        loop {
            match self.peek()? {
                b'\'' => {
                    let s = std::str::from_utf8(self.b.get(start..self.i)?).ok()?.to_string();
                    self.i += 1;
                    return Some(s);
                }
                b'\n' => return None,
                _ => self.i += 1,
            }
        }
    }

    fn parse_multiline_literal_string(&mut self) -> Option<String> {
        self.i += 3;
        if self.peek() == Some(b'\n') {
            self.i += 1;
        } else if self.peek() == Some(b'\r') && self.b.get(self.i + 1) == Some(&b'\n') {
            self.i += 2;
        }
        let start = self.i;
        loop {
            if self.starts_with(b"'''") {
                let s = std::str::from_utf8(self.b.get(start..self.i)?).ok()?.to_string();
                self.i += 3;
                return Some(s);
            }
            self.i += 1;
            if self.i > self.b.len() {
                return None;
            }
        }
    }

    fn read_hex(&mut self, n: usize) -> Option<u32> {
        let h = std::str::from_utf8(self.b.get(self.i..self.i + n)?).ok()?;
        let cp = u32::from_str_radix(h, 16).ok()?;
        self.i += n;
        Some(cp)
    }

    fn push_char(&self, buf: &mut Vec<u8>, cp: u32) {
        if let Some(ch) = char::from_u32(cp) {
            let mut enc = [0u8; 4];
            buf.extend_from_slice(ch.encode_utf8(&mut enc).as_bytes());
        }
    }

    fn parse_array(&mut self) -> Option<Toml> {
        self.i += 1;
        let mut items = Vec::new();
        loop {
            self.skip_trivia();
            if self.eat(b"]") {
                return Some(Toml::Array(items));
            }
            items.push(self.parse_value()?);
            self.skip_trivia();
            if self.eat(b",") {
                continue;
            }
            if self.eat(b"]") {
                return Some(Toml::Array(items));
            }
            return None;
        }
    }

    fn parse_inline_table(&mut self) -> Option<Toml> {
        self.i += 1;
        let mut map = BTreeMap::new();
        loop {
            self.skip_trivia();
            if self.eat(b"}") {
                return Some(Toml::Table(map));
            }
            let key = self.parse_key_path()?;
            self.skip_inline_ws();
            if !self.eat(b"=") {
                return None;
            }
            let value = self.parse_value()?;
            insert_into_table(&mut map, &key, value)?;
            self.skip_trivia();
            if self.eat(b",") {
                continue;
            }
            if self.eat(b"}") {
                return Some(Toml::Table(map));
            }
            return None;
        }
    }

    fn parse_bare_token(&mut self) -> Option<Toml> {
        let start = self.i;
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric()
                || matches!(c, b'+' | b'-' | b'.' | b':' | b'_' | b'Z' | b'T')
            {
                self.i += 1;
            } else {
                break;
            }
        }
        if self.i == start {
            return None;
        }
        let raw = std::str::from_utf8(self.b.get(start..self.i)?).ok()?;
        if let Ok(n) = raw.replace('_', "").parse::<f64>() {
            Some(Toml::Num(n))
        } else {
            Some(Toml::Bare(raw.to_string()))
        }
    }
}

fn parse_toml(s: &str) -> Option<Toml> {
    let s = s.strip_prefix('\u{FEFF}').unwrap_or(s);
    TomlParser::new(s).parse_doc()
}

fn table_child<'a>(node: &'a mut Toml, key: &str) -> Option<&'a mut Toml> {
    match node {
        Toml::Table(map) => {
            let entry = map
                .entry(key.to_string())
                .or_insert_with(|| Toml::Table(BTreeMap::new()));
            if matches!(entry, Toml::Table(_)) {
                Some(entry)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn ensure_table_path(root: &mut Toml, path: &[String]) -> Option<()> {
    let mut node = root;
    for segment in path {
        node = table_child(node, segment)?;
    }
    Some(())
}

fn insert_into_table(
    map: &mut BTreeMap<String, Toml>,
    key: &[String],
    value: Toml,
) -> Option<()> {
    let Some((first, rest)) = key.split_first() else {
        return None;
    };
    if rest.is_empty() {
        // TOML forbids redefining a key.
        if map.contains_key(first) {
            return None;
        }
        map.insert(first.clone(), value);
        return Some(());
    }
    let entry = map
        .entry(first.clone())
        .or_insert_with(|| Toml::Table(BTreeMap::new()));
    let Toml::Table(child) = entry else {
        return None;
    };
    insert_into_table(child, rest, value)
}

fn insert_path(
    root: &mut Toml,
    current: &[String],
    key: &[String],
    value: Toml,
) -> Option<()> {
    let mut node = root;
    for segment in current {
        node = table_child(node, segment)?;
    }
    let Toml::Table(map) = node else {
        return None;
    };
    insert_into_table(map, key, value)
}

// ---------------------------------------------------------------------------
// Manifest -> EntryInfo

fn entry_info_from_json(root: &BTreeMap<String, Json>) -> EntryInfo {
    let mut info = EntryInfo::default();
    if let Some(Json::Str(m)) = root.get("main") {
        info.main = Some(m.clone());
    }
    if let Some(Json::Str(t)) = root.get("types") {
        info.types = Some(t.clone());
    }
    if let Some(Json::Object(exports)) = root.get("exports") {
        info.exports_present = true;
        for (key, value) in exports {
            info.exports.insert(key.clone(), export_from_json(value));
        }
    }
    info
}

fn export_from_json(value: &Json) -> ExportValue {
    match value {
        Json::Str(s) => ExportValue::Path(s.clone()),
        Json::Object(obj) => {
            let default = match obj.get("default") {
                Some(Json::Str(s)) => Some(s.clone()),
                _ => None,
            };
            let types = match obj.get("types") {
                Some(Json::Str(s)) => Some(s.clone()),
                _ => None,
            };
            ExportValue::Conditional { default, types }
        }
        _ => ExportValue::Path(String::new()),
    }
}

fn entry_info_from_toml(root: &Toml) -> Option<EntryInfo> {
    let project = root.get("project")?.as_table()?;
    let mut info = EntryInfo::default();
    if let Some(m) = project.get("main").and_then(Toml::as_str) {
        info.main = Some(m.to_string());
    }
    if let Some(t) = project.get("types").and_then(Toml::as_str) {
        info.types = Some(t.to_string());
    }
    if let Some(exports) = project.get("exports").and_then(Toml::as_table) {
        info.exports_present = true;
        for (key, value) in exports {
            info.exports.insert(key.clone(), export_from_toml(value));
        }
    }

    // Read-only fallback for npm-style manifests that put entry points under
    // `[tool.aoxn]`.
    if let Some(aoxn) = root
        .get("tool")
        .and_then(|t| t.get("aoxn"))
        .and_then(Toml::as_table)
    {
        if info.main.is_none() {
            info.main = aoxn.get("main").and_then(Toml::as_str).map(str::to_string);
        }
        if info.types.is_none() {
            info.types = aoxn.get("types").and_then(Toml::as_str).map(str::to_string);
        }
        if !info.exports_present {
            if let Some(exports) = aoxn.get("exports").and_then(Toml::as_table) {
                info.exports_present = true;
                for (key, value) in exports {
                    info.exports.insert(key.clone(), export_from_toml(value));
                }
            }
        }
    }
    Some(info)
}

fn export_from_toml(value: &Toml) -> ExportValue {
    match value {
        Toml::Str(s) => ExportValue::Path(s.clone()),
        Toml::Table(obj) => ExportValue::Conditional {
            default: obj.get("default").and_then(Toml::as_str).map(str::to_string),
            types: obj.get("types").and_then(Toml::as_str).map(str::to_string),
        },
        _ => ExportValue::Path(String::new()),
    }
}

fn read_entry_info(pkg_dir: &Path) -> Option<EntryInfo> {
    let toml_path = pkg_dir.join("aoxn.toml");
    if let Ok(text) = std::fs::read_to_string(&toml_path) {
        if let Some(info) = parse_toml(&text).as_ref().and_then(entry_info_from_toml) {
            return Some(info);
        }
    }
    let json_path = pkg_dir.join("aoxn.json");
    let text = std::fs::read_to_string(json_path).ok()?;
    let root = parse_json(&text)?.as_object()?.clone();
    Some(entry_info_from_json(&root))
}

// The loader asks this question once per IMPORT, and every module of a package
// asks it about the same manifest. Without the memo below, the manifest was
// read from disk and parsed once per import edge -- for a package with N
// modules that is N reads and N parses of one file. The cache is a
// `thread_local` because resolution is single-threaded and this is a
// build-time answer: the tree cannot change under a build already walking it.
thread_local! {
    static ENTRY_CACHE: std::cell::RefCell<HashMap<(PathBuf, Option<String>), Option<PathBuf>>> =
        std::cell::RefCell::new(HashMap::new());
}

/// Resolve the entry source file for a package rooted at `pkg_dir`, given a
/// subpath (None or `"."` = the package root; `"./sub"` or `"sub"` = that
/// subpath). Returns the resolved file path only when it exists on disk.
///
/// Memoized per process; see the note on `ENTRY_CACHE`.
pub fn resolve_pkg_entry(pkg_dir: &Path, subpath: Option<&str>) -> Option<PathBuf> {
    let key = (pkg_dir.to_path_buf(), subpath.map(|s| s.to_string()));
    ENTRY_CACHE.with(|c| {
        if let Some(hit) = c.borrow().get(&key) {
            return hit.clone();
        }
        let fresh = resolve_pkg_entry_uncached(pkg_dir, subpath);
        c.borrow_mut().insert(key, fresh.clone());
        fresh
    })
}

fn resolve_pkg_entry_uncached(pkg_dir: &Path, subpath: Option<&str>) -> Option<PathBuf> {
    let info = read_entry_info(pkg_dir)?;

    let sub = match subpath {
        None | Some(".") | Some("") => ".",
        Some(s) => {
            // normalize "sub" and "./sub" to "./sub"
            if !s.starts_with("./") {
                return exports_lookup(pkg_dir, &info, &format!("./{s}"));
            }
            s
        }
    };
    exports_lookup(pkg_dir, &info, sub)
}

fn exports_lookup(pkg_dir: &Path, info: &EntryInfo, sub: &str) -> Option<PathBuf> {
    if info.exports_present {
        // exports present but this subpath missing (or its file absent) →
        // stop. Falling back to `main` would contradict npm semantics.
        let target = info.exports.get(sub)?.target()?;
        if target.is_empty() {
            return None;
        }
        let cand = pkg_dir.join(target);
        return if cand.exists() { Some(cand) } else { None };
    }
    if sub == "." {
        if let Some(main) = &info.main {
            let cand = pkg_dir.join(main);
            if cand.exists() {
                return Some(cand);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_pkg(dir: &Path, manifest_name: &str, manifest: &str, files: &[(&str, &str)]) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let mut f = std::fs::File::create(dir.join(manifest_name)).unwrap();
        f.write_all(manifest.as_bytes()).unwrap();
        for (rel, body) in files {
            let p = dir.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            let mut g = std::fs::File::create(&p).unwrap();
            g.write_all(body.as_bytes()).unwrap();
        }
        dir.to_path_buf()
    }

    fn temp_pkg(tag: &str) -> PathBuf {
        let tmp = std::env::temp_dir().join(format!("aoxn_pm_{tag}"));
        let _ = std::fs::remove_dir_all(&tmp);
        tmp
    }

    #[test]
    fn parses_json_main_entry() {
        let pkg = write_pkg(
            &temp_pkg("main_json"),
            "aoxn.json",
            r#"{"name":"p","version":"0.1.0","main":"src/lib.ax"}"#,
            &[("src/lib.ax", "def answer(): int { return 42 }")],
        );
        let got = resolve_pkg_entry(&pkg, None).unwrap();
        assert!(got.ends_with("src/lib.ax"));
    }

    #[test]
    fn parses_json_exports_subpath_string_target() {
        let pkg = write_pkg(
            &temp_pkg("exp_json"),
            "aoxn.json",
            r#"{"name":"p","exports":{"./sub":"./src/sub.ax"}}"#,
            &[("src/sub.ax", "def x(): int { return 1 }")],
        );
        let got = resolve_pkg_entry(&pkg, Some("./sub")).unwrap();
        assert!(got.ends_with("src/sub.ax"));
    }

    #[test]
    fn json_exports_conditional_object_uses_default() {
        let pkg = write_pkg(
            &temp_pkg("cond_json"),
            "aoxn.json",
            r#"{"name":"p","exports":{".":{"types":"./types.ax","default":"./src/lib.ax"}}}"#,
            &[("src/lib.ax", "def y(): int { return 9 }")],
        );
        let got = resolve_pkg_entry(&pkg, None).unwrap();
        assert!(got.ends_with("src/lib.ax"));
    }

    #[test]
    fn json_exports_present_but_missing_subpath_returns_none() {
        let pkg = write_pkg(
            &temp_pkg("miss_json"),
            "aoxn.json",
            r#"{"name":"p","exports":{".":"./src/lib.ax"}}"#,
            &[("src/lib.ax", "def z(): int { return 0 }")],
        );
        assert!(resolve_pkg_entry(&pkg, Some("./nope")).is_none());
    }

    #[test]
    fn no_manifest_returns_none() {
        let tmp = temp_pkg("none");
        std::fs::create_dir_all(&tmp).unwrap();
        assert!(resolve_pkg_entry(&tmp, None).is_none());
    }

    #[test]
    fn json_bom_tolerated() {
        let pkg = write_pkg(
            &temp_pkg("bom_json"),
            "aoxn.json",
            "\u{FEFF}{\"name\":\"p\",\"main\":\"src/lib.ax\"}",
            &[("src/lib.ax", "def q(): int { return 7 }")],
        );
        let got = resolve_pkg_entry(&pkg, None).unwrap();
        assert!(got.ends_with("src/lib.ax"));
    }

    #[test]
    fn parses_toml_main_entry() {
        let pkg = write_pkg(
            &temp_pkg("main_toml"),
            "aoxn.toml",
            "[project]\nname = \"p\"\nversion = \"0.1.0\"\nmain = \"src/lib.ax\"\n",
            &[("src/lib.ax", "def answer(): int { return 42 }")],
        );
        let got = resolve_pkg_entry(&pkg, None).unwrap();
        assert!(got.ends_with("src/lib.ax"));
    }

    #[test]
    fn parses_toml_exports_table_and_subpath() {
        let manifest = r#"
[project]
name = "p"
version = "0.1.0"
dependencies = [
  "http>=1.2",
  "other ^2",
]

[project.exports]
"." = "./src/lib.ax"
"./client" = "./src/client.ax"
"#;
        let pkg = write_pkg(
            &temp_pkg("exp_toml"),
            "aoxn.toml",
            manifest,
            &[
                ("src/lib.ax", "def root(): int { return 0 }"),
                ("src/client.ax", "def client(): int { return 1 }"),
            ],
        );
        assert!(resolve_pkg_entry(&pkg, None).unwrap().ends_with("src/lib.ax"));
        assert!(resolve_pkg_entry(&pkg, Some("./client"))
            .unwrap()
            .ends_with("src/client.ax"));
    }

    #[test]
    fn toml_exports_conditional_inline_table_uses_default() {
        let manifest = r#"
[project]
name = "p"
version = "0.1.0"
exports = { "." = { types = "./types.ax", default = "./src/lib.ax" } }
"#;
        let pkg = write_pkg(
            &temp_pkg("cond_toml"),
            "aoxn.toml",
            manifest,
            &[("src/lib.ax", "def y(): int { return 9 }")],
        );
        let got = resolve_pkg_entry(&pkg, None).unwrap();
        assert!(got.ends_with("src/lib.ax"));
    }

    #[test]
    fn toml_exports_present_but_missing_subpath_returns_none() {
        let manifest = "[project]\nname = \"p\"\nversion = \"0.1.0\"\n\n[project.exports]\n\".\" = \"./src/lib.ax\"\n";
        let pkg = write_pkg(
            &temp_pkg("miss_toml"),
            "aoxn.toml",
            manifest,
            &[("src/lib.ax", "def z(): int { return 0 }")],
        );
        assert!(resolve_pkg_entry(&pkg, Some("./nope")).is_none());
    }

    #[test]
    fn toml_is_preferred_over_legacy_json() {
        let tmp = temp_pkg("both");
        std::fs::create_dir_all(tmp.join("src")).unwrap();
        std::fs::write(
            tmp.join("aoxn.toml"),
            "[project]\nname=\"new\"\nversion=\"1.0.0\"\nmain=\"src/new.ax\"\n",
        )
        .unwrap();
        std::fs::write(
            tmp.join("aoxn.json"),
            r#"{"name":"old","main":"src/old.ax"}"#,
        )
        .unwrap();
        std::fs::write(tmp.join("src/new.ax"), "def n(): int { return 1 }").unwrap();
        std::fs::write(tmp.join("src/old.ax"), "def o(): int { return 2 }").unwrap();
        let got = resolve_pkg_entry(&tmp, None).unwrap();
        assert!(got.ends_with("src/new.ax"), "{got:?}");
    }

    #[test]
    fn toml_tool_aoxn_entry_fallback() {
        let manifest = "[project]\nname = \"p\"\nversion = \"0.1.0\"\n\n[tool.aoxn]\nmain = \"src/lib.ax\"\n";
        let pkg = write_pkg(
            &temp_pkg("tool_toml"),
            "aoxn.toml",
            manifest,
            &[("src/lib.ax", "def t(): int { return 3 }")],
        );
        assert!(resolve_pkg_entry(&pkg, None).unwrap().ends_with("src/lib.ax"));
    }
}
