//! Rust bridge (v0.55.0-snapshot-5): `import "./x.rs"` binds `#[no_mangle] extern "C"`
//! functions as ordinary Aoxn externs, compiled by rustc and linked in.
//!
//! Two toolchains gate the end-to-end tests, independently: clang (the C
//! backend) and rustc (the bridge). A missing one SKIPs unless the matching
//! AOXN_TEST_REQUIRE_* env turns it into a failure (CI sets both). The
//! scanner and the check/c/symbols paths need NEITHER and always run.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "aoxn-rust-interop-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// (clang, rustc) when both are available; otherwise skip (or fail under the
/// CI require-envs). The two requirements are separate so a machine with
/// only one toolchain still runs the other's tests.
fn toolchains() -> Option<(PathBuf, PathBuf)> {
    let clang = aoxn::find_clang();
    let rustc = aoxn::rust::find_rustc();
    if clang.is_none() {
        assert!(
            std::env::var_os("AOXN_TEST_REQUIRE_CLANG").is_none(),
            "clang is required for this test run; set AOXN_CLANG"
        );
        eprintln!("SKIP: clang not found; set AOXN_CLANG");
    }
    if rustc.is_none() {
        assert!(
            std::env::var_os("AOXN_TEST_REQUIRE_RUSTC").is_none(),
            "rustc is required for this test run; set AOXN_RUSTC"
        );
        eprintln!("SKIP: rustc not found; set AOXN_RUSTC");
    }
    match (clang, rustc) {
        (Some(c), Some(r)) => Some((c, r)),
        _ => None,
    }
}

fn checked(cmd: &mut Command, stage: &str) -> std::process::Output {
    let out = cmd.output().unwrap_or_else(|e| panic!("{stage}: {e}"));
    assert!(
        out.status.success(),
        "{stage}: {:?}\nstdout: {}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

const NATIVE_RS: &str = r#"
// A self-contained Rust library for the bridge tests.
use std::os::raw::c_char;

#[no_mangle]
pub extern "C" fn rs_add(a: i64, b: i64) -> i64 {
    a + b
}

#[no_mangle]
pub extern "C" fn rs_scale(x: f64, k: f64) -> f64 {
    x * k + 0.5
}

#[no_mangle]
pub extern "C" fn rs_len(s: *const c_char) -> i64 {
    let mut n = 0i64;
    unsafe {
        while *s.offset(n as isize) != 0 {
            n += 1;
        }
    }
    n
}

#[no_mangle]
pub extern "C" fn rs_greet() -> *const c_char {
    b"hello from rust\0".as_ptr() as *const c_char
}

static mut CALLS: i64 = 0;

#[no_mangle]
pub extern "C" fn rs_bump() {
    unsafe {
        CALLS += 1;
    }
}

#[no_mangle]
pub extern "C" fn rs_calls() -> i64 {
    unsafe { CALLS }
}

// A private Rust fn binds nothing; the scanner must ignore it.
fn helper(x: i64) -> i64 {
    x * 2
}

#[no_mangle]
pub extern "C" fn rs_twice(x: i64) -> i64 {
    helper(x)
}
"#;

const MAIN_AX: &str = r#"
import * from "./native.rs"

def main() -> int:
    assert(rs_add(2, 40) == 42)
    assert(rs_add(-8589934593, 8589934593) == 0)
    assert(rs_scale(2.0, 3.0) == 6.5)
    assert(rs_len("aoxn") == 4)
    assert(rs_len("") == 0)
    assert(rs_greet() == "hello from rust")
    assert(rs_twice(21) == 42)
    rs_bump()
    rs_bump()
    assert(rs_calls() == 2)
    print("PASS rust-bridge")
    return 0
"#;

#[test]
fn extern_signatures_build_link_and_run() {
    let Some(_) = toolchains() else { return };
    // Exercise both unoptimized and optimized Rust (the staticlib is
    // compiled at the program's -O level, like the generated C).
    for level in [0u8, 3u8] {
        let scratch = Scratch::new();
        std::fs::write(scratch.0.join("native.rs"), NATIVE_RS).unwrap();
        let main = scratch.0.join("main.ax");
        std::fs::write(&main, MAIN_AX).unwrap();
        let exe = scratch.0.join(format!("program{}", aoxn::platform::exe_ext()));
        aoxn::build_paths_opts_lvl(&[main.display().to_string()], &exe, level, &[], &[])
            .unwrap_or_else(|d| panic!("O{level}: {d:?}"));
        let out = checked(&mut Command::new(&exe), "run mixed program");
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"),
            "PASS rust-bridge\n"
        );
        eprintln!("EXECUTED rust-bridge O{level}");
    }
}

#[test]
fn namespace_and_selective_imports() {
    let Some(_) = toolchains() else { return };
    let scratch = Scratch::new();
    std::fs::write(scratch.0.join("native.rs"), NATIVE_RS).unwrap();
    let main = scratch.0.join("main.ax");
    std::fs::write(
        &main,
        "import native\nfrom native import rs_add\n\ndef main() -> int:\n    \
         assert(native.rs_add(1, 2) == 3)\n    assert(rs_add(40, 2) == 42)\n    \
         assert(native.rs_len(\"rust\") == 4)\n    print(\"PASS ns\")\n    return 0\n",
    )
    .unwrap();
    let exe = scratch.0.join(format!("program{}", aoxn::platform::exe_ext()));
    aoxn::build_paths_opts_lvl(&[main.display().to_string()], &exe, 0, &[], &[])
        .unwrap_or_else(|d| panic!("{d:?}"));
    let out = checked(&mut Command::new(&exe), "run namespace program");
    assert!(String::from_utf8_lossy(&out.stdout).contains("PASS ns"));
}

#[test]
fn two_modules_exporting_one_extern_name_collide() {
    // One C symbol cannot be two functions: the bridge keeps extern names
    // unmangled (they ARE the link symbols), so a duplicate is typecheck's
    // "defined more than once", not a link-time undefined reference.
    let scratch = Scratch::new();
    std::fs::write(scratch.0.join("a.rs"), "#[no_mangle]\npub extern \"C\" fn rs_dup() -> i64 { 1 }\n").unwrap();
    std::fs::write(scratch.0.join("b.rs"), "#[no_mangle]\npub extern \"C\" fn rs_dup() -> i64 { 2 }\n").unwrap();
    let main = scratch.0.join("main.ax");
    std::fs::write(
        &main,
        "import a\nimport b\n\ndef main() -> int:\n    print(a.rs_dup() + b.rs_dup())\n    return 0\n",
    )
    .unwrap();
    let diags = aoxn::compile_paths_to_c(&[main.display().to_string()], false).expect_err("duplicate extern");
    assert!(diags.iter().all(|d| d.stage == "type"), "{diags:?}");
    assert!(
        diags.iter().any(|d| d.message.contains("'rs_dup' is defined more than once")),
        "{diags:?}"
    );
}

#[test]
fn scanner_rejects_unsupported_signatures() {
    // Pure scan: no rustc, no clang. Every case is a REAL ABI or cache
    // hazard, not a style preference.
    let cases: &[(&str, &str)] = &[
        ("#[no_mangle] pub extern \"C\" fn f(x: bool) -> i64 { 0 }", "Rust bool is 1 byte"),
        ("#[no_mangle] pub extern \"C\" fn f() -> bool { true }", "Rust bool is 1 byte"),
        ("#[no_mangle] pub extern \"C\" fn f(x: i32) -> i64 { 0 }", "unsupported type 'i32'"),
        ("#[no_mangle] pub extern \"C\" fn f(x: u64) -> i64 { 0 }", "unsupported type 'u64'"),
        ("#[no_mangle] pub extern \"C\" fn f() -> String { String::new() }", "unsupported type 'String'"),
        ("#[no_mangle] pub extern \"C\" fn f(s: &str) -> i64 { 0 }", "unsupported type"),
        ("#[no_mangle] pub extern \"C\" fn f(x: *const i64) -> i64 { 0 }", "unsupported type"),
        ("pub extern \"C\" fn f() -> i64 { 0 }", "without #[no_mangle]"),
        ("#[export_name = \"other\"] pub extern \"C\" fn f() -> i64 { 0 }", "#[export_name] is not supported"),
        ("#[no_mangle] pub extern \"C\" fn f<T>(x: T) -> i64 { 0 }", "generic"),
        ("#[no_mangle] pub extern \"C\" fn f(x: i64) -> i64 where i64: Copy { x }", "where clause"),
        ("#[no_mangle] pub async extern \"C\" fn f() -> i64 { 0 }", "async fn returns a Future"),
        ("#[no_mangle] pub extern \"C-unwind\" fn f() -> i64 { 0 }", "unsupported ABI"),
        ("#[no_mangle] pub extern \"C\" fn r#type() -> i64 { 0 }", "raw identifiers"),
        ("#[no_mangle] pub extern \"C\" fn int() -> i64 { 0 }", "cannot be a C symbol"),
        ("#[no_mangle] pub extern \"C\" fn main() -> i64 { 0 }", "cannot be a C symbol"),
        ("mod other;", "multi-file Rust is not supported"),
        ("include!(\"other.rs\");", "pulls another file into the crate"),
        ("#[no_mangle] pub extern \"C\" fn f(x: ...) -> i64 { 0 }", "unsupported parameter"),
        ("#[no_mangle] pub extern \"C\" fn f((a, b): (i64, i64)) -> i64 { 0 }", "unsupported parameter"),
    ];
    for (src, expected) in cases {
        let errs = aoxn::rust::scan(src, 0).expect_err(src);
        assert!(errs.iter().all(|d| d.stage == "parse"), "{src}: {errs:?}");
        assert!(
            errs.iter().any(|d| d.message.contains(expected)),
            "{src}: expected {expected:?}, got {errs:?}"
        );
    }
}

#[test]
fn scanner_ignores_comments_strings_and_private_items() {
    // `fn` text inside comments, strings and macro bodies is not a
    // declaration; private Rust fns bind nothing. Only TOP-LEVEL exports
    // bind: an inline `mod`'s contents are skipped (documented boundary —
    // rustc still compiles them, but the bridge does not descend into item
    // bodies, and a fn body can contain nested items too).
    let src = r###"
// #[no_mangle] pub extern "C" fn fake_comment() -> i64 { 0 }
/* #[no_mangle] pub extern "C" fn fake_block() -> i64 { 0 } */
/* outer /* nested */ still comment: fn fake_nested() */
const S: &str = "#[no_mangle] pub extern \"C\" fn fake_string() -> i64 { 0 }";
const R: &str = r##"fn fake_raw() extern "C" #[no_mangle]"##;
macro_rules! m {
    () => { #[no_mangle] pub extern "C" fn fake_macro() -> i64 { 0 } };
}
fn private() -> i64 { 1 }
pub fn public_rust() -> i64 { 2 }
mod inline {
    #[no_mangle]
    pub extern "C" fn inside_inline() -> i64 { 3 }
}
#[no_mangle]
pub extern "C" fn real_export() -> i64 { 4 }
"###;
    let funcs = aoxn::rust::scan(src, 0).unwrap_or_else(|d| panic!("{d:?}"));
    let names: Vec<&str> = funcs.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, vec!["real_export"], "{names:?}");
    assert!(funcs.iter().all(|f| f.is_extern && f.body.stmts.is_empty()));
}

#[test]
fn scanner_maps_the_abi_surface() {
    let src = r#"
use std::ffi::c_char;
#[no_mangle] pub extern "C" fn a(x: i64) -> i64 { x }
#[no_mangle] pub extern "C" fn b(x: f64) -> f64 { x }
#[no_mangle] pub extern "C" fn c(s: *const c_char) -> *mut c_char { 0 as *mut _ }
#[no_mangle] pub extern "C" fn d(s: *mut std::os::raw::c_char) {}
#[no_mangle] pub extern "C" fn e() -> () {}
#[no_mangle] pub extern "C" fn f(mut x: i64) -> i64 { x = 1; x }
"#;
    let funcs = aoxn::rust::scan(src, 0).unwrap_or_else(|d| panic!("{d:?}"));
    let got: Vec<(String, String, String)> = funcs
        .iter()
        .map(|f| {
            (
                f.name.clone(),
                f.params.iter().map(|p| p.ty.to_string()).collect::<Vec<_>>().join(","),
                f.ret.to_string(),
            )
        })
        .collect();
    assert_eq!(
        got,
        vec![
            ("a".into(), "int".into(), "int".into()),
            ("b".into(), "float".into(), "float".into()),
            ("c".into(), "string".into(), "string".into()),
            ("d".into(), "string".into(), "void".into()),
            ("e".into(), "".into(), "void".into()),
            ("f".into(), "int".into(), "int".into()),
        ]
    );
}

#[test]
fn check_c_symbols_work_without_rustc() {
    // The scanner is pure Rust: check/c/symbols must work on a machine with
    // NO rustc at all, while `build` fails with the bridge's own diagnostic.
    // Run the real CLI with rustc hidden (empty PATH + an empty HOME) so the
    // guarantee is pinned end to end, not just by reading the call graph.
    let clang = match aoxn::find_clang() {
        Some(c) => c,
        None => {
            assert!(
                std::env::var_os("AOXN_TEST_REQUIRE_CLANG").is_none(),
                "clang is required for this test run; set AOXN_CLANG"
            );
            eprintln!("SKIP: clang not found");
            return;
        }
    };
    let scratch = Scratch::new();
    std::fs::write(scratch.0.join("native.rs"), NATIVE_RS).unwrap();
    let main = scratch.0.join("main.ax");
    std::fs::write(&main, MAIN_AX).unwrap();
    let empty_home = scratch.0.join("home");
    std::fs::create_dir_all(&empty_home).unwrap();
    let exe = scratch.0.join(format!("out{}", aoxn::platform::exe_ext()));
    let cmd = |args: &[&str]| {
        let mut c = Command::new(env!("CARGO_BIN_EXE_aoxn"));
        c.args(args)
            .current_dir(&scratch.0)
            .env("PATH", "")
            .env("HOME", &empty_home)
            .env("USERPROFILE", &empty_home)
            .env("AOXN_CLANG", &clang)
            .env("AOXN_CACHE_DIR", scratch.0.join("cache"))
            .env_remove("AOXN_RUSTC")
            .env_remove("AOXN_NO_CACHE");
        c
    };
    // check / c / symbols: no rustc anywhere, all must succeed
    checked(&mut cmd(&["check", "main.ax"]), "check without rustc");
    let c_out = checked(&mut cmd(&["c", "main.ax"]), "c without rustc");
    let text = String::from_utf8_lossy(&c_out.stdout);
    assert!(text.contains("long long rs_add(long long a, long long b);"), "{text}");
    assert!(text.contains("char* rs_greet(void);"), "{text}");
    let sym_out = checked(&mut cmd(&["symbols", "main.ax"]), "symbols without rustc");
    let text = String::from_utf8_lossy(&sym_out.stdout);
    assert!(text.contains("extern def rs_add(a: int, b: int) -> int"), "{text}");
    assert!(text.contains("native.rs"), "{text}");
    // build: clang is present, so the failure must be the BRIDGE's missing
    // rustc diagnostic, not a link error or a panic
    let out = cmd(&["build", "main.ax", "-o", exe.to_str().unwrap()]).output().unwrap();
    assert!(!out.status.success(), "build should fail without rustc");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("cannot find rustc"), "{stderr}");
    assert!(stderr.contains("AOXN_RUSTC"), "{stderr}");
}

#[test]
fn rustc_errors_surface_as_cc_diags() {
    let Some(_) = toolchains() else { return };
    let scratch = Scratch::new();
    // The signature scans clean; the BODY does not compile. The failure must
    // arrive as a `cc` diagnostic carrying rustc's message.
    std::fs::write(
        scratch.0.join("bad.rs"),
        "#[no_mangle]\npub extern \"C\" fn bad() -> i64 {\n    let x: i64 = \"not an integer\";\n    x\n}\n",
    )
    .unwrap();
    let main = scratch.0.join("main.ax");
    std::fs::write(&main, "import * from \"./bad.rs\"\n\ndef main() -> int:\n    print(bad())\n    return 0\n").unwrap();
    let exe = scratch.0.join(format!("program{}", aoxn::platform::exe_ext()));
    let diags = aoxn::build_paths_opts_lvl(&[main.display().to_string()], &exe, 0, &[], &[])
        .expect_err("rustc must reject the body");
    assert!(diags.iter().any(|d| d.stage == "cc" && d.message.contains("rustc rejected")), "{diags:?}");
    assert!(diags.iter().any(|d| d.message.contains("mismatched types")), "{diags:?}");
}

#[test]
fn artifact_cache_hits_and_invalidates() {
    let Some(_) = toolchains() else { return };
    if std::env::var_os("AOXN_NO_CACHE").is_some() {
        eprintln!("SKIP: artifact cache disabled by AOXN_NO_CACHE");
        return;
    }
    let scratch = Scratch::new();
    let rs = scratch.0.join("native.rs");
    std::fs::write(&rs, NATIVE_RS).unwrap();
    // The DEFAULT cache root (cwd/target/cache/rust): no env mutation, so
    // parallel tests in this file never see a cache dir appear or vanish
    // under them. The key includes the source path, so this scratch file's
    // entries cannot collide with another test's.
    let first = aoxn::rust::compile_staticlib(&rs, 0).unwrap_or_else(|d| panic!("{d:?}"));
    assert!(first.artifact.is_file());
    let md = std::fs::metadata(&first.artifact).unwrap();
    let mtime = md.modified().unwrap();
    // same content + same toolchain + same level: same path, no rebuild
    let again = aoxn::rust::compile_staticlib(&rs, 0).unwrap();
    assert_eq!(again.artifact, first.artifact);
    assert_eq!(std::fs::metadata(&again.artifact).unwrap().modified().unwrap(), mtime);
    assert!(!first.native_libs.is_empty(), "the native-static-libs note was empty");
    // a different opt level is a different artifact
    let o3 = aoxn::rust::compile_staticlib(&rs, 3).unwrap();
    assert_ne!(o3.artifact, first.artifact);
    // edited content is a different artifact
    std::fs::write(&rs, format!("{NATIVE_RS}\n// touched\n")).unwrap();
    let edited = aoxn::rust::compile_staticlib(&rs, 0).unwrap();
    assert_ne!(edited.artifact, first.artifact);
}

#[test]
fn dependency_walks_hash_rs_files_as_leaves() {
    // The exe-cache key material must include the .rs bytes (editing Rust
    // invalidates the cached exe) and must NOT scan Rust text for Aoxn
    // `import` lines (a comment shaped like one would over-include).
    let scratch = Scratch::new();
    let rs = scratch.0.join("native.rs");
    std::fs::write(&rs, "// import \"./phantom.ax\"\n#[no_mangle] pub extern \"C\" fn f() -> i64 { 1 }\n").unwrap();
    let main = scratch.0.join("main.ax");
    std::fs::write(&main, "import * from \"./native.rs\"\n\ndef main() -> int:\n    print(f())\n    return 0\n").unwrap();
    let entries = vec![main.display().to_string()];
    let before = aoxn::dependency_digests(&entries).unwrap();
    assert!(before.iter().any(|(p, _)| p.extension().map(|e| e == "rs").unwrap_or(false)));
    assert_eq!(before.len(), 2, "the phantom import must not be walked: {before:?}");
    std::fs::write(&rs, "#[no_mangle] pub extern \"C\" fn f() -> i64 { 2 }\n").unwrap();
    let after = aoxn::dependency_digests(&entries).unwrap();
    let d_before = before.iter().find(|(p, _)| p.extension().map(|e| e == "rs").unwrap_or(false)).unwrap().1;
    let d_after = after.iter().find(|(p, _)| p.extension().map(|e| e == "rs").unwrap_or(false)).unwrap().1;
    assert_ne!(d_before, d_after, "editing the .rs must change its digest");
    let files = aoxn::dependency_files(&entries).unwrap();
    assert_eq!(files.len(), 2, "{files:?}");
}

#[test]
fn rs_entry_point_is_rejected() {
    // The bridge binds IMPORTED Rust; the entry file is an Aoxn/TS program.
    // A .rs entry must fail as a parse error, not reach the Aoxn lexer's
    // "unexpected character" noise.
    let scratch = Scratch::new();
    let rs = scratch.0.join("main.rs");
    std::fs::write(&rs, NATIVE_RS).unwrap();
    let diags = aoxn::compile_paths_to_c(&[rs.display().to_string()], false).expect_err(".rs entry");
    assert!(!diags.is_empty());
}