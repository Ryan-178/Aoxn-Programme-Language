//! Robustness pins for a 1.0 release: arbitrary input must never panic the
//! compiler. `check`/`c` are advertised to surface problems as diagnostics
//! (`stage: internal` at worst), never as a Rust panic, because an IDE or an
//! agent drives them over whatever bytes are on disk.
//!
//! The cases below are DETERMINISTIC (fixed xorshift seed, no clock, no OS
//! randomness), so a failure reproduces byte-for-byte from the printed input.
//! They exercise the lexer/parser/checker/codegen chain only: `compile_to_c`
//! never loads imports, so no case touches the filesystem.

use std::panic::{catch_unwind, AssertUnwindSafe};

fn next(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

fn pick<'a>(state: &mut u64, items: &[&'a str]) -> &'a str {
    items[(next(state) % items.len() as u64) as usize]
}

const FRAGMENTS: &[&str] = &[
    "def", "main", "f", "(", ")", "->", "int", "float", "string", "bool", "void",
    ":", "\n", "\n    ", "\n        ", "if", "elif", "else", "while", "for", "in",
    "range", "return", "pass", "break", "continue", "x", "y", "z", "d",
    "dict[int]", "[int; 3]", "[", "]", "{", "}", "\"a\"", "\"\\n\"", "\"\\z\"",
    "0x", "0b", "0xFF", "0b102", "1.5", "42", "None", "|", "struct", "P",
    "extern", "import", "*", "from", "\"stdlib\"", "try", "except", "as",
    "raise", "assert", "len", ",", ".", "+", "-", "==", "!=", "<", "=",
    "is", "not", "and", "or", "&", "^", "<<", "%", "//", "**", "@", "$",
    "\\", "\u{e9}", "\r", "\t", "\u{feff}", "f\"{x}\"", "f\"{x:.2f}\"",
    "#", "# comment", "if x is None:", "d[\"k\"]", "a[1]", "a[-1]", "a[0:1]",
    "\n\n", "  ", "\u{0}", "\"unterminated", "'", "?", "~", "!", ";",
];

/// A valid program used as a mutation base, so truncation/insertion lands in
/// real grammar rather than only in a token soup.
const BASE: &str = "\
import * from \"stdlib\"

struct P:
    x: int
    y: int

def twice(v: int) -> int:
    return v * 2

def main() -> int:
    a: [int; 3] = [1, 2, 3]
    d: dict[int] = {}
    d[\"k\"] = 1
    b = [twice(v) for v in a]
    c: dict[int] = {k: v for k in d if v > 0}
    s = f\"n={len(b)}\"
    if 1 < 2 <= 3:
        print(s)
    try:
        raise \"boom\"
    except as e:
        print(e)
    return b[0] + c[\"k\"] + d[\"k\"]
";

fn run(src: &str) -> Result<(), String> {
    match catch_unwind(AssertUnwindSafe(|| aoxn::compile_to_c(src, true))) {
        Ok(Ok(_)) | Ok(Err(_)) => Ok(()),
        Err(_) => Err(src.to_string()),
    }
}

fn char_boundary(src: &str, mut n: usize) -> usize {
    if n >= src.len() {
        return src.len();
    }
    while n > 0 && !src.is_char_boundary(n) {
        n -= 1;
    }
    n
}

#[test]
fn arbitrary_sources_never_panic_the_compiler() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));

    let mut failures = Vec::new();
    let mut case = 0u32;

    for seed in [
        0x9E3779B97F4A7C15u64,
        0x123456789ABCDEF1,
        0xDEADBEEF12345678,
        0x0F0F0F0F0F0F0F0F,
        0xCAFEBABE5A5A5A5A,
    ] {
        let mut state = seed;
        for _ in 0..2000u32 {
            case += 1;
            let mode = next(&mut state) % 3;
            let src = if mode == 0 {
                // token soup
                let n = 1 + (next(&mut state) % 60) as usize;
                let mut s = String::new();
                for _ in 0..n {
                    s.push_str(pick(&mut state, FRAGMENTS));
                }
                s
            } else if mode == 1 {
                // truncate the base anywhere (char boundary safe)
                let cut = char_boundary(BASE, (next(&mut state) as usize) % (BASE.len() + 1));
                BASE[..cut].to_string()
            } else {
                // splice random fragments into the base at a char boundary
                let cut = char_boundary(BASE, (next(&mut state) as usize) % (BASE.len() + 1));
                let mut s = String::from(&BASE[..cut]);
                for _ in 0..(1 + next(&mut state) % 4) {
                    s.push_str(pick(&mut state, FRAGMENTS));
                }
                s.push_str(&BASE[cut..]);
                s
            };

            if let Err(src) = run(&src) {
                failures.push(format!("case {case} (seed {seed:#x}): {src:?}"));
                if failures.len() >= 5 {
                    break;
                }
            }
        }
        if failures.len() >= 5 {
            break;
        }
    }

    std::panic::set_hook(prev);
    assert!(
        failures.is_empty(),
        "compiler panicked on arbitrary input:\n{}",
        failures.join("\n")
    );
}

#[test]
fn lexer_edges_do_not_panic_the_compiler() {
    // Concrete pins for the two `adv!` bounds bugs the fuzz found: a lone
    // CR at EOF and an f-string interpolation whose closing quote is the
    // last character. Both used to index one past the buffer.
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));

    let mut failures = Vec::new();
    for src in [
        "\r",
        "x = 1\r",
        "\r\n",
        "f\"{x}\"",
        "f\"{\"",
        "f\"{x:",
        "f\"{}\"",
        "f\"{x}\" \r",
        "\u{feff}x = 1",
        "\u{0}",
        "\"\\z\"",
        "0b102",
        "0x",
        "def f(:\n    pass\n",
    ] {
        if let Err(s) = run(src) {
            failures.push(format!("{s:?}"));
        }
    }

    std::panic::set_hook(prev);
    assert!(
        failures.is_empty(),
        "compiler panicked on lexer edge input:\n{}",
        failures.join("\n")
    );
}

#[test]
fn random_bytes_never_panic_the_lexer() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));

    let mut failures = Vec::new();
    let mut case = 0u32;
    for seed in [0xDEADBEEF12345678u64, 0x0123456789ABCDEF, 0xFEEDFACECAFEBEEF] {
        let mut state = seed;
        for _ in 0..1000u32 {
            case += 1;
            let n = 1 + (next(&mut state) % 400) as usize;
            let mut s = String::new();
            for _ in 0..n {
                let b = (next(&mut state) % 256) as u8;
                s.push(b as char);
            }
            if let Err(src) = run(&s) {
                failures.push(format!("case {case} (seed {seed:#x}): {src:?}"));
                if failures.len() >= 5 {
                    break;
                }
            }
        }
        if failures.len() >= 5 {
            break;
        }
    }

    std::panic::set_hook(prev);
    assert!(
        failures.is_empty(),
        "compiler panicked on random bytes:\n{}",
        failures.join("\n")
    );
}
