//! stdlib/sys.ax: argv wrapper (count, order, empty/spaced args), platform
//! fold, executable path, maxsize.

use std::{fs, path::PathBuf, process::Command};

const EXE: &str = if cfg!(windows) { ".exe" } else { "" };

const SRC: &str = r#"
import * from "stdlib/sys"

def main() -> int:
    print("platform", sys_platform())
    print("argc", sys_argc())
    v = sys_argv()
    i = 0
    while i < vec_len(v):
        print("arg", vec_get_str(v, i))
        i = i + 1
    print("exe", len(sys_executable()) > 0)
    print("maxsize", sys_maxsize())
    print("DONE")
    return 0
"#;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn sys_argv_platform_and_executable() {
    let dir = root().join("target").join(format!("sys-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("sys_driver.ax");
    fs::write(&src, SRC).unwrap();
    let exe = dir.join(format!("sys_driver{EXE}"));
    aoxn::build_paths_opts(&[src.display().to_string()], &exe, true, &[], &[])
        .unwrap_or_else(|d| panic!("sys driver failed: {d:?}"));

    let out = Command::new(&exe)
        .args(["alpha", "beta two", ""])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let platform = if cfg!(windows) { "windows" } else { "linux" };
    let expected = format!(
        "platform {platform}\nargc 3\narg alpha\narg beta two\narg \nexe true\nmaxsize 9223372036854775807\nDONE\n"
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout), expected);

    let bare = Command::new(&exe).output().unwrap();
    assert!(bare.status.success());
    let text = String::from_utf8_lossy(&bare.stdout);
    assert!(text.contains("argc 0\n"), "{text}");
    assert!(!text.contains("arg "), "{text}");
    fs::remove_dir_all(&dir).unwrap();
}