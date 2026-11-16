//! Real Linux headers pin the raw layouts used by the portable Aoxn modules.
#![cfg(target_os = "linux")]

use std::path::PathBuf;
use std::process::{Command, Output};

const OS_PROGRAM: &str = r#"
import * from "@ROOT@/stdlib/os.ax"
import * from "@ROOT@/stdlib/time.ax"

def main() -> int:
    print(OS_LINUX_ST_MODE(), OS_LINUX_STAT_SIZE(), OS_LINUX_DIRENT_NAME())
    if os_linux_is_aarch64():
        print(1)
    else:
        print(0)
    assert(os_isdir("sandbox"))
    assert(os_isfile("sandbox/file"))
    assert(not os_isdir("sandbox/file"))
    assert(os_islink("sandbox/link"))
    assert(not os_exists("sandbox/missing"))
    entries = os_listdir("sandbox")
    found = False
    for i in range(entries.len):
        if vec_get_str(entries, i) == "file":
            found = True
    assert(found)
    assert(os_mkdir("sandbox/newdir"))
    assert(os_isdir("sandbox/newdir"))
    assert(write_file("sandbox/newdir/newfile", "linux ABI"))
    assert(os_isfile("sandbox/newdir/newfile"))
    assert(os_remove("sandbox/newdir/newfile"))
    assert(os_rmdir("sandbox/newdir"))
    assert(time_unix() > 1700000000)
    assert(time_now_ns() > 0)
    print("PASS linux-filesystem")
    return 0
"#;

const CPU_PROGRAM: &str = r#"
import * from "@ROOT@/selfhost/codegen.ax"

def main() -> int:
    print(clang_cpu_flag("native"))
    print(clang_cpu_flag("test-cpu"))
    return 0
"#;

const C_LAYOUT: &str = r#"
#include <stddef.h>
#include <stdio.h>
#include <sys/utsname.h>
#include <sys/stat.h>
#include <dirent.h>
#include <time.h>
_Static_assert(sizeof(struct stat) <= 144, "stat must fit Aoxn buffer");
_Static_assert(sizeof(((struct stat *)0)->st_mode) == 4, "Linux mode width");
_Static_assert(offsetof(struct dirent, d_name) == 19, "directory name offset");
_Static_assert(sizeof(struct timespec) == 16, "timespec buffer");
_Static_assert(offsetof(struct timespec, tv_nsec) == 8, "timespec nanoseconds");
_Static_assert(sizeof(struct utsname) == 390, "Linux uname buffer");
_Static_assert(offsetof(struct utsname, machine) == 260, "Linux machine offset");
int main(void) {
    struct utsname info;
    if (uname(&info) != 0) return 1;
    printf("%zu %zu %zu %zu %zu %zu %s\n",
           offsetof(struct stat, st_mode), sizeof(struct stat),
           offsetof(struct dirent, d_name), sizeof(struct timespec),
           sizeof(struct utsname), offsetof(struct utsname, machine), info.machine);
    return 0;
}
"#;

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn checked(command: &mut Command) -> Output {
    let output = command.output().expect("execute ABI fixture command");
    assert!(
        output.status.success(),
        "{command:?}: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn linux_headers_match_aoxn_layouts_and_native_operations() {
    let Some(clang) = aoxn::find_clang() else {
        assert!(
            std::env::var_os("AOXN_TEST_REQUIRE_CLANG").is_none(),
            "clang required"
        );
        eprintln!("SKIP Linux ABI fixture: clang not found");
        return;
    };
    let scratch =
        Scratch(std::env::temp_dir().join(format!("aoxn-linux-abi-{}", std::process::id())));
    std::fs::create_dir(&scratch.0).unwrap();
    let c = scratch.0.join("layout.c");
    let layout = scratch.0.join("layout");
    std::fs::write(&c, C_LAYOUT).unwrap();
    checked(
        Command::new(clang)
            .args(["-std=c11", "-Wall", "-Wextra", "-Werror"])
            .arg(&c)
            .arg("-o")
            .arg(&layout),
    );
    let output = checked(&mut Command::new(layout));
    let text = String::from_utf8(output.stdout).unwrap();
    let fields: Vec<_> = text.split_whitespace().collect();
    assert_eq!(fields.len(), 7, "{text}");
    let mode = match fields[6] {
        "aarch64" => 16,
        "x86_64" => 24,
        machine => panic!("unsupported Linux ABI fixture machine: {machine}"),
    };
    assert_eq!(fields[0].parse::<usize>().unwrap(), mode);
    assert!(fields[1].parse::<usize>().unwrap() <= 144);
    assert_eq!(&fields[2..6], &["19", "16", "390", "260"]);

    std::fs::create_dir(scratch.0.join("sandbox")).unwrap();
    std::fs::write(scratch.0.join("sandbox/file"), "fixture").unwrap();
    std::os::unix::fs::symlink("file", scratch.0.join("sandbox/link")).unwrap();
    let root = env!("CARGO_MANIFEST_DIR").replace('\\', "/");
    for (name, source, expected) in [
        (
            "os",
            OS_PROGRAM,
            format!(
                "{mode} 144 19\n{}\nPASS linux-filesystem\n",
                if fields[6] == "aarch64" { "1" } else { "0" }
            ),
        ),
        ("cpu", CPU_PROGRAM, {
            let flag = if fields[6] == "aarch64" {
                "-mcpu"
            } else {
                "-march"
            };
            format!("{flag}=native\n{flag}=test-cpu\n")
        }),
    ] {
        let source_path = scratch.0.join(format!("{name}.ax"));
        std::fs::write(&source_path, source.replace("@ROOT@", &root)).unwrap();
        // Run both optimizer settings: wrong raw offsets can hide in unoptimized code.
        for level in ["--O0", "--O3"] {
            let exe = scratch.0.join(format!("{name}{level}"));
            checked(
                Command::new(env!("CARGO_BIN_EXE_aoxn"))
                    .arg("build")
                    .arg(&source_path)
                    .arg("-o")
                    .arg(&exe)
                    .arg(level),
            );
            let output = checked(Command::new(exe).current_dir(&scratch.0));
            assert_eq!(
                String::from_utf8(output.stdout).unwrap(),
                expected,
                "{name} {level}"
            );
        }
    }
}
