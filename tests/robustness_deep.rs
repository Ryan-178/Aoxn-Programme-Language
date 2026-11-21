//! Deeper robustness pins: mutate REAL programs instead of token soup, so the
//! truncated and spliced sources reach the resolver, the checker and the C
//! emitter with mostly-valid prefixes. `tests/robustness.rs` covers the lexer
//! and parser broadly; this one is about the stages behind them.
//!
//! Deterministic by construction: fixed seeds, no clock, no OS randomness, so
//! every failure reproduces byte-for-byte from the printed source.

use std::panic::{catch_unwind, AssertUnwindSafe};

fn next(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

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

/// Each entry must be a program the compiler accepts; the corpus test below
/// runs every one through `compile_to_c` before any mutation is applied, so a
/// base that silently stops being valid fails loudly instead of weakening the
/// mutations into lexer errors.
const BASES: &[&str] = &[
    // structs, arrays, generics-free helpers, for-in, accumulate
    "\
struct P:
    x: int
    y: float

def twice(v: int) -> int:
    return v * 2

def main() -> int:
    a: [int; 3] = [1, 2, 3]
    p = P(x=1, y=2.5)
    total = 0
    for v in a:
        total += twice(v)
    if p.x > 0:
        print(p.x, p.y, total)
    return total
",
    // tuples and multiple assignment
    "\
def pair() -> (int, float):
    return (1, 2.5)

def main() -> int:
    a, b = pair()
    return a
",
    // dict, None narrowing, dict_has
    "\
def maybe(flag: bool) -> int | None:
    if flag:
        return 7
    return None

def main() -> int:
    d: dict[int] = {}
    d[\"k\"] = 1
    if dict_has(d, \"k\"):
        d[\"k\"] += 2
    m = maybe(True)
    if m is not None:
        return m + d[\"k\"]
    return 0
",
    // function pointer, comprehension, dict comprehension, f-string
    "\
def twice(v: int) -> int:
    return v * 2

def main() -> int:
    a: [int; 3] = [1, 2, 3]
    b = [twice(v) for v in a]
    c: dict[int] = {str(k): k for k in b if k > 2}
    s = f\"n={len(b)} {2.5:.2f}\"
    print(s)
    return c[\"4\"]
",
    // raise, try/except, function pointer call
    "\
def boom(n: int) -> int:
    if n < 0:
        raise \"negative\"
    return n

def main() -> int:
    cb: fn(int) -> int = boom
    try:
        print(cb(-1))
    except as e:
        print(e)
    return cb(2)
",
    // strings, slices, membership, byte loop
    "\
def main() -> int:
    s = \"hello\"
    t = s[1:3]
    n = 0
    for c in s:
        if c in \"lo\":
            n += 1
    if \"ell\" in s:
        print(t, n)
    return n
",
    // sizeof / addr_of / extern / casts
    "\
extern def sqrt(x: float) -> float

def main() -> int:
    x = 5
    size = sizeof(int)
    p = addr_of(x)
    store_i64(p, 9)
    print(size, x, to_int(sqrt(4.0)))
    return x
",
    // precedence: power, unary minus, bitwise, chained comparison, ternary
    "\
def main() -> int:
    a = 3
    b = a ** 2
    c = -2 ** 2
    d = (a & 1) == 1
    x = 1 if d else 0
    if 0 < a <= 3:
        x += b + c
    return x
",
    // while, break, continue, augmented assignment
    "\
def main() -> int:
    i = 0
    total = 0
    while i < 10:
        i += 1
        if i == 3:
            continue
        if i > 8:
            break
        total += i
    return total
",
];

const FRAGMENTS: &[&str] = &[
    ":", "(", ")", "[", "]", "{", "}", "\"", "'", "0x", "0b", "\\", "|",
    "None", "not ", "is ", "-", "+", "**", "//", "%", "<<", ">>", "^", "&",
    "f\"{", "raise ", "try:", "except as ", "elif ", "else:", "->", ",", ".",
    "@", "$", "\u{feff}", "\u{0}", "\r", "\n    ", "\n", "def ", "struct ",
    "import * from ", "\"stdlib\"", "[int; 3]", "dict[int]",
    "fn(int) -> int", "sizeof(", "addr_of(", " as ", "return ", "pass", "for ",
    " in ", "range(", "assert(", "exit(", "len(", "\"unterminated",
];

fn splice_lines(base: &str, state: &mut u64) -> String {
    let lines: Vec<&str> = base.split('\n').collect();
    if lines.is_empty() {
        return base.to_string();
    }
    let victim = (next(state) as usize) % lines.len();
    let mut out = String::new();
    for (i, l) in lines.iter().enumerate() {
        if i == victim {
            continue;
        }
        out.push_str(l);
        if i + 1 < lines.len() {
            out.push('\n');
        }
    }
    out
}

fn duplicate_line(base: &str, state: &mut u64) -> String {
    let lines: Vec<&str> = base.split('\n').collect();
    if lines.is_empty() {
        return base.to_string();
    }
    let victim = (next(state) as usize) % lines.len();
    let mut out = String::new();
    for (i, l) in lines.iter().enumerate() {
        out.push_str(l);
        if i == victim {
            out.push('\n');
            out.push_str(l);
        }
        if i + 1 < lines.len() {
            out.push('\n');
        }
    }
    out
}

fn splice_fragments(base: &str, state: &mut u64) -> String {
    let cut = char_boundary(base, (next(state) as usize) % (base.len() + 1));
    let mut s = String::from(&base[..cut]);
    for _ in 0..(1 + next(state) % 3) {
        s.push_str(FRAGMENTS[(next(state) % FRAGMENTS.len() as u64) as usize]);
    }
    s.push_str(&base[cut..]);
    s
}

#[test]
fn mutation_corpus_is_valid() {
    for (i, base) in BASES.iter().enumerate() {
        if let Err(e) = aoxn::compile_to_c(base, true) {
            panic!("corpus entry {i} no longer compiles: {e:?}\n{base}");
        }
    }
}

#[test]
fn mutated_real_programs_never_panic_the_compiler() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));

    let mut failures = Vec::new();
    let mut case = 0u32;

    'outer: for seed in [
        0xA5A5A5A5A5A5A5A5u64,
        0x0BADF00DDEADBEEF,
        0x3141592653589793,
    ] {
        let mut state = seed;
        for base in BASES {
            for mode in 0..4u32 {
                for _ in 0..80u32 {
                    case += 1;
                    let src = match mode {
                        0 => {
                            let cut = char_boundary(
                                base,
                                (next(&mut state) as usize) % (base.len() + 1),
                            );
                            base[..cut].to_string()
                        }
                        1 => splice_lines(base, &mut state),
                        2 => duplicate_line(base, &mut state),
                        _ => splice_fragments(base, &mut state),
                    };
                    if let Err(src) = run(&src) {
                        failures.push(format!("case {case} (seed {seed:#x}): {src:?}"));
                        if failures.len() >= 5 {
                            break 'outer;
                        }
                    }
                }
            }
        }
    }

    std::panic::set_hook(prev);
    assert!(
        failures.is_empty(),
        "compiler panicked on a mutated real program:\n{}",
        failures.join("\n")
    );
}
