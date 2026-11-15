//! stdlib/shutil.ax: copyfile/copytree/rmtree/move over a real temp tree,
//! overwrite semantics, missing-source refusals, value errors.

use std::{fs, path::PathBuf, process::Command};

const EXE: &str = if cfg!(windows) { ".exe" } else { "" };

const SRC: &str = r#"
import * from "stdlib"
import * from "stdlib/os"
import * from "stdlib/pathlib"
import * from "stdlib/shutil"

def main() -> int:
    root = "shutil_test_root"
    shutil_rmtree(root)
    os_mkdir(root)
    os_mkdir(path_join(root, "sub"))
    write_file(path_join(root, "a.txt"), "alpha")
    write_file(path_join(path_join(root, "sub"), "b.txt"), "beta")
    print("copytree", shutil_copytree(root, "shutil_test_copy"))
    print("copied-a", read_file(path_join("shutil_test_copy", "a.txt")))
    print("copied-b", read_file(path_join(path_join("shutil_test_copy", "sub"), "b.txt")))
    print("copyfile", shutil_copyfile(path_join(root, "a.txt"), path_join(root, "a2.txt")))
    print("a2", read_file(path_join(root, "a2.txt")))
    print("overwrite", shutil_copyfile(path_join(path_join(root, "sub"), "b.txt"), path_join(root, "a2.txt")))
    print("a2-now", read_file(path_join(root, "a2.txt")))
    print("movefile", shutil_move(path_join(root, "a2.txt"), path_join(root, "a3.txt")))
    print("a3", read_file(path_join(root, "a3.txt")))
    print("a2-gone", os_exists(path_join(root, "a2.txt")))
    print("movedir", shutil_move(path_join(root, "sub"), path_join(root, "sub2")))
    print("moved-b", read_file(path_join(path_join(root, "sub2"), "b.txt")))
    print("sub-gone", os_exists(path_join(root, "sub")))
    print("rmtree", shutil_rmtree("shutil_test_copy"))
    print("copy-gone", os_exists("shutil_test_copy"))
    print("rmtree-root", shutil_rmtree(root))
    print("root-gone", os_exists(root))
    print("missing", shutil_copyfile("no_such_file_ax", "dst_ax"))
    print("copytree-file", shutil_copytree("no_such_file_ax", "dst_dir_ax"))
    print("rmtree-missing", shutil_rmtree("no_such_dir_ax"))
    print("DONE")
    return 0
"#;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn shutil_copy_move_and_rmtree() {
    let dir = root().join("target").join(format!("shutil-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("shutil_driver.ax");
    fs::write(&src, SRC).unwrap();
    let exe = dir.join(format!("shutil_driver{EXE}"));
    aoxn::build_paths_opts(&[src.display().to_string()], &exe, true, &[], &[])
        .unwrap_or_else(|d| panic!("shutil driver failed: {d:?}"));
    let out = Command::new(&exe).current_dir(&dir).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let expected = "\
copytree true
copied-a alpha
copied-b beta
copyfile true
a2 alpha
overwrite true
a2-now beta
movefile true
a3 beta
a2-gone false
movedir true
moved-b beta
sub-gone false
rmtree true
copy-gone false
rmtree-root true
root-gone false
missing false
copytree-file false
rmtree-missing false
DONE
";
    assert_eq!(String::from_utf8_lossy(&out.stdout), expected);
    fs::remove_dir_all(&dir).unwrap();
}