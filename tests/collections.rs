//! stdlib/collections.ax: deque ring buffer (growth, wraparound, pops,
//! string views), Counter (ties, truncation, totals), defaultdict helpers,
//! OrderedDict move-to-end over the insertion-ordered dict.

use std::{fs, path::PathBuf, process::Command};

const EXE: &str = if cfg!(windows) { ".exe" } else { "" };

const SRC: &str = r#"
import * from "stdlib"
import * from "stdlib/collections"

def main() -> int:
    dq = dq_new()
    p = dq_pop_front(dq)
    print("empty", p.ok, p.value)
    dq = p.dq
    i = 1
    while i <= 5:
        dq = dq_push_back(dq, i)
        i = i + 1
    dq = dq_push_front(dq, 0)
    print("len", dq_len(dq))
    print("at0", dq_get(dq, 0))
    print("at5", dq_get(dq, 5))
    p = dq_pop_front(dq)
    dq = p.dq
    print("pf", p.ok, p.value)
    p = dq_pop_back(dq)
    dq = p.dq
    print("pb", p.ok, p.value)
    i = 0
    while i < 20:
        dq = dq_push_back(dq, 100 + i)
        p = dq_pop_front(dq)
        dq = p.dq
        i = i + 1
    print("after-rotate-len", dq_len(dq))
    print("after-rotate-head", dq_get(dq, 0))
    dq_free(dq)
    sd = dq_new()
    sd = dq_push_back_str(sd, "hello")
    sd = dq_push_front_str(sd, "world")
    print("str0", dq_get_str(sd, 0))
    print("str1", dq_get_str(sd, 1))
    dq_free(sd)
    c = counter_new()
    c = counter_add(c, "a")
    c = counter_add(c, "b")
    c = counter_add(c, "a")
    c = counter_add(c, "c")
    c = counter_add(c, "b")
    c = counter_add(c, "a")
    print("a", counter_get(c, "a"))
    print("miss", counter_get(c, "z"))
    print("total", counter_total(c))
    mc = counter_most_common(c, -1)
    i = 0
    while i < mc.n:
        print("mc", vec_get_str(mc.keys, i), vec_get(mc.counts, i))
        i = i + 1
    mc2 = counter_most_common(c, 1)
    print("top1", vec_get_str(mc2.keys, 0), vec_get(mc2.counts, 0))
    d: dict[int] = {}
    print("dd1", dd_int_get(d, "k", 7))
    print("dd2", dd_int_get(d, "k", 9))
    ds: dict[string] = {}
    print("dds", dd_str_get(ds, "s", "init"))
    od: dict[int] = {}
    od["x"] = 1
    od["y"] = 2
    od["z"] = 3
    od_move_to_end_i(od, "x")
    for k in od:
        print("od", k)
    print("DONE")
    return 0
"#;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn collections_deque_counter_and_dicts() {
    let dir = root().join("target").join(format!("collections-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("collections_driver.ax");
    fs::write(&src, SRC).unwrap();
    let exe = dir.join(format!("collections_driver{EXE}"));
    aoxn::build_paths_opts(&[src.display().to_string()], &exe, true, &[], &[])
        .unwrap_or_else(|d| panic!("collections driver failed: {d:?}"));
    let out = Command::new(&exe).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let expected = "\
empty 0 0
len 6
at0 0
at5 5
pf 1 0
pb 1 5
after-rotate-len 4
after-rotate-head 116
str0 world
str1 hello
a 3
miss 0
total 6
mc a 3
mc b 2
mc c 1
top1 a 3
dd1 7
dd2 7
dds init
od y
od z
od x
DONE
";
    assert_eq!(String::from_utf8_lossy(&out.stdout), expected);
    fs::remove_dir_all(&dir).unwrap();
}