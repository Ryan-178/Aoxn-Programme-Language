//! `aoxn test <file.ax>` — discovery, the synthesized driver, and the exit
//! code.
//!
//! These drive the real binary (`CARGO_BIN_EXE_aoxn`): the contract is the
//! command line, what the suite prints, and whether a failure reaches the
//! shell, not the internal `testcmd` helpers (those have unit tests).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

const AOXN: &str = env!("CARGO_BIN_EXE_aoxn");
static COUNTER: AtomicUsize = AtomicUsize::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Scratch {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("aoxn-testcmd-it-{}-{tag}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        Scratch(dir)
    }

    fn write(&self, name: &str, text: &str) -> PathBuf {
        let p = self.0.join(name);
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).expect("create scratch subdir");
        }
        std::fs::write(&p, text).expect("write scratch file");
        p
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `AOXN_TEST_REQUIRE_CLANG` turns a missing toolchain into a failure, the
/// same contract the other clang-driven suites follow.
fn need_clang() -> bool {
    if aoxn::find_clang().is_some() {
        return true;
    }
    assert!(
        std::env::var_os("AOXN_TEST_REQUIRE_CLANG").is_none(),
        "clang is required (AOXN_TEST_REQUIRE_CLANG) but was not found"
    );
    eprintln!("skipping: clang unavailable");
    false
}

/// Run `aoxn test <entry>` from the scratch dir, returning (code, stdout, stderr).
fn run_test(scratch: &Scratch, entry: &Path, extra: &[&str]) -> (i32, String, String) {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new(AOXN)
        .current_dir(&scratch.0)
        .env("AOXN_STDLIB", manifest.join("stdlib"))
        .env("AOXN_CACHE_DIR", scratch.0.join("cache"))
        .arg("test")
        .arg(entry)
        .args(extra)
        .output()
        .expect("run aoxn test");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn a_passing_suite_runs_every_test_and_exits_zero() {
    if !need_clang() {
        return;
    }
    let s = Scratch::new("pass");
    let f = s.write(
        "suite.ax",
        r#"import * from "stdlib/unittest"

def test_math():
    ut_assert_eq_i(6 * 7, 42, "mul")

def helper() -> int:
    return 1

def test_strings():
    ut_assert_eq_s("a" + "b", "ab", "cat")
"#,
    );
    let (code, out, err) = run_test(&s, &f, &[]);
    assert_eq!(code, 0, "stdout: {out}\nstderr: {err}");
    assert_eq!(out, "PASS test_math\nPASS test_strings\nTOTAL 2 PASSED 2 FAILED 0\n");
}

#[test]
fn a_failing_assertion_reaches_the_exit_code() {
    if !need_clang() {
        return;
    }
    let s = Scratch::new("fail");
    let f = s.write(
        "suite.ax",
        r#"import * from "stdlib/unittest"

def test_bad():
    ut_assert_eq_i(1, 2, "one is not two")
"#,
    );
    let (code, out, _err) = run_test(&s, &f, &[]);
    assert_eq!(code, 1, "stdout: {out}");
    assert!(out.contains("FAIL test_bad: one is not two: expected 2, got 1"), "{out}");
    assert!(out.contains("TOTAL 1 PASSED 0 FAILED 1"), "{out}");
}

/// The entry keeps its name when the file also declares `main`: the test
/// driver's `main` must win, and the file's own `main` must not run.
#[test]
fn a_test_file_that_declares_main_still_runs_its_tests() {
    if !need_clang() {
        return;
    }
    let s = Scratch::new("with-main");
    let f = s.write(
        "suite.ax",
        r#"import * from "stdlib/unittest"

def test_ok():
    ut_assert_true(1 < 2, "lt")

def main() -> int:
    print("user main")
    return 3
"#,
    );
    let (code, out, err) = run_test(&s, &f, &[]);
    assert_eq!(code, 0, "stdout: {out}\nstderr: {err}");
    assert!(out.contains("PASS test_ok"), "{out}");
    assert!(!out.contains("user main"), "the file's own main must not run: {out}");
}

#[test]
fn tests_in_imported_modules_are_not_collected() {
    if !need_clang() {
        return;
    }
    let s = Scratch::new("imported");
    s.write(
        "helper.ax",
        "def test_in_helper():\n    raise \"must not run\"\n",
    );
    let f = s.write(
        "suite.ax",
        r#"import * from "./helper.ax"

def test_entry_only():
    pass
"#,
    );
    let (code, out, err) = run_test(&s, &f, &[]);
    assert_eq!(code, 0, "stdout: {out}\nstderr: {err}");
    assert!(out.contains("TOTAL 1 PASSED 1 FAILED 0"), "{out}");
    assert!(!out.contains("test_in_helper"), "{out}");
}

#[test]
fn a_file_without_tests_is_an_error() {
    let s = Scratch::new("none");
    let f = s.write("plain.ax", "def helper() -> int:\n    return 1\n");
    let (code, _out, err) = run_test(&s, &f, &[]);
    assert_eq!(code, 1);
    assert!(err.contains("no top-level `def test_*(...)` functions"), "{err}");
}

#[test]
fn a_test_that_cannot_be_called_is_named() {
    let s = Scratch::new("signature");
    let f = s.write("bad.ax", "def test_takes_arg(n: int):\n    print(n)\n");
    let (code, _out, err) = run_test(&s, &f, &[]);
    assert_eq!(code, 1);
    assert!(err.contains("test function 'test_takes_arg' must take no parameters"), "{err}");
    assert!(err.contains("bad.ax:1:"), "{err}");
}

/// A directory runs each `test_*.ax` directly inside it, sorted, and ignores
/// everything else: a non-matching file, a helper without tests, a nested
/// directory.
#[test]
fn a_directory_runs_each_test_file_in_sorted_order() {
    if !need_clang() {
        return;
    }
    let s = Scratch::new("dir-pass");
    s.write(
        "test_a.ax",
        "import * from \"stdlib/unittest\"

def test_from_a():
    ut_assert_true(1 < 2, \"a\")
",
    );
    s.write(
        "test_b.ax",
        "import * from \"stdlib/unittest\"

def test_from_b():
    ut_assert_eq_s(\"x\" + \"y\", \"xy\", \"b\")
",
    );
    s.write("notes.ax", "def helper() -> int:
    return 1
");
    s.write(
        "nested/test_c.ax",
        "def test_nested():
    raise \"must not run\"
",
    );
    let (code, out, err) = run_test(&s, &s.0.clone(), &[]);
    assert_eq!(code, 0, "stdout: {out}
stderr: {err}");
    assert!(out.contains("PASS test_from_a"), "{out}");
    assert!(out.contains("PASS test_from_b"), "{out}");
    assert!(!out.contains("test_nested"), "nested directories are not walked: {out}");
    assert!(
        out.find("PASS test_from_a") < out.find("PASS test_from_b"),
        "suites run in sorted order: {out}"
    );
    assert!(out.contains("SUMMARY files=2 failed_suites=0"), "{out}");
}

/// A failing suite does not stop the directory run; the summary counts it
/// and the command exits 1.
#[test]
fn a_directory_reports_a_failing_suite_and_exits_one() {
    if !need_clang() {
        return;
    }
    let s = Scratch::new("dir-fail");
    s.write(
        "test_a.ax",
        "import * from \"stdlib/unittest\"

def test_ok():
    ut_assert_true(1 < 2, \"ok\")
",
    );
    s.write(
        "test_b.ax",
        "import * from \"stdlib/unittest\"

def test_bad():
    ut_assert_eq_i(1, 2, \"one is not two\")
",
    );
    let (code, out, err) = run_test(&s, &s.0.clone(), &[]);
    assert_eq!(code, 1, "stdout: {out}
stderr: {err}");
    assert!(out.contains("PASS test_ok"), "{out}");
    assert!(out.contains("FAIL test_bad"), "{out}");
    assert!(out.contains("SUMMARY files=2 failed_suites=1"), "{out}");
}

#[test]
fn a_directory_without_test_files_is_an_error() {
    let s = Scratch::new("dir-none");
    s.write("suite.ax", "def helper() -> int:
    return 1
");
    let (code, _out, err) = run_test(&s, &s.0.clone(), &[]);
    assert_eq!(code, 1);
    assert!(err.contains("no `test_*.ax` files"), "{err}");
}

/// A file the directory rule picked up but that declares no test is named,
/// not silently counted as a passing suite.
#[test]
fn a_test_file_without_tests_in_a_directory_is_named() {
    let s = Scratch::new("dir-empty");
    s.write("test_empty.ax", "def helper() -> int:
    return 1
");
    let (code, _out, err) = run_test(&s, &s.0.clone(), &[]);
    assert_eq!(code, 1);
    assert!(err.contains("no top-level `def test_*(...)` functions"), "{err}");
    assert!(err.contains("test_empty.ax"), "{err}");
}

#[test]
fn json_diagnostics_come_from_the_real_build() {
    if !need_clang() {
        return;
    }
    let s = Scratch::new("json");
    let f = s.write(
        "suite.ax",
        "def test_bad():\n    y: int = \"not an int\"\n",
    );
    let (code, _out, err) = run_test(&s, &f, &["--json"]);
    assert_eq!(code, 1);
    assert!(err.contains("\"errors\""), "{err}");
}
