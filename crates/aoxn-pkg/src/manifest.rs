//! Package manifests: `aoxn.toml` (current) and `aoxn.json` (legacy).
//!
//! `aoxn.toml` follows the shape of Python's `pyproject.toml`:
//!
//! ```toml
//! [project]
//! name = "my-app"
//! version = "0.1.0"
//! description = "optional"
//! main = "src/main.ax"
//! types = "src/types.ax"
//! dependencies = [
//!   "http>=1.2,<2",
//!   "json ^1",
//!   "my-utils @ path:../my-utils",
//! ]
//!
//! [project.exports]
//! "." = "src/lib.ax"
//! "./client" = "src/client.ax"
//!
//! [project.optional-dependencies]
//! dev = ["test-harness>=2"]
//!
//! [tool.aoxn.dependencies]
//! json = { registry = "mirror" }
//!
//! [tool.aoxn.overrides]
//! http = "=1.4.2"
//!
//! [tool.aoxn.registries.default]
//! url = "https://github.com/you/aoxn-registry"
//!
//! [tool.aoxn.workspace]
//! members = ["packages/*"]
//! ```
//!
//! Dependencies are PEP 508-flavoured strings: `name`, `name>=1.2`, `name ^1`
//! (semver spelling), or a local path written `name @ path:../name` (also
//! accepts `file:`). Registry hints that a string cannot carry live in
//! `[tool.aoxn.dependencies]`. Dev tooling is the `dev` extra, mirroring
//! `pip install -e ".[dev]"`.
//!
//! The legacy `aoxn.json` format is still read (new files are TOML) so
//! existing projects and published packages keep working. `aoxn migrate`
//! converts a project in place. `save` follows the target file's extension,
//! which is what lets commands edit a legacy project without silently
//! changing its format.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::errors::PkgError;

pub const FILE_NAME: &str = "aoxn.toml";
pub const LEGACY_FILE_NAME: &str = "aoxn.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main: Option<String>,
    /// TypeScript type-entry file (e.g. `"src/types.ax"`). Resolved by the
    /// compiler's manifest reader but not yet enforced by the TS checker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub types: Option<String>,
    /// Subpath entry map (npm-style): `{"." : "src/lib.ax", "./client":
    /// "src/client.ax"}`. When present, gates all package imports.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub exports: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dependencies: BTreeMap<String, DependencySpec>,
    /// tooling-only dependencies; resolved with `dependencies` into one
    /// lockfile but excluded from `aoxn install --prod` (v0.31.0)
    #[serde(
        default,
        rename = "devDependencies",
        alias = "dev_dependencies",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub dev_dependencies: BTreeMap<String, DependencySpec>,
    /// forced requirements, applied wherever the package appears in the
    /// graph (`{"http": "1.4.2"}`) — v0.31.0
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub overrides: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<WorkspaceDecl>,
    /// named registries; the key `default` is used when a dependency has no
    /// explicit `registry` hint. Values are git URLs (kind inferred),
    /// directory paths with `{"kind": "dir"}`, or `http://` URLs (kind
    /// inferred; read-only) with an optional explicit `{"kind": "http"}`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub registries: BTreeMap<String, RegistryDecl>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryDecl {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>, // "git" | "dir" | "http"
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceDecl {
    /// glob patterns relative to this manifest's directory (`packages/*`)
    pub members: Vec<String>,
}

/// One entry in `dependencies`. Either a semver requirement string
/// (registry dependency) or a local path (`name @ path:../name`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencySpec {
    Registry {
        req: String,
        registry: Option<String>,
    },
    Path {
        path: PathBuf,
    },
}

impl Serialize for DependencySpec {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            DependencySpec::Registry { req, registry: None } => s.serialize_str(req),
            DependencySpec::Registry { req, registry: Some(r) } => {
                use serde::ser::SerializeMap;
                let mut map = s.serialize_map(Some(2))?;
                map.serialize_entry("version", req)?;
                map.serialize_entry("registry", r)?;
                map.end()
            }
            DependencySpec::Path { path } => {
                use serde::ser::SerializeMap;
                let mut map = s.serialize_map(Some(1))?;
                map.serialize_entry("path", &path.to_string_lossy())?;
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for DependencySpec {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = DependencySpec;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("version requirement string or {\"path\": ...} / {\"version\": ..., \"registry\": ...}")
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(DependencySpec::Registry {
                    req: v.to_string(),
                    registry: None,
                })
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut m: A) -> Result<Self::Value, A::Error> {
                let mut path = None;
                let mut version = None;
                let mut registry = None;
                while let Some(k) = m.next_key::<String>()? {
                    match k.as_str() {
                        "path" => path = Some(m.next_value::<String>()?),
                        "version" => version = Some(m.next_value::<String>()?),
                        "registry" => registry = Some(m.next_value::<String>()?),
                        other => return Err(serde::de::Error::unknown_field(other, &["path", "version", "registry"])),
                    }
                }
                if let Some(p) = path {
                    return Ok(DependencySpec::Path { path: PathBuf::from(p) });
                }
                match version {
                    Some(v) => Ok(DependencySpec::Registry { req: v, registry }),
                    None => Err(serde::de::Error::missing_field("version")),
                }
            }
        }
        d.deserialize_any(V)
    }
}

/// Package name rule: lowercase letters, digits, `-`, `_`; 1-64 chars;
/// must not start/end with `-` (keeps `name@version` keys unambiguous).
pub fn validate_name(name: &str) -> Result<(), PkgError> {
    let bad = |why: &str| Err(PkgError::Manifest(format!("invalid package name `{name}`: {why}")));
    if name.is_empty() || name.len() > 64 {
        return bad("must be 1-64 characters");
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return bad("only lowercase a-z, 0-9, '-' and '_' allowed");
    }
    if name.starts_with('-') || name.ends_with('-') {
        return bad("must not start or end with '-'");
    }
    Ok(())
}

/// Parse a PEP 508-flavoured dependency string into `(name, spec)`.
///
/// Accepted:
/// * `http` — any version (`*`)
/// * `http>=1.2,<2`, `http ^1.2`, `http==1.2.3`, `http~=1.2`
/// * `my-utils @ path:../my-utils` (also `file:`)
/// * `json @ registry:mirror` — registry hint with any version
/// * `http@^1.2` — the pre-TOML CLI spelling, still accepted
///
/// Extras and environment markers are rejected: Aoxn has no extras and no
/// Python environment to evaluate markers against.
pub fn parse_dependency(spec: &str) -> Result<(String, DependencySpec), PkgError> {
    let raw = spec.trim();
    if raw.is_empty() {
        return Err(PkgError::Manifest("empty dependency string".into()));
    }
    if raw.contains(';') {
        return Err(PkgError::Manifest(format!(
            "dependency `{raw}`: environment markers are not supported"
        )));
    }
    if raw.contains('[') || raw.contains(']') {
        return Err(PkgError::Manifest(format!(
            "dependency `{raw}`: extras are not supported (Aoxn imports whole modules)"
        )));
    }

    let (name, after_at) = match raw.split_once('@') {
        Some((n, rest)) => (n.trim(), Some(rest.trim())),
        None => (raw, None),
    };
    let (name_part, req_part) = match after_at {
        Some(rest) => (name, Some(rest)),
        None => split_requirement(raw),
    };
    validate_name(name_part)?;

    if let Some(rest) = req_part {
        if rest.is_empty() {
            return Err(PkgError::Manifest(format!(
                "dependency `{raw}`: empty version or reference"
            )));
        }
        if let Some(p) = rest.strip_prefix("path:").or_else(|| rest.strip_prefix("file:")) {
            let p = p.trim();
            if p.is_empty() {
                return Err(PkgError::Manifest(format!(
                    "dependency `{raw}`: empty path"
                )));
            }
            return Ok((
                name_part.to_string(),
                DependencySpec::Path { path: PathBuf::from(p) },
            ));
        }
        if let Some(r) = rest.strip_prefix("registry:") {
            let r = r.trim();
            if r.is_empty() {
                return Err(PkgError::Manifest(format!(
                    "dependency `{raw}`: empty registry name"
                )));
            }
            return Ok((
                name_part.to_string(),
                DependencySpec::Registry { req: "*".into(), registry: Some(r.to_string()) },
            ));
        }
        if rest.contains("://") {
            return Err(PkgError::Manifest(format!(
                "dependency `{raw}`: URL references are not supported; use `{name_part} @ path:<dir>` for a local package"
            )));
        }
        let req = normalize_req(rest)?;
        return Ok((
            name_part.to_string(),
            DependencySpec::Registry { req, registry: None },
        ));
    }

    Ok((
        name_part.to_string(),
        DependencySpec::Registry { req: "*".into(), registry: None },
    ))
}

/// Split `http>=1.2` into `("http", Some(">=1.2"))` at the first comparator
/// character. Package names may contain `-` and `_`, neither of which is a
/// comparator.
fn split_requirement(raw: &str) -> (&str, Option<&str>) {
    const OPS: &[char] = &['<', '>', '=', '!', '~', '^', '*'];
    match raw.find(OPS) {
        Some(i) => (raw[..i].trim(), Some(raw[i..].trim())),
        None => (raw.trim(), None),
    }
}

/// PEP 508 spellings (`==`, `~=`) become semver spellings; a bare version
/// (`1.2`) means the caret range, matching `semver` and the old CLI default.
fn normalize_req(raw: &str) -> Result<String, PkgError> {
    let mut r = raw.trim();
    if r.starts_with('(') && r.ends_with(')') {
        r = r[1..r.len() - 1].trim();
    }
    let r = if let Some(rest) = r.strip_prefix("~=") {
        format!("~{rest}")
    } else if let Some(rest) = r.strip_prefix("==") {
        format!("={rest}")
    } else {
        r.to_string()
    };
    let r: String = r.chars().filter(|c| !c.is_whitespace()).collect();
    let r = if r.is_empty() {
        "*".to_string()
    } else if r.starts_with(|c: char| c.is_ascii_digit())
        && !r.contains('*')
        && !r.contains('x')
        && !r.contains('X')
    {
        format!("^{r}")
    } else {
        r
    };
    if r.parse::<semver::VersionReq>().is_err() {
        return Err(PkgError::Manifest(format!(
            "invalid version requirement `{raw}` (semver expected, e.g. >=1.2,<2 or ^1.2)"
        )));
    }
    Ok(r)
}

impl DependencySpec {
    /// Serialize back to a `[project].dependencies` string. Registry hints
    /// are emitted separately into `[tool.aoxn.dependencies]` because a
    /// PEP 508 string cannot carry them.
    pub fn to_spec_string(&self, name: &str) -> String {
        match self {
            DependencySpec::Registry { req, .. } => {
                if req == "*" {
                    name.to_string()
                } else {
                    // Legacy manifests may store a bare version (`"1.4.2"`),
                    // which `semver` treats as the caret range. Spell it in
                    // the TOML form so a reload parses the same requirement.
                    let req = normalize_req(req).unwrap_or_else(|_| req.trim().to_string());
                    // `http>=1.2` is PEP 508's spelling; caret/tilde keep a
                    // space so `http ^1.2` does not read as part of the name.
                    if req.starts_with('^') || req.starts_with('~') {
                        format!("{name} {req}")
                    } else {
                        format!("{name}{req}")
                    }
                }
            }
            DependencySpec::Path { path } => {
                format!("{name} @ path:{}", path.display())
            }
        }
    }
}

// ---------------------------------------------------------------------------
// TOML document model

#[derive(Debug, Serialize, Deserialize)]
struct TomlDoc {
    project: TomlProject,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tool: Option<TomlTool>,
    /// PEP 735 dependency groups. `dev` is the only group with a meaning in
    /// Aoxn today; accepted as an alias for the `dev` extra so modern
    /// pyproject files work unchanged.
    #[serde(default, rename = "dependency-groups", skip_serializing_if = "BTreeMap::is_empty")]
    dependency_groups: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct TomlProject {
    name: String,
    version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    main: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    types: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    exports: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    dependencies: Vec<String>,
    #[serde(
        default,
        rename = "optional-dependencies",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    optional_dependencies: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct TomlTool {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    aoxn: Option<TomlAoxn>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct TomlAoxn {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    dependencies: BTreeMap<String, TomlDepHint>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    overrides: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    workspace: Option<WorkspaceDecl>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    registries: BTreeMap<String, RegistryDecl>,
    // Read-only fallbacks for manifests that put entry points under
    // `[tool.aoxn]` instead of `[project]` (the npm-style layout).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    main: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    types: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    exports: BTreeMap<String, String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct TomlDepHint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    registry: Option<String>,
}

impl Manifest {
    /// The preferred manifest path for a directory (`aoxn.toml`).
    pub fn preferred_path(dir: &Path) -> PathBuf {
        dir.join(FILE_NAME)
    }

    pub fn legacy_path(dir: &Path) -> PathBuf {
        dir.join(LEGACY_FILE_NAME)
    }

    /// The manifest that actually exists here: TOML first, legacy JSON
    /// second. `None` when the directory has neither.
    pub fn locate(dir: &Path) -> Option<PathBuf> {
        let toml = Manifest::preferred_path(dir);
        if toml.is_file() {
            return Some(toml);
        }
        let legacy = Manifest::legacy_path(dir);
        if legacy.is_file() {
            return Some(legacy);
        }
        None
    }

    /// The path commands should read and write. Existing files keep their
    /// format; a project with no manifest gets the new `aoxn.toml` name.
    pub fn path_for(dir: &Path) -> PathBuf {
        Manifest::locate(dir).unwrap_or_else(|| Manifest::preferred_path(dir))
    }

    pub fn exists_in(dir: &Path) -> bool {
        Manifest::locate(dir).is_some()
    }

    /// Load the manifest in `dir`, whichever format it uses.
    pub fn load_dir(dir: &Path) -> Result<Manifest, PkgError> {
        let path = Manifest::locate(dir).ok_or_else(|| {
            PkgError::Manifest(format!(
                "no {} or {} found in {}",
                FILE_NAME,
                LEGACY_FILE_NAME,
                dir.display()
            ))
        })?;
        Manifest::load(&path)
    }

    pub fn load(path: &Path) -> Result<Manifest, PkgError> {
        let text = std::fs::read_to_string(path).map_err(|e| {
            PkgError::Manifest(format!(
                "cannot read {}: {}",
                path.display(),
                crate::errors::io_reason_for(&e, path, crate::errors::IoTarget::File)
            ))
        })?;
        // PowerShell's UTF-8 writer adds a BOM; serde_json rejects it and a
        // TOML parser would see a stray key. Strip it once here.
        let text = text.strip_prefix('\u{FEFF}').unwrap_or(&text);
        let m = if is_json_path(path) {
            serde_json::from_str::<Manifest>(text)
                .map_err(|e| PkgError::Manifest(format!("invalid {}: {e}", path.display())))?
        } else {
            let doc: TomlDoc = toml::from_str(text)
                .map_err(|e| PkgError::Manifest(format!("invalid {}: {e}", path.display())))?;
            manifest_from_toml(doc)
                .map_err(|e| PkgError::Manifest(format!("invalid {}: {e}", path.display())))?
        };
        m.validate()?;
        Ok(m)
    }

    pub fn validate(&self) -> Result<(), PkgError> {
        validate_name(&self.name)?;
        if self.version.parse::<semver::Version>().is_err() {
            return Err(PkgError::Manifest(format!(
                "invalid version `{}` (semver expected, e.g. 1.2.3)",
                self.version
            )));
        }
        for (dep, spec) in self.dependencies.iter().chain(&self.dev_dependencies) {
            // The KEY becomes a directory: `aox_modules/<name>`, and that
            // directory is removed RECURSIVELY before extraction and before
            // any integrity check. A key like `../victim` therefore deleted
            // `app/victim` (reproduced). `validate_name` is the same strict
            // allowlist that governs the project's own name — lowercase
            // a-z, 0-9, `-`, `_` — which cannot spell a separator, a drive
            // letter or a dot segment, so no key can escape.
            validate_name(dep)?;
            match spec {
                DependencySpec::Registry { req, registry: _ } => {
                    if req.parse::<semver::VersionReq>().is_err() {
                        return Err(PkgError::Manifest(format!(
                            "invalid requirement for dependency `{dep}`: `{req}`"
                        )));
                    }
                }
                DependencySpec::Path { path } => {
                    if path.as_os_str().is_empty() {
                        return Err(PkgError::Manifest(format!(
                            "dependency `{dep}`: empty path"
                        )));
                    }
                }
            }
        }
        for (pkg, req) in &self.overrides {
            if req.parse::<semver::VersionReq>().is_err() {
                return Err(PkgError::Manifest(format!(
                    "invalid override for `{pkg}`: `{req}`"
                )));
            }
            validate_name(pkg)?;
        }
        Ok(())
    }

    /// A valid but empty manifest, for contexts that need a `Ctx` without a
    /// project on disk (`aoxn trust bootstrap`).
    pub fn synthetic() -> Manifest {
        Manifest {
            name: "aoxn-pkg-probe".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            main: None,
            types: None,
            exports: BTreeMap::new(),
            description: None,
            dependencies: BTreeMap::new(),
            dev_dependencies: BTreeMap::new(),
            overrides: BTreeMap::new(),
            workspace: None,
            registries: BTreeMap::new(),
        }
    }

    /// Write the manifest. The format follows the path's extension, so
    /// editing a legacy project keeps `aoxn.json`, while `init`/`migrate`
    /// write `aoxn.toml`.
    pub fn save(&self, path: &Path) -> Result<(), PkgError> {
        let text = if is_json_path(path) {
            serde_json::to_string_pretty(self)? + "\n"
        } else {
            self.to_toml_string()?
        };
        std::fs::write(path, text)?;
        Ok(())
    }

    pub fn to_toml_string(&self) -> Result<String, PkgError> {
        let doc = manifest_to_toml(self);
        let mut text = toml::to_string_pretty(&doc)
            .map_err(|e| PkgError::Manifest(format!("cannot serialize aoxn.toml: {e}")))?;
        if !text.ends_with('\n') {
            text.push('\n');
        }
        Ok(text)
    }

    /// The declared entry file for the package root (subpath `"."`).
    /// Resolution order mirrors the compiler's manifest reader: `exports["."]`
    /// first, then `main`, then the conventional probe order (`index.ax` then
    /// `main.ax`, also `.ts`/`.tsx`). Returns a path relative to the package
    /// root, or None when nothing exists on disk.
    pub fn entry_file(&self, pkg_dir: &Path) -> Option<PathBuf> {
        if let Some(target) = self.exports.get(".") {
            let p = PathBuf::from(target);
            return if pkg_dir.join(&p).exists() { Some(p) } else { None };
        }
        if let Some(main) = &self.main {
            let p = PathBuf::from(main);
            return if pkg_dir.join(&p).exists() { Some(p) } else { None };
        }
        for cand in ["index.ax", "index.ts", "index.tsx", "main.ax", "main.ts", "main.tsx"] {
            if pkg_dir.join(cand).exists() {
                return Some(PathBuf::from(cand));
            }
        }
        None
    }
}

fn is_json_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("json"))
}

fn manifest_to_toml(m: &Manifest) -> TomlDoc {
    let mut deps = Vec::new();
    let mut dev_deps = Vec::new();
    let mut hints: BTreeMap<String, TomlDepHint> = BTreeMap::new();

    fn push_table(
        out: &mut Vec<String>,
        hints: &mut BTreeMap<String, TomlDepHint>,
        table: &BTreeMap<String, DependencySpec>,
    ) {
        for (name, spec) in table {
            out.push(spec.to_spec_string(name));
            if let DependencySpec::Registry { registry: Some(r), .. } = spec {
                hints.entry(name.clone()).or_default().registry = Some(r.clone());
            }
        }
    }
    push_table(&mut deps, &mut hints, &m.dependencies);
    push_table(&mut dev_deps, &mut hints, &m.dev_dependencies);

    let mut optional = BTreeMap::new();
    if !dev_deps.is_empty() {
        optional.insert("dev".to_string(), dev_deps);
    }

    let aoxn = if hints.is_empty()
        && m.overrides.is_empty()
        && m.workspace.is_none()
        && m.registries.is_empty()
    {
        None
    } else {
        Some(TomlAoxn {
            dependencies: hints,
            overrides: m.overrides.clone(),
            workspace: m.workspace.clone(),
            registries: m.registries.clone(),
            main: None,
            types: None,
            exports: BTreeMap::new(),
        })
    };

    TomlDoc {
        project: TomlProject {
            name: m.name.clone(),
            version: m.version.clone(),
            description: m.description.clone(),
            main: m.main.clone(),
            types: m.types.clone(),
            exports: m.exports.clone(),
            dependencies: deps,
            optional_dependencies: optional,
        },
        tool: aoxn.map(|a| TomlTool { aoxn: Some(a) }),
        dependency_groups: BTreeMap::new(),
    }
}

fn manifest_from_toml(doc: TomlDoc) -> Result<Manifest, PkgError> {
    let p = doc.project;
    let tool = doc.tool.and_then(|t| t.aoxn).unwrap_or_default();

    let mut dependencies = BTreeMap::new();
    for raw in &p.dependencies {
        let (name, spec) = parse_dependency(raw)?;
        if dependencies.insert(name.clone(), spec).is_some() {
            return Err(PkgError::Manifest(format!(
                "dependency `{name}` is declared twice in [project].dependencies"
            )));
        }
    }

    let mut dev_dependencies = BTreeMap::new();
    for (group, specs) in &p.optional_dependencies {
        if group != "dev" {
            return Err(PkgError::Manifest(format!(
                "optional-dependency group `{group}` is not supported; Aoxn recognizes only `dev`"
            )));
        }
        for raw in specs {
            let (name, spec) = parse_dependency(raw)?;
            if dev_dependencies.insert(name.clone(), spec).is_some() {
                return Err(PkgError::Manifest(format!(
                    "dependency `{name}` is declared twice in [project.optional-dependencies].dev"
                )));
            }
        }
    }
    for (group, specs) in &doc.dependency_groups {
        if group != "dev" {
            return Err(PkgError::Manifest(format!(
                "dependency group `{group}` is not supported; Aoxn recognizes only `dev`"
            )));
        }
        for raw in specs {
            let (name, spec) = parse_dependency(raw)?;
            dev_dependencies.insert(name, spec);
        }
    }

    // Registry hints and path overrides. A hint may also introduce a
    // dependency the project list omitted, which is how poetry-style
    // `[tool.aoxn.dependencies]` manifests are read.
    for (name, hint) in &tool.dependencies {
        validate_name(name)?;
        let version = match &hint.version {
            Some(v) => Some(normalize_req(v)?),
            None => None,
        };
        let path = hint
            .path
            .as_ref()
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty());
        if path.is_some() && version.is_some() {
            return Err(PkgError::Manifest(format!(
                "dependency `{name}`: [tool.aoxn.dependencies] cannot combine `path` with `version`"
            )));
        }
        let override_spec = match (&path, &version) {
            (Some(p), _) => Some(DependencySpec::Path { path: PathBuf::from(p) }),
            (None, Some(req)) => Some(DependencySpec::Registry {
                req: req.clone(),
                registry: hint.registry.clone(),
            }),
            (None, None) => None,
        };
        let tables = [&mut dependencies, &mut dev_dependencies];
        let mut found = false;
        for table in tables {
            if let Some(existing) = table.get_mut(name) {
                found = true;
                if let Some(spec) = &override_spec {
                    *existing = spec.clone();
                } else {
                    match existing {
                        DependencySpec::Registry { registry, .. } => {
                            if let Some(r) = &hint.registry {
                                *registry = Some(r.clone());
                            }
                        }
                        DependencySpec::Path { .. } => {
                            if hint.registry.is_some() {
                                return Err(PkgError::Manifest(format!(
                                    "dependency `{name}` is a path dependency and cannot be bound to a registry"
                                )));
                            }
                        }
                    }
                }
            }
        }
        if !found {
            let spec = override_spec.unwrap_or(DependencySpec::Registry {
                req: version.clone().unwrap_or_else(|| "*".into()),
                registry: hint.registry.clone(),
            });
            dependencies.insert(name.clone(), spec);
        }
    }

    let main = p.main.or(tool.main);
    let types = p.types.or(tool.types);
    let mut exports = tool.exports;
    exports.extend(p.exports);

    let m = Manifest {
        name: p.name,
        version: p.version,
        main,
        types,
        exports,
        description: p.description,
        dependencies,
        dev_dependencies,
        overrides: tool.overrides,
        workspace: tool.workspace,
        registries: tool.registries,
    };
    m.validate()?;
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legacy(json: &str) -> Manifest {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn parses_plain_and_object_deps() {
        let m = legacy(
            r#"{
                "name": "app",
                "version": "0.1.0",
                "dependencies": {
                    "http": "^1.2",
                    "json": {"version": "^1", "registry": "mirror"},
                    "local-lib": {"path": "../local-lib"}
                }
            }"#,
        );
        assert_eq!(
            m.dependencies.get("http"),
            Some(&DependencySpec::Registry { req: "^1.2".into(), registry: None })
        );
        assert_eq!(
            m.dependencies.get("local-lib"),
            Some(&DependencySpec::Path { path: PathBuf::from("../local-lib") })
        );
    }

    #[test]
    fn rejects_bad_names() {
        assert!(validate_name("MyPkg").is_err());
        assert!(validate_name("-x").is_err());
        assert!(validate_name("a b").is_err());
        assert!(validate_name("ok_name-2").is_ok());
    }

    #[test]
    fn legacy_json_roundtrip_is_stable() {
        let m = legacy(r#"{"name":"a","version":"1.0.0","dependencies":{"b":"^0.1"}}"#);
        let text = serde_json::to_string_pretty(&m).unwrap();
        let m2 = legacy(&text);
        assert_eq!(serde_json::to_string_pretty(&m2).unwrap(), text);
    }

    #[test]
    fn parses_exports_and_types() {
        let m = legacy(
            r#"{
                "name": "lib",
                "version": "1.0.0",
                "types": "src/types.ax",
                "exports": {
                    ".": "src/lib.ax",
                    "./client": "src/client.ax"
                }
            }"#,
        );
        assert_eq!(m.types.as_deref(), Some("src/types.ax"));
        assert_eq!(m.exports.get("."), Some(&"src/lib.ax".to_string()));
        assert_eq!(m.exports.get("./client"), Some(&"src/client.ax".to_string()));
    }

    #[test]
    fn legacy_exports_roundtrip_omits_empty() {
        let m = legacy(r#"{"name":"a","version":"1.0.0","exports":{".":"./lib.ax"}}"#);
        let text = serde_json::to_string_pretty(&m).unwrap();
        assert!(text.contains("exports"));
        assert!(!text.contains("types"));
        let m2 = legacy(&text);
        assert_eq!(m2.exports.get("."), Some(&"./lib.ax".to_string()));
    }

    #[test]
    fn legacy_deny_unknown_fields_still_holds() {
        let res: Result<Manifest, _> =
            serde_json::from_str(r#"{"name":"a","version":"1.0.0","bogus":true}"#);
        assert!(res.is_err(), "deny_unknown_fields must reject unknown keys");
    }

    #[test]
    fn parses_dev_dependencies_and_overrides() {
        let m = legacy(
            r#"{
                "name": "app",
                "version": "0.1.0",
                "dependencies": { "http": "^1" },
                "devDependencies": { "harness": { "path": "../harness" } },
                "overrides": { "zlib": "1.4.2" }
            }"#,
        );
        assert_eq!(m.dependencies.len(), 1);
        assert_eq!(m.dev_dependencies.len(), 1);
        assert!(matches!(
            m.dev_dependencies.get("harness"),
            Some(DependencySpec::Path { .. })
        ));
        assert_eq!(m.overrides.get("zlib").map(String::as_str), Some("1.4.2"));
        m.validate().unwrap();
    }

    #[test]
    fn legacy_dev_dependencies_accept_the_snake_case_spelling() {
        let m = legacy(r#"{"name":"a","version":"1.0.0","dev_dependencies":{"x":"^1"}}"#);
        assert!(m.dev_dependencies.get("x").is_some());
    }

    #[test]
    fn legacy_overrides_roundtrip_omit_when_empty() {
        let m = legacy(r#"{"name":"a","version":"1.0.0","dependencies":{"b":"^1"}}"#);
        let text = serde_json::to_string_pretty(&m).unwrap();
        assert!(!text.contains("devDependencies"));
        assert!(!text.contains("overrides"), "empty tables must not be written");
        let with_dev = Manifest {
            dev_dependencies: BTreeMap::from([(
                "x".to_string(),
                DependencySpec::Registry { req: "^1".into(), registry: None },
            )]),
            ..Manifest::synthetic()
        };
        let text = serde_json::to_string_pretty(&with_dev).unwrap();
        assert!(text.contains("devDependencies"), "{text}");
        assert!(!text.contains("dev_dependencies"), "{text}");
    }

    #[test]
    fn legacy_invalid_override_is_rejected_at_load() {
        let m = legacy(r#"{"name":"a","version":"1.0.0","overrides":{"z":"~~1"}}"#);
        let err = m.validate().unwrap_err();
        assert!(format!("{err}").contains("override"), "{err}");
    }

    #[test]
    fn legacy_dev_dependency_requirements_are_validated_too() {
        let m = legacy(r#"{"name":"a","version":"1.0.0","devDependencies":{"x":"not-a-range"}}"#);
        let err = m.validate().unwrap_err();
        assert!(format!("{err}").contains("invalid requirement"), "{err}");
    }

    /// A dependency KEY becomes `aox_modules/<key>`, and that directory is
    /// removed RECURSIVELY before extraction and before any integrity check —
    /// so the key `"../victim"` deleted `app/victim` outright (reproduced).
    #[test]
    fn legacy_dependency_keys_are_validated_as_names() {
        for bad in ["../victim", "..", "a/b", "a\\b", "C:name", "./x", "..\\..\\y"] {
            let json_bad = bad.replace(0x5Cu8 as char, "\\\\");
            let m = legacy(&format!(
                r#"{{"name":"app","version":"1.0.0","dependencies":{{"{json_bad}":"^1.0.0"}}}}"#
            ));
            let err = m.validate().unwrap_err();
            assert!(
                format!("{err}").contains("invalid package name"),
                "dependency key `{bad}` must be refused as a name: {err}"
            );
        }
        let d = legacy(
            r#"{"name":"app","version":"1.0.0","devDependencies":{"../victim":"^1.0.0"}}"#,
        );
        assert!(format!("{:?}", d.validate().unwrap_err()).contains("invalid package name"));
        let ok =
            legacy(r#"{"name":"app","version":"1.0.0","dependencies":{"my-lib_2":"^1.0.0"}}"#);
        ok.validate().unwrap();
    }

    // ---- TOML (v0.55.0 snapshot 6, manifest v2) ----

    #[test]
    fn toml_parses_project_dependencies_extras_and_tool_tables() {
        let text = r#"
[project]
name = "my-app"
version = "0.1.0"
description = "demo"
main = "src/main.ax"
types = "src/types.ax"
dependencies = [
  "http>=1.2,<2",
  "json ^1",
  "my-utils @ path:../my-utils",
]

[project.exports]
"." = "src/lib.ax"
"./client" = "src/client.ax"

[project.optional-dependencies]
dev = ["test-harness>=2", "linter"]

[tool.aoxn.dependencies]
json = { registry = "mirror" }

[tool.aoxn.overrides]
http = "=1.4.2"

[tool.aoxn.registries.default]
url = "https://example.invalid/registry"

[tool.aoxn.workspace]
members = ["packages/*"]
"#;
        let tmp = std::env::temp_dir().join(format!("aoxn-toml-parse-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join(FILE_NAME);
        std::fs::write(&path, text).unwrap();
        let m = Manifest::load(&path).unwrap();
        assert_eq!(m.name, "my-app");
        assert_eq!(m.main.as_deref(), Some("src/main.ax"));
        assert_eq!(
            m.dependencies.get("http"),
            Some(&DependencySpec::Registry { req: ">=1.2,<2".into(), registry: None })
        );
        assert_eq!(
            m.dependencies.get("json"),
            Some(&DependencySpec::Registry { req: "^1".into(), registry: Some("mirror".into()) })
        );
        assert_eq!(
            m.dependencies.get("my-utils"),
            Some(&DependencySpec::Path { path: PathBuf::from("../my-utils") })
        );
        assert_eq!(m.dev_dependencies.len(), 2);
        assert_eq!(m.overrides.get("http").map(String::as_str), Some("=1.4.2"));
        assert_eq!(m.workspace.as_ref().unwrap().members, vec!["packages/*"]);
        assert!(m.registries.contains_key("default"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn toml_roundtrip_preserves_every_field() {
        let m = Manifest {
            name: "app".into(),
            version: "1.2.3".into(),
            main: Some("main.ax".into()),
            types: None,
            exports: BTreeMap::from([(".".into(), "lib.ax".into())]),
            description: Some("x".into()),
            dependencies: BTreeMap::from([
                ("http".into(), DependencySpec::Registry { req: "^1.2".into(), registry: None }),
                (
                    "json".into(),
                    DependencySpec::Registry { req: "*".into(), registry: Some("mirror".into()) },
                ),
                (
                    "local".into(),
                    DependencySpec::Path { path: PathBuf::from("../local") },
                ),
            ]),
            dev_dependencies: BTreeMap::from([(
                "harness".into(),
                DependencySpec::Registry { req: "^2".into(), registry: None },
            )]),
            overrides: BTreeMap::from([("http".into(), "=1.4.2".into())]),
            workspace: Some(WorkspaceDecl { members: vec!["packages/*".into()] }),
            registries: BTreeMap::from([(
                "default".into(),
                RegistryDecl { url: "https://example.invalid/r".into(), kind: None },
            )]),
        };
        let text = m.to_toml_string().unwrap();
        assert!(text.starts_with("[project]"), "{text}");
        assert!(text.contains("\"http ^1.2\""), "{text}");
        assert!(text.contains("\"json\""), "{text}");
        assert!(!text.contains("my-utils"), "{text}");
        assert!(text.contains("\"local @ path:../local\""), "{text}");
        let tmp = std::env::temp_dir().join(format!("aoxn-toml-round-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join(FILE_NAME);
        std::fs::write(&path, &text).unwrap();
        let m2 = Manifest::load(&path).unwrap();
        assert_eq!(m, m2);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn legacy_json_is_still_read_and_saved_by_extension() {
        let tmp = std::env::temp_dir().join(format!("aoxn-legacy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let json_path = tmp.join(LEGACY_FILE_NAME);
        std::fs::write(&json_path, r#"{"name":"old","version":"0.1.0"}"#).unwrap();
        assert_eq!(Manifest::path_for(&tmp), json_path);
        let mut m = Manifest::load(&json_path).unwrap();
        m.dependencies.insert(
            "x".into(),
            DependencySpec::Registry { req: "^1".into(), registry: None },
        );
        m.save(&json_path).unwrap();
        let text = std::fs::read_to_string(&json_path).unwrap();
        assert!(text.contains("\"x\""), "legacy projects must stay JSON: {text}");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn toml_is_preferred_when_both_exist() {
        let tmp = std::env::temp_dir().join(format!("aoxn-both-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join(FILE_NAME), "[project]\nname=\"new\"\nversion=\"1.0.0\"\n")
            .unwrap();
        std::fs::write(tmp.join(LEGACY_FILE_NAME), r#"{"name":"old","version":"1.0.0"}"#)
            .unwrap();
        assert_eq!(Manifest::path_for(&tmp), tmp.join(FILE_NAME));
        assert_eq!(Manifest::load_dir(&tmp).unwrap().name, "new");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn dependency_string_forms() {
        assert_eq!(
            parse_dependency("http").unwrap(),
            ("http".into(), DependencySpec::Registry { req: "*".into(), registry: None })
        );
        assert_eq!(
            parse_dependency("http>=1.2,<2").unwrap(),
            (
                "http".into(),
                DependencySpec::Registry { req: ">=1.2,<2".into(), registry: None }
            )
        );
        assert_eq!(
            parse_dependency("http == 1.2.3").unwrap(),
            (
                "http".into(),
                DependencySpec::Registry { req: "=1.2.3".into(), registry: None }
            )
        );
        assert_eq!(
            parse_dependency("http~=1.2").unwrap(),
            (
                "http".into(),
                DependencySpec::Registry { req: "~1.2".into(), registry: None }
            )
        );
        assert_eq!(
            parse_dependency("http@^1.2").unwrap(),
            (
                "http".into(),
                DependencySpec::Registry { req: "^1.2".into(), registry: None }
            )
        );
        assert_eq!(
            parse_dependency("local @ path:../local").unwrap(),
            (
                "local".into(),
                DependencySpec::Path { path: PathBuf::from("../local") }
            )
        );
        assert_eq!(
            parse_dependency("json @ registry:mirror").unwrap(),
            (
                "json".into(),
                DependencySpec::Registry { req: "*".into(), registry: Some("mirror".into()) }
            )
        );
    }

    #[test]
    fn dependency_string_rejects_unsupported_pep508_surface() {
        assert!(parse_dependency("http[security]>=1").is_err());
        assert!(parse_dependency("http>=1; python_version<'3'").is_err());
        assert!(parse_dependency("http @ https://example.invalid/x.tar.gz").is_err());
        assert!(parse_dependency("HTTP>=1").is_err());
        assert!(parse_dependency("http @ path:").is_err());
    }

    #[test]
    fn only_the_dev_optional_group_is_supported() {
        let text = r#"
[project]
name = "app"
version = "0.1.0"
[project.optional-dependencies]
test = ["harness"]
"#;
        let tmp = std::env::temp_dir().join(format!("aoxn-extra-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join(FILE_NAME);
        std::fs::write(&path, text).unwrap();
        let err = Manifest::load(&path).unwrap_err();
        assert!(format!("{err}").contains("only `dev`"), "{err}");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn dependency_groups_dev_is_an_alias() {
        let text = r#"
[project]
name = "app"
version = "0.1.0"
[dependency-groups]
dev = ["harness>=1"]
"#;
        let tmp = std::env::temp_dir().join(format!("aoxn-groups-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join(FILE_NAME);
        std::fs::write(&path, text).unwrap();
        let m = Manifest::load(&path).unwrap();
        assert!(m.dev_dependencies.contains_key("harness"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn toml_bom_is_tolerated() {
        let tmp = std::env::temp_dir().join(format!("aoxn-toml-bom-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join(FILE_NAME);
        std::fs::write(&path, "\u{FEFF}[project]\nname=\"p\"\nversion=\"1.0.0\"\n").unwrap();
        assert_eq!(Manifest::load(&path).unwrap().name, "p");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
