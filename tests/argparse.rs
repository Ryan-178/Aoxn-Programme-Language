//! stdlib/argparse.ax: option/flag/positional parsing, defaults, `--`,
//! inline and combined short values, help, value errors, ap_to_int, and the
//! exact help text.

use std::{fs, path::PathBuf, process::Command};

const EXE: &str = if cfg!(windows) { ".exe" } else { "" };

const SRC: &str = r#"
import * from "stdlib"
import * from "stdlib/argparse"

def mk() -> ArgParser:
    ap = ap_new("demo", "test parser")
    ap = ap_flag(ap, "verbose", "v", "chatty")
    ap = ap_opt(ap, "output", "o", "out.txt", "output")
    ap = ap_opt(ap, "count", "c", "1", "count")
    ap = ap_pos(ap, "input", 1, "input")
    ap = ap_pos(ap, "extra", 0, "extra")
    return ap

def parse_show(args: Vec):
    ap = mk()
    res = ap_parse(ap, args)
    if res.err != "":
        print("ERR " + res.err)
        return
    if res.help_requested == 1:
        print("HELP")
        return
    print("OK v=" + ap_get(res, "verbose", "?") + " o=" + ap_get(res, "output", "?") + " c=" + ap_get(res, "count", "?") + " in=" + ap_get(res, "input", "?") + " extra=" + ap_get(res, "extra", "?"))

def main() -> int:
    a = vec_new()
    a = vec_push_str(a, "--verbose")
    a = vec_push_str(a, "-o")
    a = vec_push_str(a, "result.bin")
    a = vec_push_str(a, "in.txt")
    a = vec_push_str(a, "--count=3")
    a = vec_push_str(a, "tail")
    parse_show(a)
    b = vec_new()
    b = vec_push_str(b, "in.txt")
    parse_show(b)
    c = vec_new()
    c = vec_push_str(c, "-c5")
    c = vec_push_str(c, "--")
    c = vec_push_str(c, "--not-an-option")
    parse_show(c)
    h = vec_new()
    h = vec_push_str(h, "-h")
    parse_show(h)
    e1 = vec_new()
    e1 = vec_push_str(e1, "--bogus")
    parse_show(e1)
    e2 = vec_new()
    e2 = vec_push_str(e2, "-z")
    parse_show(e2)
    e3 = vec_new()
    e3 = vec_push_str(e3, "in.txt")
    e3 = vec_push_str(e3, "--output")
    parse_show(e3)
    e4 = vec_new()
    parse_show(e4)
    e5 = vec_new()
    e5 = vec_push_str(e5, "a")
    e5 = vec_push_str(e5, "b")
    e5 = vec_push_str(e5, "c")
    parse_show(e5)
    e6 = vec_new()
    e6 = vec_push_str(e6, "--verbose=1")
    parse_show(e6)
    print(ap_to_int("42", -1))
    print(ap_to_int("-7", -1))
    print(ap_to_int("x9", -1))
    print(ap_to_int("", -1))
    print(ap_to_int("-", -1))
    print(ap_help(mk()))
    print("DONE")
    return 0
"#;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn argparse_parsing_errors_and_help() {
    let dir = root().join("target").join(format!("argparse-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("argparse_driver.ax");
    fs::write(&src, SRC).unwrap();
    let exe = dir.join(format!("argparse_driver{EXE}"));
    aoxn::build_paths_opts(&[src.display().to_string()], &exe, true, &[], &[])
        .unwrap_or_else(|d| panic!("argparse driver failed: {d:?}"));
    let out = Command::new(&exe).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let expected = "\
OK v=1 o=result.bin c=3 in=in.txt extra=tail
OK v=0 o=out.txt c=1 in=in.txt extra=
OK v=0 o=out.txt c=5 in=--not-an-option extra=
HELP
ERR unrecognized argument: --bogus
ERR unrecognized argument: -z
ERR option --output requires a value
ERR missing required argument: input
ERR unexpected positional argument: c
ERR flag --verbose takes no value
42
-7
-1
-1
-1
usage: demo [-h] [--verbose] [--output output] [--count count] input [extra]

test parser

  -v, --verbose    chatty
  -o, --output output    output (default: out.txt)
  -c, --count count    count (default: 1)
  input    input
  extra    extra

DONE
";
    assert_eq!(String::from_utf8_lossy(&out.stdout), expected);
    fs::remove_dir_all(&dir).unwrap();
}