//! stdlib/re.ax: backtracking regex subset, pinned vectors plus a differential
//! comparison against Python's `re` for the supported syntax (the canonical
//! fixtures below are pinned regardless of Python availability).

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const EXE: &str = if cfg!(windows) { ".exe" } else { "" };

const DRIVER: &str = r#"
import * from "stdlib/re.ax"

def hex_str(s: string) -> string:
    d = "0123456789abcdef"
    w = sb_new(len(s) * 2 + 8)
    i = 0
    while i < len(s):
        b = str_get(s, i)
        w = sb_byte(w, str_get(d, b / 16))
        w = sb_byte(w, str_get(d, b % 16))
        i = i + 1
    return sb_text(w)

def groups_line(m: ReMatch) -> string:
    w = sb_new(64)
    w = sb_str(w, "OK " + str(re_start(m, 0)) + " " + str(re_end(m, 0)))
    g = 0
    n = re_groups(m)
    while g <= n:
        w = sb_str(w, " " + hex_str(re_group(m, g)))
        g = g + 1
    return sb_text(w)

def main() -> int:
    mode = arg(0)
    pat = arg(1)
    if mode == "err":
        r = re_compile(pat)
        if r.err != 0:
            print("ERR")
        else:
            print("OK")
        return 0
    subj = arg(2)
    if mode == "match" or mode == "search" or mode == "full":
        m = ReMatch(ok=0, src="", caps=vec_new())
        if mode == "match":
            m = re_match(pat, subj)
        elif mode == "search":
            m = re_search(re_compile(pat), subj, 0)
        else:
            m = re_fullmatch(pat, subj)
        if m.ok == 1:
            print(groups_line(m))
        else:
            print("NO")
        return 0
    if mode == "findall":
        v = re_findall(pat, subj)
        i = 0
        while i < v.len:
            print("F" + hex_str(vec_get_str(v, i)))
            i = i + 1
        print("DONE")
        return 0
    if mode == "sub":
        repl = arg(3)
        print("S" + hex_str(re_sub(pat, repl, subj)))
        return 0
    if mode == "split":
        v = re_split(pat, subj)
        i = 0
        while i < v.len:
            print("P" + hex_str(vec_get_str(v, i)))
            i = i + 1
        print("DONE")
        return 0
    print("unknown mode")
    return 2
"#;

const REF: &str = r#"
import re, sys
mode = sys.argv[1]; pat = sys.argv[2]
def hx(s): return s.encode('utf-8').hex()
if mode == 'err':
    try:
        re.compile(pat); print('OK')
    except re.error:
        print('ERR')
    sys.exit(0)
subj = sys.argv[3]
rx = re.compile(pat)
if mode in ('match', 'search', 'full'):
    if mode == 'match': m = rx.match(subj)
    elif mode == 'search': m = rx.search(subj)
    else: m = rx.fullmatch(subj)
    if m is None:
        print('NO')
    else:
        parts = ['OK', str(m.start()), str(m.end())]
        for g in range(0, rx.groups + 1):
            gg = m.group(g)
            parts.append(hx(gg if gg is not None else ''))
        print(' '.join(parts))
    sys.exit(0)
if mode == 'findall':
    for m in rx.finditer(subj):
        print('F' + hx(m.group(0) if rx.groups == 0 else (m.group(1) or '')))
    print('DONE')
    sys.exit(0)
if mode == 'sub':
    print('S' + hx(rx.sub(sys.argv[4], subj)))
    sys.exit(0)
if mode == 'split':
    for piece in rx.split(subj):
        print('P' + hx(piece))
    print('DONE')
    sys.exit(0)
print('unknown mode')
sys.exit(2)
"#;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn build_driver(dir: &Path) -> PathBuf {
    let driver = dir.join("re_driver.ax");
    fs::write(&driver, DRIVER).unwrap();
    let exe = dir.join(format!("re_driver{EXE}"));
    aoxn::build_paths_opts(&[driver.display().to_string()], &exe, true, &[], &[])
        .unwrap_or_else(|d| panic!("re driver failed to compile: {d:?}"));
    exe
}

fn run(exe: &Path, args: &[&str]) -> String {
    let out = Command::new(exe).args(args).output().unwrap();
    assert!(out.status.success(), "driver {:?}: {}", args, String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n")
}

fn python_available() -> Option<String> {
    for cand in ["python", "python3"] {
        if Command::new(cand).arg("--version").output().map(|o| o.status.success()).unwrap_or(false) {
            return Some(cand.to_string());
        }
    }
    None
}

fn python_ref(python: &str, dir: &Path, args: &[&str]) -> String {
    let script = dir.join("re_ref.py");
    fs::write(&script, REF).unwrap();
    let out = Command::new(python).arg(&script).args(args).output().unwrap();
    assert!(out.status.success(), "python {:?}: {}", args, String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n")
}

#[test]
fn re_matches_python_on_the_supported_subset() {
    let dir = root().join("target").join(format!("re-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let exe = build_driver(&dir);
    let python = python_available();

    // (mode, pattern, subject) for match/search/full/findall/split.
    let cases: &[(&str, &str, &str)] = &[
        ("match", "abc", "abc"),
        ("match", "abc", "xabc"),
        ("search", "abc", "xxabcyy"),
        ("search", "abc", "nope"),
        ("match", "a.c", "abc"),
        ("match", "a.c", "a\nc"),
        ("match", "ab*c", "ac"),
        ("match", "ab*c", "abbbc"),
        ("match", "ab+c", "ac"),
        ("match", "ab+c", "abbc"),
        ("match", "ab?c", "ac"),
        ("match", "ab?c", "abc"),
        ("match", "a.*c", "abcabc"),
        ("match", "a.*?c", "abcabc"),
        ("match", "a.+?c", "abcabc"),
        ("match", "cat|dog", "dog"),
        ("match", "cat|dog", "cat"),
        ("search", "cat|dog", "a dog here"),
        ("match", "(a)(b)", "ab"),
        ("match", "(a|b)+", "abba"),
        ("match", "(?:ab)+", "abab"),
        ("match", "(a(b))c", "abc"),
        ("match", "[a-c]+", "abc"),
        ("match", "[^0-9]+", "abc"),
        ("search", "[^0-9]+", "123abc456"),
        ("match", "[0-9]{3}", "123"),
        ("match", "[0-9]{3}", "12"),
        ("match", "a{2,4}", "aaaaa"),
        ("match", "a{2,}", "aaaaa"),
        ("match", "a{3}", "aaa"),
        ("match", "a{2,4}?", "aaaaa"),
        ("match", "^abc$", "abc"),
        ("match", "^abc$", "xabc"),
        ("match", "abc$", "abc\n"),
        ("search", "\\bcat\\b", "a cat here"),
        ("search", "\\bcat\\b", "concat"),
        ("search", "\\Bcat", "concat"),
        ("match", "\\d+", "123"),
        ("search", "\\d+", "ab123"),
        ("match", "\\w+", "hello_9"),
        ("match", "\\s+", " \t\n"),
        ("match", "\\D+", "abc"),
        ("match", "\\W+", "!!"),
        ("match", "\\S+", "abc"),
        ("match", "[\\d]+", "123"),
        ("match", "[a-z0-9_]+", "ab_9"),
        ("match", "[[:alpha:]]", "a]"),
        ("match", "[[:alpha:]]", ":]"),
        ("match", "[]]+", "]"),
        ("match", "[a-]+", "-"),
        ("match", "a\\.b", "a.b"),
        ("match", "a\\.b", "axb"),
        ("match", "\\n", "\n"),
        ("match", "\\t", "\t"),
        ("full", "a+", "aaa"),
        ("full", "a+", "aaab"),
        ("full", "(a|b)+", "abba"),
        ("findall", "\\d+", "a1 b22 c333"),
        ("findall", "\\d+", "none"),
        ("findall", "(\\d)(\\d)", "12 34"),
        ("findall", "x*", "abc"),
        ("findall", "[a-z]+", "hi there"),
        ("split", ",", "a,b,"),
        ("split", ",", "abc"),
        ("split", "\\s+", "a b  c"),
        ("split", "", "ab"),
        ("split", "x*", "abc"),
    ];
    for (mode, pat, subj) in cases {
        let args = [*mode, *pat, *subj];
        let got = run(&exe, &args);
        if let Some(py) = &python {
            assert_eq!(got, python_ref(py, &dir, &args), "case {mode} {pat:?} {subj:?}");
        }
    }

    // sub with replacement backrefs / escapes in the common subset.
    let subs: &[(&str, &str, &str)] = &[
        ("\\d+", "#", "a1 b22"),
        ("(a)(b)", "\\2\\1", "ab"),
        ("(a)(b)", "\\g<0>", "ab"),
        ("(a)(b)", "\\g<2>\\g<1>", "ab"),
        ("a", "\\n", "a"),
        ("a", "\\\\", "a"),
        ("a", "\\&", "a"),
        ("x*", "-", "abc"),
        ("(\\w+)@(\\w+)", "\\2.\\1", "user@host"),
        ("a+", "X", "baaac"),
    ];
    for (pat, repl, subj) in subs {
        let args = ["sub", *pat, *subj, *repl];
        let got = run(&exe, &args);
        if let Some(py) = &python {
            assert_eq!(got, python_ref(py, &dir, &args), "sub {pat:?} {repl:?} {subj:?}");
        }
    }

    // Patterns invalid in BOTH engines -> compile error.
    for pat in ["[", "a{2,1}", "*", "(abc", "a\\", "[z-a]", "+a", "a**"] {
        let args = ["err", pat];
        let got = run(&exe, &args);
        assert_eq!(got, "ERR\n", "expected compile error for {pat:?}, got {got}");
        if let Some(py) = &python {
            assert_eq!(got, python_ref(py, &dir, &args), "err {pat:?}");
        }
    }

    // Pinned values, independent of Python.
    assert_eq!(run(&exe, &["search", "abc", "xxabcyy"]), "OK 2 5 616263\n");
    assert_eq!(run(&exe, &["match", "(a)(b)", "ab"]), "OK 0 2 6162 61 62\n");
    assert_eq!(run(&exe, &["match", "a.*?c", "abcabc"]), "OK 0 3 616263\n");
    assert_eq!(run(&exe, &["match", "a.*c", "abcabc"]), "OK 0 6 616263616263\n");
    assert_eq!(run(&exe, &["sub", "(a)(b)", "ab", "\\2\\1"]), "S6261\n");
    assert_eq!(run(&exe, &["sub", "x*", "abc", "-"]), "S2d612d622d632d\n");
    assert_eq!(run(&exe, &["findall", "\\d+", "a1 b22 c333"]), "F31\nF3232\nF333333\nDONE\n");
    assert_eq!(run(&exe, &["split", ",", "a,b,"]), "P61\nP62\nP\nDONE\n");

    // Unsupported-but-Python-valid features must be a clean compile error,
    // never a silent misparse (pinned, not differential).
    for pat in ["(?=a)", "(?!a)", "(?i)a", "(?P<n>a)", "a\\1", "\\x41", "\\u0041"] {
        let got = run(&exe, &["err", pat]);
        assert_eq!(got, "ERR\n", "unsupported feature must error: {pat:?}, got {got}");
    }

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn re_module_is_selfhost_compilable() {
    let Some(clang) = aoxn::find_clang() else {
        assert!(std::env::var_os("AOXN_TEST_REQUIRE_CLANG").is_none(), "clang required");
        eprintln!("skipping: clang unavailable");
        return;
    };
    let dir = root().join("target").join(format!("re-selfhost-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let source = dir.join("re_selfhost.ax");
    fs::write(
        &source,
        r#"import * from "stdlib/re.ax"
def main():
    m = re_search(re_compile("[0-9]+"), "abc123def", 0)
    print(re_group(m, 0))
    print(re_sub("a+", "X", "baaac"))
    v = re_findall("\\w+", "hi there")
    print(v.len)
"#,
    )
    .unwrap();
    let cli = dir.join(format!("selfhost-cli{EXE}"));
    aoxn::build_paths_exe(&[root().join("selfhost/cli.ax").display().to_string()], &cli, true)
        .expect("build the selfhost CLI");
    let out = Command::new(&cli)
        .current_dir(&dir)
        .env("AOXN_STDLIB", root().join("stdlib"))
        .env("AOXN_CLANG", &clang)
        .args(["run", &source.display().to_string()])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "selfhost re failed: {} {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(text, "123\nbXc\n2\n", "{text}");
    fs::remove_dir_all(&dir).unwrap();
}