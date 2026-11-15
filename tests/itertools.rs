//! stdlib/itertools.ax: batch-form tools, exact outputs, edge cases
//! (empty/step-0/factorial guard), and a CPython differential for
//! permutation/combination ORDER on unsorted and duplicate inputs.

use std::{fs, path::PathBuf, process::Command};

const EXE: &str = if cfg!(windows) { ".exe" } else { "" };

const SRC: &str = r#"
import * from "stdlib"
import * from "stdlib/itertools"

def show(v: Vec):
    n = vec_len(v)
    line = ""
    i = 0
    while i < n:
        if i > 0:
            line = line + " "
        line = line + str(vec_get(v, i))
        i = i + 1
    print(line)

def main() -> int:
    a = it_range(0, 3, 1)
    b = it_range(10, 12, 1)
    show(it_chain(a, b))
    show(it_repeat(7, 3))
    show(it_range(5, 0, -2))
    show(it_range(0, 3, 0))
    show(it_take(it_range(0, 10, 1), 3))
    show(it_count(2, 3, 4))
    show(it_cycle(b, 5))
    show(it_cycle(a, 0))
    show(it_accumulate(it_range(1, 5, 1)))
    z = it_zip(a, b)
    print("zip", z.n)
    show(z.left)
    show(z.right)
    pr = it_product(a, b)
    print("product", pr.n)
    i = 0
    while i < pr.n:
        print(vec_get(pr.left, i), vec_get(pr.right, i))
        i = i + 1
    pm = it_permutations(a)
    print("perms", pm.n)
    i = 0
    while i < pm.n:
        show(vecvec_get(pm.rows, i))
        i = i + 1
    cm = it_combinations(it_range(1, 5, 1), 2)
    print("combs", cm.n)
    i = 0
    while i < cm.n:
        show(vecvec_get(cm.rows, i))
        i = i + 1
    print("perm0", it_permutations(vec_new()).n)
    print("comb0", it_combinations(a, 0).n)
    print("combbad", it_combinations(a, 5).n)
    print("guard", it_permutations(it_range(0, 10, 1)).n)
    print("DONE")
    return 0
"#;

const DIFF_SRC: &str = r#"
import * from "stdlib"
import * from "stdlib/itertools"

def main() -> int:
    v = vec_new()
    v = vec_push(v, 3)
    v = vec_push(v, 1)
    v = vec_push(v, 2)
    pm = it_permutations(v)
    i = 0
    while i < pm.n:
        row = vecvec_get(pm.rows, i)
        print("P", vec_get(row, 0), vec_get(row, 1), vec_get(row, 2))
        i = i + 1
    w = vec_new()
    w = vec_push(w, 1)
    w = vec_push(w, 1)
    w = vec_push(w, 2)
    cm = it_combinations(w, 2)
    i = 0
    while i < cm.n:
        row = vecvec_get(cm.rows, i)
        print("C", vec_get(row, 0), vec_get(row, 1))
        i = i + 1
    print("DONE")
    return 0
"#;

const PY: &str = r#"
import itertools
for p in itertools.permutations([3, 1, 2]):
    print("P", *p)
for c in itertools.combinations([1, 1, 2], 2):
    print("C", *c)
print("DONE")
"#;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn build_driver(dir: &std::path::Path, name: &str, src: &str) -> PathBuf {
    let path = dir.join(format!("{name}.ax"));
    fs::write(&path, src).unwrap();
    let exe = dir.join(format!("{name}{EXE}"));
    aoxn::build_paths_opts(&[path.display().to_string()], &exe, true, &[], &[])
        .unwrap_or_else(|d| panic!("{name} driver failed: {d:?}"));
    exe
}

#[test]
fn itertools_batch_forms_and_edges() {
    let dir = root().join("target").join(format!("itertools-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let exe = build_driver(&dir, "itertools_driver", SRC);
    let out = Command::new(&exe).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let expected = "\
0 1 2 10 11
7 7 7
5 3 1

0 1 2
2 5 8 11
10 11 10 11 10

1 3 6 10
zip 2
0 1
10 11
product 6
0 10
0 11
1 10
1 11
2 10
2 11
perms 6
0 1 2
0 2 1
1 0 2
1 2 0
2 0 1
2 1 0
combs 6
1 2
1 3
1 4
2 3
2 4
3 4
perm0 1
comb0 1
combbad 0
guard 0
DONE
";
    assert_eq!(String::from_utf8_lossy(&out.stdout), expected);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn itertools_order_matches_python() {
    let py = Command::new("python").arg("--version").output();
    let Ok(py) = py else {
        eprintln!("skipping: python unavailable");
        return;
    };
    if !py.status.success() {
        eprintln!("skipping: python unavailable");
        return;
    }
    let dir = root().join("target").join(format!("itertools-diff-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let exe = build_driver(&dir, "itertools_diff", DIFF_SRC);
    let out = Command::new(&exe).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let pyfile = dir.join("diff.py");
    fs::write(&pyfile, PY).unwrap();
    let pyout = Command::new("python").arg(&pyfile).output().unwrap();
    assert!(pyout.status.success());
    // CPython's Windows stdout is text-mode CRLF; the Aoxn driver emits LF.
    let norm = |b: &[u8]| -> Vec<u8> { b.iter().copied().filter(|&c| c != b'\r').collect() };
    assert_eq!(norm(&out.stdout), norm(&pyout.stdout));
    fs::remove_dir_all(&dir).unwrap();
}