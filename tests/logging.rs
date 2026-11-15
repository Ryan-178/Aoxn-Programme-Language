//! stdlib/logging.ax: level filtering, append-to-file, timestamp shape,
//! level names, stdout channel.

use std::{fs, path::PathBuf, process::Command};

const EXE: &str = if cfg!(windows) { ".exe" } else { "" };

const SRC: &str = r#"
import * from "stdlib"
import * from "stdlib/logging"

def count_lines(s: string) -> int:
    n = len(s)
    c = 0
    i = 0
    while i < n:
        if s[i] == "\n":
            c = c + 1
        i = i + 1
    return c

def has(s: string, sub: string) -> int:
    n = len(s)
    m = len(sub)
    if m == 0:
        return 1
    if m > n:
        return 0
    i = 0
    while i <= n - m:
        j = 0
        ok = 1
        while j < m:
            if s[i + j] != sub[j]:
                ok = 0
                j = m
            else:
                j = j + 1
        if ok == 1:
            return 1
        i = i + 1
    return 0

def main() -> int:
    lg = log_new("t")
    lg = log_set_stdout(lg, 0)
    lg = log_add_file(lg, "log_test.log")
    log_debug(lg, "hidden")
    log_info(lg, "shown")
    log_warning(lg, "warned")
    text = read_file("log_test.log")
    print("lines", count_lines(text))
    print("info", has(text, "INFO t: shown"))
    print("warn", has(text, "WARNING t: warned"))
    print("nodebug", has(text, "DEBUG"))
    stamp = 0
    if text[4] == "-" and text[7] == "-" and text[10] == " " and text[13] == ":":
        stamp = 1
    print("stamp", stamp)
    lg = log_set_level(lg, LOG_DEBUG())
    log_debug(lg, "now visible")
    text = read_file("log_test.log")
    print("lines3", count_lines(text))
    print("debug", has(text, "DEBUG t: now visible"))
    lg2 = log_new("t")
    lg2 = log_set_stdout(lg2, 0)
    lg2 = log_add_file(lg2, "log_test.log")
    log_error(lg2, "appended")
    text = read_file("log_test.log")
    print("append", count_lines(text))
    print("levels", log_level_name(LOG_DEBUG()), log_level_name(LOG_INFO()), log_level_name(LOG_WARNING()), log_level_name(LOG_ERROR()), log_level_name(LOG_CRITICAL()))
    print("DONE")
    lg = log_set_stdout(lg, 1)
    log_info(lg, "to-stdout")
    return 0
"#;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn logging_levels_append_and_format() {
    let dir = root().join("target").join(format!("logging-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("logging_driver.ax");
    fs::write(&src, SRC).unwrap();
    let exe = dir.join(format!("logging_driver{EXE}"));
    aoxn::build_paths_opts(&[src.display().to_string()], &exe, true, &[], &[])
        .unwrap_or_else(|d| panic!("logging driver failed: {d:?}"));
    let out = Command::new(&exe).current_dir(&dir).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8_lossy(&out.stdout);
    let expected = "\
lines 2
info 1
warn 1
nodebug 0
stamp 1
lines3 3
debug 1
append 4
levels DEBUG INFO WARNING ERROR CRITICAL
DONE
";
    assert!(text.starts_with(expected), "{text}");
    assert!(text.contains("INFO t: to-stdout"), "{text}");
    let file = fs::read_to_string(dir.join("log_test.log")).unwrap();
    // the final to-stdout line ALSO lands in the file: lg keeps its file
    // handler when stdout is re-enabled (both channels are independent)
    assert_eq!(file.lines().count(), 5);
    assert!(file.contains("ERROR t: appended"));
    assert!(file.contains("INFO t: to-stdout"));
    fs::remove_dir_all(&dir).unwrap();
}