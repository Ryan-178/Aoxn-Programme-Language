//! Rust bridge (v0.55.0-snapshot-5): `import "./native.rs"` compiles a self-contained
//! Rust file with rustc and binds its `#[no_mangle] extern "C"` functions as
//! ordinary `extern def` declarations.
//!
//! Shape of the feature (docs/rust-interop.md is the reference):
//!
//! * The LOADER scans the .rs file and synthesizes one `FnDecl` per exported
//!   function, so the file becomes a module like any other: star/selective/
//!   namespace imports, `aoxn symbols` and the IDE all work unchanged. The
//!   scan needs NO rustc — `check`, `c` and `symbols` run on machines without
//!   a Rust toolchain.
//! * Only `build`/`run` invoke rustc (`--crate-type staticlib`), cache the
//!   artifact under the build cache's `rust/` subdirectory, and link it plus
//!   the `native-static-libs` note rustc prints. rustc is an OPTIONAL external
//!   toolchain, exactly like clang: AOXN_RUSTC -> PATH -> ~/.cargo/bin.
//! * The ABI surface is deliberately narrow: `i64` -> `int`, `f64` -> `float`,
//!   `*const`/`*mut c_char` -> `string`, no return -> `void`. Everything else
//!   is a diagnostic, not a guess — Rust `bool` is one byte while Aoxn `bool`
//!   is C `int`, so a bool mapping would be a silent ABI bug.
//! * No codegen mirror obligation: by the time typecheck/codegen run, the
//!   bindings ARE ordinary extern FnDecls (the same trick CSS assets use).
//!   The selfhost CLI does not resolve .rs imports, same boundary as CSS.

use crate::ast::{Block, FnDecl, Param, Pos, Type};
use crate::Diag;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Whether `path` names a Rust source file (a bridge import).
pub fn is_rs(path: &Path) -> bool {
    path.extension().map(|e| e == "rs").unwrap_or(false)
}

/// Locate the rustc driver used to compile .rs imports.
/// Order: AOXN_RUSTC env -> PATH -> rustup's default `~/.cargo/bin`.
/// Mirrors `find_clang`; an unset/invalid AOXN_RUSTC falls through.
pub fn find_rustc() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("AOXN_RUSTC") {
        let path = PathBuf::from(p);
        if path.is_file() {
            return Some(path);
        }
    }
    for name in crate::paths::rustc_names() {
        if let Some(path) = crate::which(name) {
            return Some(path);
        }
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    for name in crate::paths::rustc_names() {
        let candidate = PathBuf::from(&home).join(".cargo").join("bin").join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// First line of `rustc --version`, trimmed; a marker when it cannot be run.
pub fn version(rustc: &Path) -> String {
    Command::new(rustc)
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().next().unwrap_or("").trim().to_string())
        .unwrap_or_else(|| "(version query failed)".into())
}

thread_local! {
    static VERSIONS: std::cell::RefCell<HashMap<PathBuf, String>> =
        std::cell::RefCell::new(HashMap::new());
}

/// The toolchain identity for cache keys. `rustc --version` output, NOT the
/// binary's metadata: `~/.cargo/bin/rustc` is a rustup SHIM whose path, size
/// and mtime survive every `rustup update` — only the version string notices
/// the toolchain actually changed. Memoized per resolved path so a build
/// with N .rs imports pays for one query, and an env override that resolves
/// elsewhere is a different entry.
pub fn identity(rustc: &Path) -> String {
    VERSIONS.with(|v| {
        if let Some(hit) = v.borrow().get(rustc) {
            return hit.clone();
        }
        let fresh = version(rustc);
        v.borrow_mut().insert(rustc.to_path_buf(), fresh.clone());
        fresh
    })
}

// ---- signature scan --------------------------------------------------------

/// One token with its source position (1-based line, byte column). Comments
/// are dropped; string/char literals keep their text (the ABI marker in
/// `extern "C"` is a string literal, so the scanner must see it).
#[derive(Default)]
struct Tok {
    text: String,
    line: usize,
    col: usize,
}

/// A comment/string-aware tokenizer, just enough to find top-level items.
/// It is not a Rust lexer: numbers and operators are opaque text. What it
/// must never do is mistake a `fn` inside a comment, a string or a macro
/// body for a declaration, so literals and comments are consumed whole.
fn tokenize(src: &str) -> Vec<Tok> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    let mut line = 1usize;
    let mut col = 1usize;
    // advance one byte, tracking position; columns are byte columns, which
    // is what the Aoxn lexer reports too
    fn adv(b: &[u8], i: &mut usize, line: &mut usize, col: &mut usize) {
        if b[*i] == b'\n' {
            *line += 1;
            *col = 1;
        } else {
            *col += 1;
        }
        *i += 1;
    }
    while i < b.len() {
        let (l, c) = (line, col);
        match b[i] {
            b' ' | b'\t' | b'\r' | b'\n' => adv(b, &mut i, &mut line, &mut col),
            b'/' if i + 1 < b.len() && b[i + 1] == b'/' => {
                while i < b.len() && b[i] != b'\n' {
                    adv(b, &mut i, &mut line, &mut col);
                }
            }
            b'/' if i + 1 < b.len() && b[i + 1] == b'*' => {
                // Rust block comments NEST
                let mut depth = 1usize;
                adv(b, &mut i, &mut line, &mut col);
                adv(b, &mut i, &mut line, &mut col);
                while i < b.len() && depth > 0 {
                    if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
                        depth += 1;
                        adv(b, &mut i, &mut line, &mut col);
                        adv(b, &mut i, &mut line, &mut col);
                    } else if b[i] == b'*' && i + 1 < b.len() && b[i + 1] == b'/' {
                        depth -= 1;
                        adv(b, &mut i, &mut line, &mut col);
                        adv(b, &mut i, &mut line, &mut col);
                    } else {
                        adv(b, &mut i, &mut line, &mut col);
                    }
                }
            }
            b'"' => {
                let start = i;
                adv(b, &mut i, &mut line, &mut col);
                while i < b.len() && b[i] != b'"' {
                    if b[i] == b'\\' && i + 1 < b.len() {
                        adv(b, &mut i, &mut line, &mut col);
                    }
                    adv(b, &mut i, &mut line, &mut col);
                }
                if i < b.len() {
                    adv(b, &mut i, &mut line, &mut col);
                }
                out.push(Tok { text: src[start..i].to_string(), line: l, col: c });
            }
            b'r' | b'b' if raw_string_len(b, i) > 0 => {
                let len = raw_string_len(b, i);
                let start = i;
                for _ in 0..len {
                    adv(b, &mut i, &mut line, &mut col);
                }
                out.push(Tok { text: src[start..i].to_string(), line: l, col: c });
            }
            b'\'' => {
                // char literal ('x', '\n', '\u{1F600}') vs lifetime ('a):
                // a char literal closes with a quote, a lifetime does not
                let start = i;
                adv(b, &mut i, &mut line, &mut col);
                let mut is_char = false;
                if i < b.len() && b[i] == b'\\' {
                    adv(b, &mut i, &mut line, &mut col);
                    // escape body: xHH or u{...} or one byte
                    if i < b.len() && b[i] == b'u' {
                        while i < b.len() && b[i] != b'}' && b[i] != b'\n' {
                            adv(b, &mut i, &mut line, &mut col);
                        }
                        if i < b.len() && b[i] == b'}' {
                            adv(b, &mut i, &mut line, &mut col);
                        }
                    } else {
                        if i < b.len() {
                            adv(b, &mut i, &mut line, &mut col);
                        }
                        if i < b.len() && b[i - 1] == b'x' {
                            for _ in 0..2 {
                                if i < b.len() && b[i].is_ascii_hexdigit() {
                                    adv(b, &mut i, &mut line, &mut col);
                                }
                            }
                        }
                    }
                    if i < b.len() && b[i] == b'\'' {
                        adv(b, &mut i, &mut line, &mut col);
                        is_char = true;
                    }
                } else if i + 1 < b.len() && b[i + 1] == b'\'' {
                    adv(b, &mut i, &mut line, &mut col);
                    adv(b, &mut i, &mut line, &mut col);
                    is_char = true;
                }
                if !is_char {
                    // lifetime: consume the identifier, leave the text as-is
                    while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                        adv(b, &mut i, &mut line, &mut col);
                    }
                }
                out.push(Tok { text: src[start..i].to_string(), line: l, col: c });
            }
            b'A'..=b'Z' | b'a'..=b'z' | b'_' => {
                let start = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    adv(b, &mut i, &mut line, &mut col);
                }
                out.push(Tok { text: src[start..i].to_string(), line: l, col: c });
            }
            b'0'..=b'9' => {
                let start = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    adv(b, &mut i, &mut line, &mut col);
                }
                // a float's fractional part, but not the `..` range operator
                if i + 1 < b.len() && b[i] == b'.' && b[i + 1].is_ascii_digit() {
                    adv(b, &mut i, &mut line, &mut col);
                    while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                        adv(b, &mut i, &mut line, &mut col);
                    }
                }
                out.push(Tok { text: src[start..i].to_string(), line: l, col: c });
            }
            _ => {
                // one byte of punctuation; non-ASCII bytes land here too and
                // fail the name/type checks downstream
                let start = i;
                adv(b, &mut i, &mut line, &mut col);
                out.push(Tok { text: src[start..i].to_string(), line: l, col: c });
            }
        }
    }
    out
}

/// Length of the raw string starting at `i` (`r"..."`, `r#"..."#`, `br##"..."##`),
/// or 0 when there is none. Unterminated raw strings consume to EOF, the same
/// recovery rustc's lexer would report as an error later.
fn raw_string_len(b: &[u8], i: usize) -> usize {
    let mut j = i;
    if b[j] == b'b' {
        j += 1;
    }
    if j >= b.len() || b[j] != b'r' {
        return 0;
    }
    j += 1;
    let mut hashes = 0usize;
    while j < b.len() && b[j] == b'#' {
        hashes += 1;
        j += 1;
    }
    if j >= b.len() || b[j] != b'"' {
        return 0;
    }
    j += 1;
    // find the closing quote followed by exactly `hashes` '#'s
    while j < b.len() {
        if b[j] == b'"' {
            let mut k = j + 1;
            let mut seen = 0usize;
            while seen < hashes && k < b.len() && b[k] == b'#' {
                seen += 1;
                k += 1;
            }
            if seen == hashes {
                return k - i;
            }
        }
        j += 1;
    }
    b.len() - i
}

/// Reconstruct a type's text from its tokens: `std::ffi::c_char`,
/// `*const c_char`, `()`. Spaces disappear around `::`, after `*`/`&`, and
/// inside parens so the mapping table can match exact spellings.
fn type_text(toks: &[&Tok]) -> String {
    let mut s = String::new();
    for (i, t) in toks.iter().enumerate() {
        if i > 0 {
            let prev = toks[i - 1].text.as_str();
            let cur = t.text.as_str();
            let glue = cur == ":"
                || prev == ":"
                || cur == ")"
                || prev == "("
                || prev == "*"
                || prev == "&"
                || cur == ","
                || prev == ",";
            if !glue {
                s.push(' ');
            }
        }
        s.push_str(&t.text);
    }
    s
}

/// The ABI mapping. `None` means "unsupported"; the caller turns that into a
/// diagnostic naming the type. Only exact-ABI pairs are accepted: Aoxn `int`
/// is C `long long` (i64), `float` is C `double` (f64), `string` is C `char*`
/// (c_char pointer). Rust `bool` (1 byte) and `i32` (C `int`) are NOT Aoxn
/// `bool`/`int` and must stay rejections, not conversions.
fn map_type(text: &str) -> Option<Type> {
    match text {
        "i64" => Some(Type::Int),
        "f64" => Some(Type::Float),
        "()" => Some(Type::Void),
        _ => {
            let pointee = text.strip_prefix("*const ").or_else(|| text.strip_prefix("*mut "))?;
            let pointee = pointee.strip_prefix("::").unwrap_or(pointee);
            match pointee {
                "c_char" | "std::ffi::c_char" | "core::ffi::c_char" | "std::os::raw::c_char" => Some(Type::Str),
                _ => None,
            }
        }
    }
}

fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// C keywords plus the generated entry point: an extern emitted verbatim
/// with one of these names is invalid C or hijacks the program's entry.
fn is_bad_c_symbol(name: &str) -> bool {
    name == "main" || crate::codegen_c::C_KEYWORDS.contains(&name)
}

/// Scan a self-contained .rs file for `#[no_mangle] extern "C" fn` items and
/// synthesize their `extern def` declarations. Runs at LOAD time for every
/// command (check/c/symbols included), so it must not need rustc.
///
/// Rejections are diagnostics with the .rs file's id and the offending
/// token's position. A Rust function that is not `extern "C"` is private to
/// the crate and binds nothing; an `extern "C"` function WITHOUT
/// `#[no_mangle]` is an error rather than a silence — its symbol is mangled,
/// so binding it would fail at link time with a much worse message.
pub fn scan(src: &str, file: u32) -> Result<Vec<FnDecl>, Vec<Diag>> {
    let toks = fold_attributes(tokenize(src));
    let mut errs = Vec::new();
    let mut funcs = Vec::new();

    // `include!` and friends pull other files into the crate; the bridge's
    // cache key hashes ONE file, so a multi-file crate would serve stale
    // artifacts after an edit to the included file. Reject up front.
    for w in toks.windows(2) {
        if matches!(w[0].text.as_str(), "include" | "include_str" | "include_bytes") && w[1].text == "!" {
            errs.push(Diag::at(
                "parse",
                file,
                w[0].line,
                w[0].col,
                format!(
                    "{}! pulls another file into the crate; the Rust bridge compiles ONE self-contained .rs file — inline the contents",
                    w[0].text
                ),
            ));
        }
    }

    let mut i = 0usize;
    let mut braces = 0usize;
    let mut parens = 0usize;
    // token indices of the current top-level item header (params included:
    // they sit inside parens but still belong to the header)
    let mut item: Vec<usize> = Vec::new();
    while i < toks.len() {
        let t = &toks[i];
        match t.text.as_str() {
            "{" if braces == 0 && parens == 0 => {
                scan_item(&toks, &item, false, file, &mut funcs, &mut errs);
                item.clear();
                braces += 1;
                i += 1;
            }
            "{" => {
                braces += 1;
                i += 1;
            }
            "}" => {
                braces = braces.saturating_sub(1);
                if braces == 0 {
                    item.clear();
                }
                i += 1;
            }
            "(" | "[" => {
                if braces == 0 {
                    item.push(i);
                }
                parens += 1;
                i += 1;
            }
            ")" | "]" => {
                parens = parens.saturating_sub(1);
                if braces == 0 {
                    item.push(i);
                }
                i += 1;
            }
            ";" if braces == 0 && parens == 0 => {
                scan_item(&toks, &item, true, file, &mut funcs, &mut errs);
                item.clear();
                i += 1;
            }
            _ => {
                if braces == 0 {
                    item.push(i);
                }
                i += 1;
            }
        }
    }
    if !item.is_empty() {
        scan_item(&toks, &item, true, file, &mut funcs, &mut errs);
    }
    if !errs.is_empty() {
        return Err(errs);
    }
    Ok(funcs)
}

/// Fold each `#[...]` / `#![...]` attribute run into ONE synthetic token so
/// the item walk never sees an attribute's innards as declaration tokens.
/// The text keeps the leading `#` (what `scan_item` filters on) and the
/// concatenated contents (what the `no_mangle` / `export_name` checks read).
fn fold_attributes(mut toks: Vec<Tok>) -> Vec<Tok> {
    let mut out: Vec<Tok> = Vec::with_capacity(toks.len());
    let mut i = 0;
    while i < toks.len() {
        let mut j = i + 1;
        if toks[i].text == "#" {
            if j < toks.len() && toks[j].text == "!" {
                j += 1;
            }
            if j < toks.len() && toks[j].text == "[" {
                let mut text = String::from("#");
                if toks[i + 1].text == "!" {
                    text.push('!');
                }
                let mut depth = 1usize;
                j += 1;
                while j < toks.len() && depth > 0 {
                    match toks[j].text.as_str() {
                        "[" => depth += 1,
                        "]" => depth -= 1,
                        _ => {}
                    }
                    if depth > 0 {
                        text.push_str(&toks[j].text);
                    }
                    j += 1;
                }
                text.push(']');
                out.push(Tok { text, line: toks[i].line, col: toks[i].col });
                i = j;
                continue;
            }
        }
        out.push(std::mem::take(&mut toks[i]));
        i += 1;
    }
    out
}

/// One top-level item (header tokens only). `semicolon` marks a `;`-terminated
/// item, which is what separates `mod util;` (multi-file, rejected) from
/// `mod util { ... }` (inline, fine).
fn scan_item(toks: &[Tok], item: &[usize], semicolon: bool, file: u32, funcs: &mut Vec<FnDecl>, errs: &mut Vec<Diag>) {
    if item.is_empty() {
        return;
    }
    let texts: Vec<&str> = item.iter().map(|&i| toks[i].text.as_str()).collect();
    // find the `fn` keyword; everything before it must be declaration
    // prefixes (attributes, pub, unsafe, const, async, extern + ABI string),
    // which is what keeps a `fn` inside a paren-delimited macro body out
    let fn_at = match texts.iter().position(|&t| t == "fn") {
        Some(at) => at,
        None => {
            // `mod util;` reaches another file; the bridge is single-file
            let first_kw = texts.iter().find(|&&t| !t.starts_with('#'));
            if semicolon && first_kw == Some(&"mod") {
                let pos = &toks[item[texts.iter().position(|&t| t == "mod").unwrap()]];
                errs.push(Diag::at(
                    "parse",
                    file,
                    pos.line,
                    pos.col,
                    "multi-file Rust is not supported: `mod name;` reads another file the bridge's cache key cannot see — inline the module",
                ));
            }
            return;
        }
    };
    for &t in &texts[..fn_at] {
        let ok = t.starts_with('#')
            || t.starts_with('"')
            || matches!(t, "pub" | "unsafe" | "const" | "async" | "extern" | "(" | ")")
            || is_ident(t);
        if !ok {
            return; // not a function declaration (macro body, expression, ...)
        }
    }
    let has_extern_c = texts[..fn_at].windows(2).any(|w| w[0] == "extern" && w[1] == "\"C\"");
    if !has_extern_c {
        // a private Rust fn, or a foreign ABI we do not bind
        if let Some(w) = texts[..fn_at].windows(2).find(|w| w[0] == "extern" && w[1].starts_with('"')) {
            let pos = &toks[item[fn_at]];
            errs.push(Diag::at(
                "parse",
                file,
                pos.line,
                pos.col,
                format!("unsupported ABI {} for the Rust bridge: only extern \"C\" is bound (\"C-unwind\" unwinds into frames that cannot catch it)", w[1]),
            ));
        }
        return;
    }
    let attrs: Vec<&str> = texts.iter().filter(|t| t.starts_with('#')).copied().collect();
    if attrs.iter().any(|a| a.contains("export_name")) {
        let pos = &toks[item[fn_at]];
        errs.push(Diag::at(
            "parse",
            file,
            pos.line,
            pos.col,
            "#[export_name] is not supported by the Rust bridge; rename the function and use #[no_mangle]",
        ));
        return;
    }
    if !attrs.iter().any(|a| a.contains("no_mangle")) {
        let pos = &toks[item[fn_at]];
        errs.push(Diag::at(
            "parse",
            file,
            pos.line,
            pos.col,
            format!(
                "extern \"C\" function '{}' is not exported: its symbol is mangled without #[no_mangle]; add the attribute",
                texts.get(fn_at + 1).copied().unwrap_or("?")
            ),
        ));
        return;
    }
    if texts[..fn_at].contains(&"async") {
        let pos = &toks[item[fn_at]];
        errs.push(Diag::at("parse", file, pos.line, pos.col, "an async fn returns a Future, not a C ABI value; make the export synchronous"));
        return;
    }

    // name
    let name_idx = match item.get(fn_at + 1) {
        Some(&i) => i,
        None => return,
    };
    let name = &toks[name_idx].text;
    if name == "r" && texts.get(fn_at + 1) == Some(&"r") && texts.get(fn_at + 2) == Some(&"#") {
        let pos = &toks[name_idx];
        errs.push(Diag::at("parse", file, pos.line, pos.col, "raw identifiers (r#name) are not supported by the Rust bridge"));
        return;
    }
    if !is_ident(name) || is_bad_c_symbol(name) {
        let pos = &toks[name_idx];
        errs.push(Diag::at(
            "parse",
            file,
            pos.line,
            pos.col,
            format!("'{name}' cannot be a C symbol: the bridge emits the name verbatim, so C keywords and 'main' are rejected"),
        ));
        return;
    }
    let fn_pos = Pos { line: toks[item[fn_at]].line, col: toks[item[fn_at]].col, file };
    let mut at = fn_at + 2;
    if texts.get(at) == Some(&"<") {
        let pos = &toks[name_idx];
        errs.push(Diag::at("parse", file, pos.line, pos.col, format!("generic extern \"C\" function '{name}' is not supported")));
        return;
    }
    if texts.get(at) != Some(&"(") {
        return; // malformed; rustc will report it
    }
    // params: split the paren body on top-level commas
    let open = at;
    let mut depth = 0usize;
    let mut close = None;
    for j in open..texts.len() {
        match texts[j] {
            "(" | "[" => depth += 1,
            ")" | "]" => {
                depth -= 1;
                if depth == 0 {
                    close = Some(j);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = match close {
        Some(c) => c,
        None => return,
    };
    let mut params: Vec<Param> = Vec::new();
    let mut seg_start = open + 1;
    let mut segs: Vec<(usize, usize)> = Vec::new();
    let mut d = 0usize;
    for j in (open + 1)..close {
        match texts[j] {
            "(" | "[" | "<" => d += 1,
            ")" | "]" | ">" => d = d.saturating_sub(1),
            "," if d == 0 => {
                segs.push((seg_start, j));
                seg_start = j + 1;
            }
            _ => {}
        }
    }
    if seg_start < close {
        segs.push((seg_start, close));
    }
    for (a, b) in segs {
        let seg = &texts[a..b];
        if seg.is_empty() {
            continue;
        }
        let colon = match seg.iter().position(|&t| t == ":") {
            Some(c) => c,
            None => {
                let pos = &toks[item[a]];
                errs.push(Diag::at(
                    "parse",
                    file,
                    pos.line,
                    pos.col,
                    format!("unsupported parameter in extern \"C\" function '{name}': expected `name: type` (no patterns, no variadics)"),
                ));
                return;
            }
        };
        let pname_toks = if seg[0] == "mut" { &seg[1..colon] } else { &seg[..colon] };
        if pname_toks.len() != 1 || !is_ident(pname_toks[0]) {
            let pos = &toks[item[a]];
            errs.push(Diag::at(
                "parse",
                file,
                pos.line,
                pos.col,
                format!("unsupported parameter pattern in extern \"C\" function '{name}': only a plain `name: type` is bound"),
            ));
            return;
        }
        let ty_toks: Vec<&Tok> = (a + colon + 1..b).map(|j| &toks[item[j]]).collect();
        // `name: ...` is a variadic spelled with a named binder; the bare
        // `...` segment (no colon) is caught above. Either way the C ABI the
        // bridge emits is fixed-arity, so a variadic cannot be bound.
        if !ty_toks.is_empty() && ty_toks.iter().all(|t| t.text == ".") {
            let pos = &toks[item[a]];
            errs.push(Diag::at(
                "parse",
                file,
                pos.line,
                pos.col,
                format!("unsupported parameter in extern \"C\" function '{name}': variadics (`...`) are not bound (the bridge emits a fixed-arity C prototype)"),
            ));
            return;
        }
        let ty_text = type_text(&ty_toks);
        let ty = match map_type(&ty_text) {
            Some(t) if t != Type::Void => t,
            _ => {
                let pos = ty_toks.first().map(|t| Pos { line: t.line, col: t.col, file }).unwrap_or(fn_pos);
                errs.push(Diag::at("parse", file, pos.line, pos.col, unsupported_type_msg(name, &ty_text)));
                return;
            }
        };
        let p = &toks[item[a + if seg[0] == "mut" { 1 } else { 0 }]];
        params.push(Param { name: pname_toks[0].to_string(), ty, pos: Pos { line: p.line, col: p.col, file }, default: None });
    }
    // return type
    at = close + 1;
    // a where clause may follow the params directly OR the return type
    // (`fn f(x: i64) -> i64 where i64: Copy`); scan the whole tail so the
    // second shape is rejected as a generic, not mis-read as a type
    if let Some(w) = (at..texts.len()).find(|&j| texts[j] == "where") {
        let pos = &toks[item[w]];
        errs.push(Diag::at("parse", file, pos.line, pos.col, format!("generic extern \"C\" function '{name}' is not supported (where clause)")));
        return;
    }
    let mut ret = Type::Void;
    if texts.get(at) == Some(&"-") && texts.get(at + 1) == Some(&">") {
        let ty_toks: Vec<&Tok> = (at + 2..texts.len()).map(|j| &toks[item[j]]).collect();
        let ty_text = type_text(&ty_toks);
        ret = match map_type(&ty_text) {
            Some(t) => t,
            None => {
                let pos = ty_toks.first().map(|t| Pos { line: t.line, col: t.col, file }).unwrap_or(fn_pos);
                errs.push(Diag::at("parse", file, pos.line, pos.col, unsupported_type_msg(name, &ty_text)));
                return;
            }
        };
    }
    funcs.push(FnDecl {
        name: name.to_string(),
        type_params: Vec::new(),
        len_param: None,
        params,
        ret,
        body: Block { stmts: Vec::new() },
        is_extern: true,
        pos: fn_pos,
    });
}

fn unsupported_type_msg(fn_name: &str, ty: &str) -> String {
    if ty == "bool" {
        return format!(
            "unsupported type 'bool' in extern \"C\" signature of '{fn_name}': Rust bool is 1 byte but Aoxn bool is C int; use i64 (0/1)"
        );
    }
    format!(
        "unsupported type '{ty}' in extern \"C\" signature of '{fn_name}': the bridge maps i64 -> int, f64 -> float, *const/*mut c_char -> string, () -> void"
    )
}

// ---- rustc invocation + artifact cache --------------------------------------

/// One compiled .rs import: the staticlib to link and the native libraries
/// rustc's `native-static-libs` note says it needs.
pub struct RustLib {
    pub source: PathBuf,
    pub artifact: PathBuf,
    /// bare names for `-l` (the note's `-lfoo` / `foo.lib` tokens, normalized)
    pub native_libs: Vec<String>,
    /// note tokens passed to the linker verbatim: anything that is neither
    /// `-l` nor `.lib`, plus `/defaultlib:NAME` translated to `-Wl,/DEFAULTLIB:NAME`
    pub raw_link_args: Vec<String>,
}

/// Classify the tokens of rustc's `native-static-libs` note. Windows prints
/// `kernel32.lib advapi32.lib ...`, Linux prints `-lgcc_s -lutil ...`; both
/// normalize to bare names the link step's `-l` mechanism already handles.
fn classify_native_tokens(tokens: &[String]) -> (Vec<String>, Vec<String>) {
    let mut libs = Vec::new();
    let mut raw = Vec::new();
    for tok in tokens {
        if let Some(name) = tok.strip_prefix("-l") {
            libs.push(name.to_string());
        } else if let Some(name) = tok.strip_suffix(".lib") {
            libs.push(name.to_string());
        } else if let Some(name) = tok.strip_prefix("/defaultlib:") {
            // A MSVC linker DIRECTIVE, not an input library. Translating it to
            // `-l<name>` adds the dynamic CRT import lib (msvcrt.lib) as a
            // primary input, which shadows the static libcmt that provides
            // `printf` and breaks every link with "undefined symbol: printf".
            // Forward it as the defaultlib directive rustc actually meant; the
            // linker searches defaultlibs after explicit inputs, so the CRT
            // clang already chose still wins.
            raw.push(format!("-Wl,/DEFAULTLIB:{name}"));
        } else {
            raw.push(tok.clone());
        }
    }
    (libs, raw)
}

fn native_tokens_from_stderr(stderr: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in stderr.lines() {
        if let Some(at) = line.find("native-static-libs:") {
            out.extend(line[at + "native-static-libs:".len()..].split_whitespace().map(|s| s.to_string()));
        }
    }
    out
}

/// The artifact cache lives UNDER the build cache (`<cache>/rust/<key>/`),
/// never as loose files in it: `prune_cache` only recognizes 16-hex FILE
/// names, so anything else written there would leak forever.
fn rust_cache_root() -> PathBuf {
    let base = if let Ok(dir) = std::env::var("AOXN_CACHE_DIR") {
        PathBuf::from(dir)
    } else {
        let local = std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join("target").join("cache");
        if std::fs::create_dir_all(&local).is_ok() {
            local
        } else {
            std::env::temp_dir().join("aoxn-cache")
        }
    };
    let root = base.join("rust");
    let _ = std::fs::create_dir_all(&root);
    root
}

fn artifact_key(rs: &Path, src: &[u8], rustc: &Path, opt_level: u8) -> String {
    use std::hash::{BuildHasher, Hash, Hasher};
    let mut h = crate::hashing::FastBuild.build_hasher();
    h.write(b"aoxn-rust-staticlib-v1");
    rs.display().to_string().hash(&mut h);
    crate::hashing::digest_bytes(src).hash(&mut h);
    rustc.display().to_string().hash(&mut h);
    identity(rustc).hash(&mut h);
    opt_level.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// Keep the artifact cache bounded (approximate LRU, mirroring the exe
/// cache's prune): only published 16-hex key directories are candidates,
/// never the `<key>.<pid>` staging directory of a concurrent build.
fn prune(root: &Path) {
    const MAX_ENTRIES: usize = 32;
    let Ok(entries) = std::fs::read_dir(root) else { return };
    let mut dirs: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let name = path.file_name()?.to_string_lossy().into_owned();
            if name.len() != 16 || !name.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            let md = e.metadata().ok()?;
            if !md.is_dir() {
                return None;
            }
            Some((md.modified().unwrap_or(std::time::UNIX_EPOCH), path))
        })
        .collect();
    if dirs.len() <= MAX_ENTRIES {
        return;
    }
    dirs.sort_by_key(|(t, _)| *t);
    for (_, path) in dirs.iter().take(dirs.len() - MAX_ENTRIES) {
        let _ = std::fs::remove_dir_all(path);
    }
}

/// Compile one .rs file into a cached staticlib. The key covers the file's
/// content, the toolchain identity and the optimization level, so an
/// unchanged rebuild never spawns rustc twice. `AOXN_NO_CACHE=1` bypasses
/// the cache in both directions.
pub fn compile_staticlib(rs: &Path, opt_level: u8) -> Result<RustLib, Vec<Diag>> {
    let rustc = find_rustc().ok_or_else(|| {
        vec![Diag::at(
            "cc",
            u32::MAX,
            0,
            0,
            "cannot find rustc for a .rs import. Install Rust (https://rustup.rs) \
             or set AOXN_RUSTC to the rustc executable. `check`/`c`/`symbols` work without it.",
        )]
    })?;
    let canonical = std::fs::canonicalize(rs)
        .map_err(|e| vec![Diag::at("io", u32::MAX, 0, 0, format!("cannot open '{}': {}", rs.display(), crate::io_reason_for(&e, rs, crate::IoTarget::File)))])?;
    let src = std::fs::read(&canonical)
        .map_err(|e| vec![Diag::at("io", u32::MAX, 0, 0, format!("cannot read '{}': {}", rs.display(), crate::io_reason_for(&e, rs, crate::IoTarget::File)))])?;
    let cache_on = std::env::var("AOXN_NO_CACHE").is_err();
    let key = artifact_key(&canonical, &src, &rustc, opt_level);
    let root = rust_cache_root();
    let dir = root.join(&key);
    let lib_name = format!("aoxnrust{}", crate::platform::staticlib_ext());
    let artifact = dir.join(&lib_name);
    let note_file = dir.join("native-libs.txt");
    if cache_on && artifact.is_file() && note_file.is_file() {
        if let Ok(note) = std::fs::read_to_string(&note_file) {
            let tokens: Vec<String> = note.lines().map(|l| l.to_string()).collect();
            let (native_libs, raw_link_args) = classify_native_tokens(&tokens);
            return Ok(RustLib { source: canonical, artifact, native_libs, raw_link_args });
        }
    }
    // stage under a unique name, then publish by rename, so concurrent
    // invocations never link a half-written artifact (same pattern as the
    // exe cache)
    let staging = root.join(format!("{key}.{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)
        .map_err(|e| vec![Diag::at("io", u32::MAX, 0, 0, format!("cannot create '{}': {}", staging.display(), crate::io_reason_for(&e, &staging, crate::IoTarget::Dir)))])?;
    let staged_lib = staging.join(&lib_name);
    let out = Command::new(&rustc)
        .arg("--edition=2021")
        .arg("--crate-type")
        .arg("staticlib")
        .arg("--crate-name")
        .arg("aoxnrust")
        .arg("-C")
        .arg(format!("opt-level={opt_level}"))
        .arg("--print")
        .arg("native-static-libs")
        .arg(&canonical)
        .arg("-o")
        .arg(&staged_lib)
        .output()
        .map_err(|e| {
            vec![Diag::at("cc", u32::MAX, 0, 0, format!("failed to spawn {}: {}", rustc.display(), crate::io_reason_for(&e, &rustc, crate::IoTarget::File)))]
        })?;
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    if !out.status.success() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(vec![Diag::at(
            "cc",
            u32::MAX,
            0,
            0,
            format!("rustc rejected '{}':\n{}", rs.display(), stderr.trim_end()),
        )]);
    }
    let tokens = native_tokens_from_stderr(&stderr);
    // surface rustc's own warnings, but not the two-line native-static-libs
    // note the link step consumes — that is machine output, not user-facing
    let warnings: Vec<&str> = stderr
        .lines()
        .filter(|l| !l.contains("native-static-libs:") && !l.contains("link against the following native artifacts"))
        .collect();
    let warnings = warnings.join("\n");
    if !warnings.trim().is_empty() {
        eprintln!("{}", warnings.trim_end());
    }
    let note = tokens.join("\n");
    let staged_note = staging.join("native-libs.txt");
    if let Err(e) = std::fs::write(&staged_note, &note) {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(vec![Diag::at("io", u32::MAX, 0, 0, format!("cannot write '{}': {}", staged_note.display(), crate::io_reason_for(&e, &staged_note, crate::IoTarget::File)))]);
    }
    let (native_libs, raw_link_args) = classify_native_tokens(&tokens);
    if cache_on {
        if std::fs::rename(&staging, &dir).is_ok() {
            prune(&root);
        } else {
            // another invocation published first; its artifact is identical
            let _ = std::fs::remove_dir_all(&staging);
            if !artifact.is_file() || !note_file.is_file() {
                return Err(vec![Diag::internal(format!("rust artifact cache race lost for '{}'", rs.display()))]);
            }
        }
        Ok(RustLib { source: canonical, artifact, native_libs, raw_link_args })
    } else {
        // cache disabled: link straight out of the staging dir
        Ok(RustLib { source: canonical, artifact: staged_lib, native_libs, raw_link_args })
    }
}