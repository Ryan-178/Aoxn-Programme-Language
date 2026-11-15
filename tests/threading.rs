//! stdlib/threading.ax + stdlib/futures.ax: spawn/join return values, a
//! mutex-guarded shared counter (4 threads x 1000 increments), ordered
//! parallel map, submit/wait futures — plus a selfhost-CLI pin (fn
//! pointers + platform externs through stage 2).

use std::{fs, path::PathBuf, process::Command};

const EXE: &str = if cfg!(windows) { ".exe" } else { "" };

const SRC: &str = r#"
import * from "stdlib"
import * from "stdlib/threading"
import * from "stdlib/futures"

def w_double(x: int) -> int:
    return x * 2

def w_inc(p: int) -> int:
    m = Mutex(handle=load_i64(p), err=0)
    i = 0
    while i < 1000:
        mutex_lock(m)
        c = load_i64(p + 8)
        store_i64(p + 8, c + 1)
        mutex_unlock(m)
        i = i + 1
    return 0

def main() -> int:
    t = thread_spawn(w_double, 21)
    print("spawn-err " + str(t.err))
    t = thread_join(t)
    print("joined " + str(t.joined) + " " + str(t.result))
    t = thread_join(t)
    print("rejoin " + str(t.result))
    mu = mutex_new()
    print("mutex-err " + str(mu.err))
    block = malloc(16)
    store_i64(block, mu.handle)
    store_i64(block + 8, 0)
    ts = vec_new()
    i = 0
    while i < 4:
        t = thread_spawn(w_inc, block)
        ts = vec_push(ts, t.handle)
        i = i + 1
    i = 0
    while i < 4:
        t = Thread(handle=vec_get(ts, i), err=0, result=0, joined=0)
        t = thread_join(t)
        i = i + 1
    print("counter " + str(load_i64(block + 8)))
    mutex_free(mu)
    free(block)
    args = vec_new()
    i = 1
    while i <= 5:
        args = vec_push(args, i)
        i = i + 1
    r = fut_parallel(w_double, args)
    print("errors " + str(r.errors))
    i = 0
    while i < vec_len(r.values):
        print("v " + str(vec_get(r.values, i)))
        i = i + 1
    empty = vec_new()
    r2 = fut_parallel(w_double, empty)
    print("empty " + str(vec_len(r2.values)) + " " + str(r2.errors))
    fu = fut_submit(w_double, 50)
    fu = fut_wait(fu)
    print("future " + str(fu.done) + " " + str(fu.th.result))
    print("again " + str(fut_result(fu)))
    thread_sleep_ms(1)
    print("DONE")
    return 0
"#;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

const EXPECTED: &str = "\
spawn-err 0
joined 1 42
rejoin 42
mutex-err 0
counter 4000
errors 0
v 2
v 4
v 6
v 8
v 10
empty 0 0
future 1 100
again 100
DONE
";

#[test]
fn threading_and_futures() {
    let dir = root().join("target").join(format!("threading-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("threading_driver.ax");
    fs::write(&src, SRC).unwrap();
    let exe = dir.join(format!("threading_driver{EXE}"));
    aoxn::build_paths_opts(&[src.display().to_string()], &exe, true, &[], &[])
        .unwrap_or_else(|d| panic!("threading driver failed: {d:?}"));
    let out = Command::new(&exe).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout), EXPECTED);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn threading_runs_through_the_selfhost_cli() {
    let Some(clang) = aoxn::find_clang() else {
        assert!(
            std::env::var_os("AOXN_TEST_REQUIRE_CLANG").is_none(),
            "clang required"
        );
        eprintln!("skipping: clang unavailable");
        return;
    };
    let dir = root().join("target").join(format!("threading-selfhost-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("threading_driver.ax");
    fs::write(&src, SRC).unwrap();
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
        .args(["run", "threading_driver.ax"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "selfhost threading failed: {} / {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("counter 4000"), "{text}");
    assert!(text.ends_with("DONE\n"), "{text}");
    fs::remove_dir_all(&dir).unwrap();
}