# Rust interoperability (v0.55.0-snapshot-5)

Aoxn can compile Rust code by **driving `rustc`**, the same way it drives
`clang` for C. Importing a `.rs` file compiles it into a static library and
binds its exported C-ABI functions as ordinary Aoxn `extern def`
declarations. The full Rust language and ecosystem stay available inside the
`.rs` file; Aoxn sees only a narrow, explicit C ABI at the boundary.

```Aoxn
import * from "./native.rs"      # binds native.rs's #[no_mangle] extern "C" fns

def main() -> int:
    print(rs_add(2, 40))         # 42
    print(rs_greet())            # hello from rust
    return 0
```

```Rust
// native.rs
use std::os::raw::c_char;

#[no_mangle]
pub extern "C" fn rs_add(a: i64, b: i64) -> i64 {
    a + b
}

#[no_mangle]
pub extern "C" fn rs_greet() -> *const c_char {
    b"hello from rust\0".as_ptr() as *const c_char
}
```

`aoxn run main.ax` compiles `native.rs` with `rustc`, links the resulting
static library into the program, and runs it. Rust is an **optional external
toolchain**: a machine without `rustc` can still `check`, `c` and `symbols`
any program (the signature scan is pure Rust source analysis); only `build`
and `run` need `rustc`.

## How it fits the pipeline

The bridge lives in the **loader** (`src/rust.rs`), before typecheck. When
`load_file` reaches a `.rs` import it scans the source and synthesizes one
`FnDecl` (`is_extern: true`, empty body) per exported function. The file then
behaves like any other module:

- Every import form works: `import * from "./native.rs"`,
  `from native import rs_add`, `import native` then `native.rs_add(...)`.
- `aoxn symbols` reports the bindings with their source positions, so the
  IDE's outline, go-to-definition and symbol search work unchanged.
- By the time typecheck and `codegen_c.rs` run, the bindings **are** ordinary
  externs. There is no codegen dispatch and therefore **no self-host mirror
  obligation** — the same construction CSS assets use (`docs/css-assets.md`).
  The self-hosted CLI does not resolve `.rs` imports, exactly as it does not
  resolve `.css` assets.

The scan needs no `rustc`. `build`/`run` additionally invoke
`rustc --edition=2021 --crate-type staticlib --crate-name aoxnrust
-C opt-level=<N> --print native-static-libs` and link the artifact.

## The ABI surface

Only exact-ABI pairs are accepted. Aoxn `int` is C `long long` (64-bit),
`float` is C `double`, `string` is C `char*`:

| Rust signature type | Aoxn type | C text |
|---|---|---|
| `i64` | `int` | `long long` |
| `f64` | `float` | `double` |
| `*const c_char` / `*mut c_char` | `string` | `char*` |
| `()` / no return | `void` | `void` |

`c_char` may be spelled `std::ffi::c_char`, `core::ffi::c_char` or
`std::os::raw::c_char`, with or without a leading `::`.

Everything else is a **diagnostic, not a guess**:

- `bool` is rejected on purpose: Rust `bool` is one byte but Aoxn `bool` is
  C `int`, so a mapping would be a silent ABI bug. Use `i64` (0/1).
- `i32`, `u64`, `String`, `&str`, `*const i64` and other unsupported types
  are named in the diagnostic.
- Generic functions (`fn f<T>(...)`), `where` clauses (before or after the
  return type), `async fn`, variadics (`...`), tuple/destructuring parameter
  patterns, `#[export_name]`, raw identifiers (`r#name`), C keywords and
  `main` as a symbol name, and any ABI other than `extern "C"`
  (`"C-unwind"` unwinds into frames that cannot catch it) are all rejected.
- An `extern "C" fn` **without** `#[no_mangle]` is rejected: its symbol would
  be mangled and the verbatim C declaration would not link.

A function that is not `extern "C"` (a private Rust `fn`, a `pub fn` with
Rust ABI) binds nothing and is ignored — it still compiles as part of the
crate, it just is not callable from Aoxn.

## Boundaries

- **Top-level exports only.** The scanner binds `#[no_mangle] extern "C"`
  functions at the crate root. It does not descend into an inline `mod { ... }`
  block: those functions still compile and link, but the bridge does not
  generate bindings for them. Put exports at the top level of the file.
- **Single file.** The bridge compiles one self-contained `.rs` file. A
  `mod name;` declaration (which reads a sibling file) and the
  `include!` / `include_str!` / `include_bytes!` macros are rejected: they
  pull files into the crate that the artifact cache key cannot see. Inline
  the module, or keep the crate in one file.
- **Comments, strings and macro bodies are not declarations.** The scanner
  is comment- and string-aware (including nested block comments and raw
  strings), so `fn` text inside them binds nothing.

## Toolchain discovery and diagnostics

`rustc` is located like `clang`: `AOXN_RUSTC` env → `PATH` → rustup's default
`~/.cargo/bin` (`%USERPROFILE%\.cargo\bin` on Windows). An unset or invalid
`AOXN_RUSTC` falls through to the next probe.

- A missing `rustc` at `build`/`run` is a `cc`-stage diagnostic naming
  `AOXN_RUSTC` and noting that `check`/`c`/`symbols` work without it.
- A signature the scanner rejects is a `parse`-stage diagnostic with the
  function's source position.
- A body `rustc` rejects is a `cc`-stage diagnostic carrying `rustc`'s own
  message (`rustc rejected '<file>': ...`). `rustc`'s warnings are surfaced;
  the machine-only `native-static-libs` note is not.

`aoxn doctor` reports `rustc` as an **optional** row (version when found,
"not found (optional)" otherwise); it never fails the doctor report.

## Linking and the native-library note

`rustc --print native-static-libs` prints the platform libraries the static
library needs (Windows: `kernel32.lib ntdll.lib userenv.lib ws2_32.lib
dbghelp.lib /defaultlib:msvcrt`; Linux: `-lgcc_s -lutil ...`). The link step
adds these after the user's own `-l` flags:

- `-lfoo` and `foo.lib` tokens become `-lfoo`.
- A `/defaultlib:NAME` token becomes the linker **directive**
  `-Wl,/DEFAULTLIB:NAME`, **not** `-lNAME`. Translating it to an input library
  adds the dynamic CRT import lib (`msvcrt.lib`) as a primary input, which
  shadows the static `libcmt` that provides `printf` and breaks every link
  with `undefined symbol: printf`. The directive form preserves rustc's CRT
  guidance while letting the CRT clang already chose win.

## Caching

- The compiled static library is cached under the build cache's `rust/`
  subdirectory: `<cache>/rust/<16-hex-key>/aoxnrust.lib` (`.a` on Linux) plus
  a `native-libs.txt` note. The key covers the file's canonical path, its
  content digest, the `rustc` identity and the optimization level, so an
  unchanged rebuild never spawns `rustc` twice.
- The `rustc` identity is the `rustc --version` string, **not** the binary's
  path/size/mtime: `~/.cargo/bin/rustc` is a rustup shim whose metadata
  survives every `rustup update` — only the version string notices the
  toolchain changed.
- Artifacts are staged under `<key>.<pid>` and published by rename, so
  concurrent builds never link a half-written library. The cache is pruned
  to 32 entries (approximate LRU). `AOXN_NO_CACHE=1` bypasses it in both
  directions; `AOXN_CACHE_DIR` relocates it.
- The executable cache key gains the `rustc` identity **only when the
  program has `.rs` dependencies**, so a pure-Aoxn program's cache key is
  unchanged. `.rs` files participate in the dependency walk as leaves
  (their content is hashed; they import nothing).

## Tests and CI

`tests/rust_interop.rs` covers the ABI mapping, every import form, the
scanner's rejection table, comment/string/private-item handling, the
`check`/`c`/`symbols`-without-`rustc` path, `rustc` error surfacing, artifact
cache hit/invalidation, and the dependency walk. The end-to-end tests gate on
**both** toolchains independently (`AOXN_TEST_REQUIRE_CLANG`,
`AOXN_TEST_REQUIRE_RUSTC` turn a missing toolchain from a skip into a
failure); CI sets both on Windows and Linux. The scanner and the
no-`rustc` paths run everywhere.