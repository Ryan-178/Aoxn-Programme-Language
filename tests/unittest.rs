//! stdlib/unittest.ax: suite registry, raise-based assertions, per-test
//! isolation, report counts — plus a combined selfhost-CLI pin covering
//! sys + argparse + logging + unittest (dict, fn pointers, raise/try,
//! datetime and file I/O through stage 2).

use std::{fs, path::PathBuf, process::Command};

const EXE: &str = if cfg!(windows) { ".exe" } else { "" };

const SRC: &str = r#"
import * from "stdlib/unittest"

def good_i():
    ut_assert_eq_i(6 * 7, 42, "mul")

def good_s():
    ut_assert_eq_s("x" + "y", "xy", "cat")

def good_true():
    ut_assert_true(1 < 2, "lt")

def boom():
    raise "bang"

def good_raises():
    ut_assert_raises(boom, "must raise")

def bad_i():
    ut_assert_eq_i(2, 3, "numbers")

def bad_raises():
    ut_assert_raises(good_i, "should have raised")

def main() -> int:
    s = ut_suite()
    s = ut_add(s, "i", good_i)
    s = ut_add(s, "s", good_s)
    s = ut_add(s, "true", good_true)
    s = ut_add(s, "raises", good_raises)
    s = ut_add(s, "bad_i", bad_i)
    s = ut_add(s, "bad_raises", bad_raises)
    r = ut_run(s)
    print("report", r.total, r.passed, r.failed)
    print("DONE")
    return 0
"#;

const COMBINED: &str = r#"
import * from "stdlib"
import * from "stdlib/sys"
import * from "stdlib/argparse"
import * from "stdlib/logging"
import * from "stdlib/unittest"

def check_args():
    ap = ap_new("p", "d")
    ap = ap_opt(ap, "name", "n", "world", "name")
    a = vec_new()
    a = vec_push_str(a, "-n")
    a = vec_push_str(a, "aoxn")
    res = ap_parse(ap, a)
    ut_assert_eq_s(res.err, "", "parse err")
    ut_assert_eq_s(ap_get(res, "name", ""), "aoxn", "name")

def check_platform():
    p = sys_platform()
    ut_assert_true(p == "windows" or p == "linux" or p == "other", "platform")

def check_log():
    lg = log_new("sh")
    lg = log_set_stdout(lg, 0)
    lg = log_add_file(lg, "selfhost_log.txt")
    log_info(lg, "via selfhost")
    t = read_file("selfhost_log.txt")
    ut_assert_true(len(t) > 0, "log written")

def main() -> int:
    s = ut_suite()
    s = ut_add(s, "args", check_args)
    s = ut_add(s, "platform", check_platform)
    s = ut_add(s, "log", check_log)
    r = ut_run(s)
    print("DONE")
    return r.failed
"#;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn unittest_suite_assertions_and_report() {
    let dir = root().join("target").join(format!("unittest-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("unittest_driver.ax");
    fs::write(&src, SRC).unwrap();
    let exe = dir.join(format!("unittest_driver{EXE}"));
    aoxn::build_paths_opts(&[src.display().to_string()], &exe, true, &[], &[])
        .unwrap_or_else(|d| panic!("unittest driver failed: {d:?}"));
    let out = Command::new(&exe).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let expected = "\
PASS i
PASS s
PASS true
PASS raises
FAIL bad_i: numbers: expected 3, got 2
FAIL bad_raises: should have raised
TOTAL 6 PASSED 4 FAILED 2
report 6 4 2
DONE
";
    assert_eq!(String::from_utf8_lossy(&out.stdout), expected);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn batch1_modules_run_through_the_selfhost_cli() {
    let Some(clang) = aoxn::find_clang() else {
        assert!(
            std::env::var_os("AOXN_TEST_REQUIRE_CLANG").is_none(),
            "clang required"
        );
        eprintln!("skipping: clang unavailable");
        return;
    };
    let dir = root().join("target").join(format!("unittest-selfhost-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("combined_driver.ax");
    fs::write(&src, COMBINED).unwrap();
    let cli = dir.join(format!("selfhost-cli{EXE}"));
    aoxn::build_paths_exe(
        &[root().join("selfhost/cli.ax").display().to_string()],
        &cli,
        true,
    )
    .expect("build the selfhost CLI");
    let out = Command::new(&cli)
        .current_dir(&dir)
        .env("AOXN_STDLIB", root().join("stdlib"))
        .env("AOXN_CACHE_DIR", dir.join("cache"))
        .env("AOXN_CLANG", &clang)
        .args(["run", "combined_driver.ax"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "selfhost batch1 failed: {} / {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("TOTAL 3 PASSED 3 FAILED 0"), "{text}");
    assert!(text.ends_with("DONE\n"), "{text}");
    fs::remove_dir_all(&dir).unwrap();
}