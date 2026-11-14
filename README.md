# Aoxn

## English

### Project and current scope

Aoxn is a statically typed native language with Python-style indentation. The
Rust compiler emits ISO C99 and invokes external clang. The compiler library
`src/` has no external Rust crate dependencies or LLVM library dependency. The
independent package manager has dependencies pinned in `Cargo.lock`.

**Compiler 0.55.0; package-manager crate 0.43.0.** The current tree carries
**unreleased** language and library work (CHANGELOG `[Unreleased]`, no release
until the user asks):

- **`sizeof(T)` and `addr_of(x)`**. `sizeof` takes a TYPE argument and folds
  to the C representation's byte size at compile time (works inside generic
  bodies, substituting per instantiation); `addr_of` yields the live address
  of an int/float/bool/struct/array lvalue as a plain `int`, so writes through
  `store_i64`/`store_f64` are seen by the owner. This removes the
  language-level blocker for every non-8-byte-slot container (gap A2): a
  struct container can now be written in Aoxn with `sizeof` + `addr_of` +
  `memcpy`, though the stdlib `Vec` itself still moves 8-byte slots.
  Temporaries, strings, dicts, function pointers and any nullable-rooted
  lvalue (even narrowed) are rejected with named diagnostics.
- **`stdlib/re.ax`**, a backtracking regular-expression matcher covering a
  Python `re` subset (groups, classes, lazy quantifiers, anchors, boundaries;
  `match/search/fullmatch/findall/sub/split`), differential-tested against
  CPython and compilable by the self-hosted compiler.
- **The standard-library roadmap closes.** Ten more modules land in this
  batch — `shutil`, `collections`, `itertools`, `net` (urllib-style URL layer
  + HTTP facade), `threading`, `futures` (thread subset), `sys`, `argparse`,
  `logging` and `unittest` — so **all 31 Python-stdlib roadmap items in
  `docs/stdlib-todo.md` §2 are dispositioned: 27 shipped, 4 standing won't-do
  decisions with substitutes** (pickle → JSON, multiprocessing →
  socket/subprocess, traceback → compiler diagnostics, typing → the static
  type system). The §7 completion audit maps every item to its module, test
  and example. The 15 third-party libraries (§5) remain out of the stdlib
  scope.

The released 0.55.0 line shipped the pyproject-shaped `aoxn.toml` manifest
and pip-style dependency CLI (snapshot 6) and the **Rust bridge** (snapshot
5): `import "./native.rs"` compiles a self-contained Rust file with rustc as
a static library and binds its `#[no_mangle] extern "C"` functions as
ordinary Aoxn `extern def` declarations. None of this introduces C source
compatibility; see [C compatibility](docs/c-compatibility.md).

```python
import * from "stdlib"

struct Point:
    x: int
    y: int

def main() -> int:
    print(sizeof(int), sizeof(Point), sizeof([int; 4]))   # 8 16 32
    p = Point(x=3, y=4)
    a = addr_of(p.x)          # live address of the field
    store_i64(a, 30)
    print(p.x)                # 30
    return 0
```

```python
import * from "./native.rs"   # binds native.rs's #[no_mangle] extern "C" fns

def main() -> int:
    print(rs_add(2, 40))      # 42
    return 0
```

```rust
// native.rs
#[no_mangle]
pub extern "C" fn rs_add(a: i64, b: i64) -> i64 {
    a + b
}
```

rustc is an **optional** external toolchain, discovered like clang. `check`,
`c` and `symbols` never invoke it, because the binding scan is pure source
analysis; only `build` and `run` need Rust installed.

### Installation, platforms and commands

Windows is first-class, Linux is supported, macOS is unsupported. The
[platform policy](docs/platform-support.md) is authoritative. Windows ships
`Aoxn-<version>-Setup.exe` from the [release page](https://github.com/Ryan-178/Aoxn-Programme-Language/releases).
It unpacks into `%LOCALAPPDATA%\aoxn`, updates PATH and runs `aoxn doctor`.
**Nothing is downloaded by default**; `-InstallClang` explicitly opts in.
clang is external; Windows linking needs MSVC-compatible build tools. Rust is
an external prerequisite only for `.rs` imports; `aoxn doctor` lists rustc as
an optional row and never fails on its absence. Unsigned binaries can be
quarantined by OS policy; see [installation](docs/install.md).

Build the compiler with Rust. Compile native Aoxn programs with clang as well.
Linux GUI programs need X11/Xft/fontconfig development libraries.

```powershell
cargo build --release
cargo run -- run examples/hello.ax
cargo run -- check examples/fib.ax
cargo run -- c examples/fib.ax
cargo run -- build examples/fib.ax -o fib.exe
cargo run -- symbols examples/ui_gallery.ax --json
cargo run -- doctor --json
cargo run -- version
```

On Linux use an output such as `-o fib`. `check`, `c` and `symbols` do not invoke
clang or rustc. `--json` provides machine-readable diagnostics. `--O0` through
`--O3` select clang optimization (default O3), not different C text.
`--clang-arg`, `-g` and `--cc-warnings` expose clang options, debug information
and warnings.

`run`/`build` share a content-keyed cache. `AOXN_NO_CACHE=1` disables it;
`AOXN_CACHE_DIR` relocates it. `AOXN_CLANG` selects clang and `AOXN_RUSTC`
selects rustc; `AOXN_HOME` and `AOXN_STDLIB` select installed
toolchain/library paths. `AOXN_DUMP_C=1` prints C, and `AOXN_TIME=1` reports
lex/parse/resolve/typecheck/codegen/link phases.

### Language, ABI and self-hosting

- Strict types, explicit int/float conversion, bool-only conditions, checked
  returns and unreachable statements. Defaults, generics, conditional expressions,
  multi-value print, namespaces and selective/star imports are available.
- Structs and fixed arrays copy by value. Strings are immutable NUL-terminated
  bytes; string bounds clamp, while fixed-array indexing is unchecked.
- Dicts share heap handles and preserve insertion order with linear lookup.
  Nullable narrowing, function pointers and string-payload exceptions are supported
  by the Rust compiler. Concatenation intentionally leaks; there is no GC.
- **`sizeof(T)` / `addr_of(x)` (unreleased)** extend the raw-memory escape hatch:
  compile-time type sizes and live lvalue addresses. They perform no lifetime
  checks — an address is valid only while its owner lives, and a struct
  parameter's address is the callee's own copy (value semantics unchanged).
- C FFI and raw-memory builtins require correct caller contracts. Aoxn `int`
  maps to `long long`, not arbitrary C `int`; bool maps to C `int`, not a promise
  of `_Bool` compatibility. Externs are fixed-arity, not variadic. Internal
  aggregate ABI is not arbitrary C struct-by-value extern compatibility.
- The Rust bridge accepts exact ABI pairs only: `i64` to `int`, `f64` to
  `float`, `*const`/`*mut c_char` to `string`, and no return to `void`.
  Everything else — Rust `bool` (one byte against Aoxn's C `int`), `i32`,
  `u64`, `String`, `&str`, `*const i64`, generics, `where` clauses, `async`,
  variadics, tuple parameter patterns, `#[export_name]`, raw identifiers,
  C-keyword or `main` symbol names and non-`"C"` ABIs — is a diagnostic, not
  a guess. An `extern "C" fn` without `#[no_mangle]` is rejected. Only
  top-level exports bind, and the crate must be a single file (`mod name;` and
  `include!` are rejected). The bindings are synthesized in the loader, so no
  codegen dispatch exists and there is no self-host mirror obligation.
- The TypeScript front end is a supported subset, not full TS/JavaScript.
  Closures, objects and GC remain gaps; see the [TS plan](docs/ts-m1-spec.md).

The self-hosted compiler covers the v0.40.0 surface — `None`/`T | None`,
`dict[V]`, function pointers and `raise`/`try`/`except` — with C that matches
the Rust emitter byte-for-byte on the tested common subset. Programs without
those features keep their previous C. **`sizeof(T)` and `addr_of(x)` are
Rust-compiler-only: stage-2 rejects both forms** (no type-argument call form,
no `addr_of` builtin), and programs using neither emit byte-identical C, so
the fixed point needs no stage-2 mirror. The stage fixed point compares
self-hosted driver products across stages (masking Windows COFF timestamps) and
remains independent of the direct Rust-vs-selfhost emitter comparison; neither
finite corpus proves all-feature equivalence.

`selfhost/cli.ax` is a real Aoxn implementation of help, version, check, c,
build, run, symbols and doctor. `run` passes an exact argv vector to the child
(Windows CRT spawn, POSIX fork/exec — no shell), preserves empty/spaced/special
arguments and returns the child exit code. `-o`, `--O0..--O3`, `-l/-L`, `--cpu`,
`--backend c`, `--clang-arg`, `-g`, `--cc-warnings` and `--json` reach
codegen/link; JSON reports (success and error) go to stderr; W001, a
content-keyed run cache (`AOXN_NO_CACHE`/`AOXN_CACHE_DIR`) and
`AOXN_STDLIB`/`AOXN_HOME` resolution are implemented.

Still outside the selfhost CLI, and rejected rather than forwarded: package
commands, CSS assets/`--emit-assets`/`--tailwind`, namespace/selective
imports, and `sizeof`/`addr_of`. It also does not resolve `.rs` imports,
exactly as it does not resolve `.css` assets. Build caching, full doctor rows,
imported-symbol file attribution and exact diagnostic spans are incomplete.
The Rust CLI remains the production entry point. Read
[C compatibility](docs/c-compatibility.md), the [spec](docs/spec.md),
[language card](AGENTS.md) and [gaps](docs/language-gaps.md).

### Libraries and tools

Explicitly imported [stdlib modules](docs/stdlib.md) now cover the whole
roadmap: math, time, datetime, calendar, pathlib, base64, hashlib, hmac, os,
glob, JSON, bisect, heapq, CSV, random, socket, re, **shutil, collections,
itertools, net (urllib), threading, futures, sys, argparse, logging and
unittest**. Core containers/I/O remain in `stdlib/stdlib.ax`, which imports
nothing — every module is its own file, keeping the self-host critical path
untouched. Each new module ships an `examples/<name>_demo.ax`, a
`tests/<name>.rs` driver suite and a `docs/stdlib.md` section.

The closing batch in one paragraph each: **shutil** does real filesystem
copy/move/remove over `os`/`pathlib`, never follows reparse points
(symlinks/junctions) and caps recursion at depth 64. **collections** provides
a ring-buffer `Deque` (write-back pops), `Counter` over `dict[int]` with
stable `most_common`, defaultdict helpers and OrderedDict move-to-end.
**itertools** is eagerly batched (no generators exist): chain/repeat/range/
take/count/cycle/accumulate/zip/product/permutations/combinations, order
differential-checked against CPython. **net** is a pure URL layer
(`url_parse`/`url_build`, quote/unquote, query parse/encode; invalid input
answers a clean zero `Url`) differential-tested against `urllib.parse`, plus
an HTTP facade (`http_get`/`http_post`/`http_request`) over the WinHTTP
transport — a plain GET used to fail because WinHTTP rejects an empty header
block (error 87); both request paths now skip that call when no extra headers
are given. **threading** spawns real OS threads (`fn(int) -> int` workers,
state via heap addresses — no closures), write-back join, mutexes;
**futures** adds `fut_submit`/`fut_wait`/`fut_result`/`fut_parallel` with a
64-worker cap and inline overflow. Both compile and run under the selfhost
CLI (pinned in tests). **sys** exposes argv/platform/executable/exit/maxsize
over the v0.42.0 builtins. **argparse** is declarative options/flags/
positionals with `--help` and value errors (no subcommands). **logging** has
Python's 10–50 levels, stdout and append-mode file channels, no handler tree
or rotation. **unittest** is function-based (raise-based asserts caught per
test); there is still no `aoxn test` runner — repo-level testing stays
`cargo test`.

Earlier modules keep their documented boundaries: CSV is RFC 4180 with value
errors, not the full Python dialect; random is deterministic MINSTD and
explicitly NOT cryptographically secure; socket is blocking IPv4 TCP (no
DNS/UDP/IPv6/TLS) with Windows `-l ws2_32`, and its Linux branch is
implemented but not natively executed in this round. `re` is a backtracking
matcher for a Python `re` subset, differential-tested against CPython;
unsupported syntax (backreferences, lookaround, inline flags, hex/unicode
escapes) is a named compile error, never a silent degradation, and the module
compiles under the self-hosted compiler. The
[Rust bridge](docs/rust-interop.md) is the second foreign-code
path beside clang FFI: a `.rs` import becomes synthesized extern declarations
and a rustc-built static library, with rustc required only for `build`/`run`.
The [UI](docs/ui.md) shares widgets across Win32/GDI and Linux X11/Xft. The
[OpenAI](docs/openai-sdk.md) and [Anthropic](docs/anthropic-sdk.md) SDKs share
JSON/HTTP/SSE; WinHTTP transport is Windows-only.

The [package manager](docs/pkg-manager.md) uses a pyproject-shaped `aoxn.toml`
(`[project]` + `[project.optional-dependencies]` + `[tool.aoxn]`), `aoxn.lock`
and `aox_modules/`; legacy `aoxn.json` projects are still read and convert with
`aoxn migrate`. `aoxn install <pkg>` declares then installs, `aoxn install`
syncs, and `uninstall/list/freeze/tree/why/outdated/audit/trust/publish/yank/
cache/npm-import` follow the pip vocabulary; `add`/`remove` remain aliases.
Registries are directories, git or read-only plain HTTP. npm-import requires
Aoxn metadata, not arbitrary JS. Warm reuse rechecks content hashes, excluding
only its own `index.ax` shim. [CSS assets](docs/css-assets.md) include CSS Modules,
minification, URL handling and `--emit-assets`; built-in `--tailwind` is a subset.
The [IDE](docs/ide.md) is a Tauri/Next.js/Monaco application with a
ZCode-derived Zai design system, assistant, terminal, source-control and
preview panes; separately tested.

### Repository maintenance and CI scope

Main CI explicitly runs `cargo test -p aoxn --locked` and
`cargo test -p aoxn-pkg --locked` on Windows/Linux: the root non-virtual workspace
otherwise defaults to only `aoxn`. A separate named step reruns
`--test c_emitter_parity --test c_abi` with `AOXN_TEST_REQUIRE_CLANG=1`; missing
clang fails that gate instead of silently skipping the comparisons. Both
Windows and Linux jobs also run a named `Test Rust bridge` step,
`cargo test -p aoxn --locked --test rust_interop`, with
`AOXN_TEST_REQUIRE_CLANG=1` and `AOXN_TEST_REQUIRE_RUSTC=1`, so a missing rustc
fails rather than skips. Linux also runs the X11 probe under Xvfb. Release
packaging/install smoke runs on tags or manual dispatch, not every push;
web benchmarks and IDE checks are separate. The web-bench workflow's bash steps
use forward-slash paths: Git Bash treats a backslash as an escape, which had
turned `web\server_win.ax` into `webserver_win.ax` and failed the build step.
Windows smoke commands now check native exit codes immediately. Release notes
match the installer's opt-in clang download behavior. The GitHub `main`
branch is protected: changes land through a pull request (required
approvals: 0 — a solo maintainer cannot review their own PR); direct
pushes and force pushes are blocked, so session work is pushed as a
branch and merged via the PR.

Current specifications and contributor instructions have been reconciled with
the C backend; LLVM-era reports retain historical data with explicit boundaries.
`wiki/` stays frozen. The [repository audit](docs/repository-audit.md) records
artifact provenance without deleting unexplained files. Run the dependency-free
read-only checks with Python:

```text
python tools/test_check_doc_links.py
python tools/check_doc_links.py
```

The path auditor is warning-only by default; `--strict` treats all findings as
failures. Command hints are heuristic, not confirmed bugs. It does not fetch
links, execute documented commands, validate anchors or replace a security audit.

### Validation and evidence

```powershell
$env:AOXN_CLANG = "E:/llvm/bin/clang.exe"  # choose an existing clang
$env:AOXN_TEST_REQUIRE_CLANG = "1"
$env:AOXN_TEST_REQUIRE_RUSTC = "1"
cargo test -p aoxn --locked --no-fail-fast
cargo test -p aoxn --locked --test c_emitter_parity --test c_abi
cargo test -p aoxn --locked --test rust_interop
cargo test -p aoxn-pkg --locked
cargo check --workspace --tests --target x86_64-unknown-linux-gnu
```

Final Windows validation for the current tree (2026-11-14, clang 23.1.2 at
`E:\llvm\bin\clang.exe` on this machine, rustc from rustup): **399 root +
116 package = 515 passed, 0 failed, 0 ignored**. The root suite grew from
385 to 399 with the closing stdlib batch's 14 tests across nine files:
`sys` (1), `argparse` (1), `logging` (1), `unittest` (2), `collections` (1),
`itertools` (2, including the CPython order differential), `shutil` (1, real
temp trees), `net` (3: URL-layer codecs, the `urllib.parse` differential and
the HTTP facade against a local mock server with a bounded wait) and
`threading` (2: four-thread mutex contention plus futures, and a selfhost-CLI
pin that builds `selfhost/cli.ax` and runs a threads program through it).
The full run includes the fixed point, both parity targets, the four ABI
tests (O0/O3), both CLI stages, the Rust bridge (11 tests), real socket
loopback, random cross-process vectors, the Python `csv` differential, the
`re` CPython differential and selfhost pin, and the eight
`sizeof_*`/`addr_of_*` pins.
`cargo check --workspace --tests --target x86_64-unknown-linux-gnu` passed;
Linux native linking/runtime was not executed. The package suite ran
through `run_pkg_tests.sh` (the Smart App Control hash-stamp workaround).
The read-only doc audit (2026-11-14) reported `missing-path=0`,
`historical-path=0` and 25 heuristic command-drift warnings; its own tests
passed 11/11. Full installer packaging was not run; IDE suites were not
re-run (no IDE source changed).

**Environment note (corrected 2026-11-14)**: on this dev machine clang 23.1.2
lives at `E:\llvm\bin\clang.exe`; `C:\Program Files\LLVM` does NOT exist. An
earlier note (2026-11-10) claimed the opposite and was wrong. The general
lesson stands: a stale `AOXN_CLANG` pointing at a dead path is a split
failure — the Rust resolver falls back and Rust-side tests pass, but the
selfhost driver uses the env value verbatim and all five driver tests fail
with `_spawnvp` exit 127. That signature means "the AOXN_CLANG path is dead",
not a codegen bug.

**Dependency security, 2026-11-01**: the two JS manifests were re-audited and
upgraded — `ide` to `next` 16.4.3 plus dompurify 3.4.17, `web/next-app` to
`next` 15.5.28 — closing seven advisories; `pnpm audit` now reports no known
vulnerabilities in either, with 62/62 IDE frontend tests, `pnpm typecheck`, both
`pnpm build`s and a `next start` route smoke passing. The one remaining Rust
advisory (glib, `RUSTSEC-2024-0429`) stays accepted in `ide/src-tauri/audit.toml`
with a dated re-check. No compiler suite re-ran for it: nothing in `src/`,
`tests/`, the Aoxn sources or any `Cargo.lock` changed.

**IDE rework, 2026-11-07**: the IDE moved to a ZCode v3.17.2-derived Zai
design system and gained assistant, terminal, source-control and preview
panes. Validation: `pnpm --dir ide typecheck`, **64/64 IDE frontend tests**,
`pnpm build`, **43 IDE Rust tests** and `pnpm audit` (no known
vulnerabilities) all pass; the git porcelain parser and preview MIME map
have dedicated Rust tests. No compiler suite re-ran — no compiler source
changed.

**Linux CI, 2026-11-01**: one `aoxn-pkg` test was Windows-only in its expectation.
The archive entry `demo/C:/Users/victim/…` is an absolute path on Windows and an
ordinary directory named `C:` on POSIX, where extraction keeps it inside the
destination — so the guard accepted what the test demanded it refuse. The test now
derives its absolute spelling from `temp_dir()` and both hosts assert one rule; a
second test pins the divergence (refused here, contained there, never escaping).
The guard's own code is unchanged. Verified: **105/105 `aoxn-pkg` lib tests** on
Windows, `cargo check --workspace --tests --target x86_64-unknown-linux-gnu`, and
the hosted CI run on `main` — **Windows and Linux both green**, where the Linux
`Test package manager` step no longer aborts, so `Smoke test` and the X11 probe
under Xvfb ran again for the first time since the streak turned red. The Linux
test binary was not executed on this machine (no WSL or Docker); CI supplied that.
Details: `CHANGELOG.md`.

Historical baseline **8f90aca** recorded 343 root + 104 package tests (447), with
explicit clang 23.1.2 on Windows. This is a dated baseline, not a live count of
the modified tree. The [performance report](docs/p2-compiler-performance.md)
records exact comparisons and failures: the 1600-struct synthetic chain improved
from 602.866 to 2.398 ms typecheck median, but real workloads do not establish a
general end-to-end speedup. Wrapped/array indirect struct cycles remain a known
checker limitation.

| Location | Purpose |
|---|---|
| `src/`, `tests/` | compiler, CLI, installer, C ABI/parity and Rust bridge suites |
| `selfhost/` | self-hosted compiler subset |
| `stdlib/`, `examples/` | libraries and examples |
| `crates/aoxn-pkg/` | independent package manager |
| `ide/`, `web/` | IDE and Windows web benchmarks |
| `dist/package.ps1` | Windows installer packaging |
| `tools/compiler_perf.py` | clang-free interleaved measurements |
| `docs/` | maintained specifications and evidence |
| `wiki/` | frozen historical documentation |

Contribute under [AGENTS.md](AGENTS.md); report vulnerabilities privately under
[SECURITY.md](SECURITY.md). License: [Apache-2.0](LICENSE).

---

## 中文

### 项目与当前范围

Aoxn 是使用 Python 风格缩进的静态类型原生语言。Rust 编译器发射 ISO C99，
再调用外部 clang；`src/` 编译器库没有外部 Rust crate 或 LLVM 库依赖。
独立包管理器的依赖固定在 `Cargo.lock`。

**编译器 0.55.0，包管理器 crate 0.43.0**。当前树带有**未发布**的语言与
库工作（CHANGELOG `[Unreleased]`，未经用户明确要求不发布新版本）：

- **`sizeof(T)` 与 `addr_of(x)`**。`sizeof` 接收类型实参，编译期折叠为 C
  表示的字节大小（泛型体内随实例化代入）；`addr_of` 返回 int/float/bool/
  结构体/数组左值的活地址（普通 `int`），经 `store_i64`/`store_f64` 的写入
  对所有者可见。这解除了所有非 8 字节槽容器的语言层阻塞（缺口 A2）：
  结构体容器现在可以用 `sizeof` + `addr_of` + `memcpy` 在 Aoxn 里写出来，
  但 stdlib 的 `Vec` 本身仍是槽位实现。临时量、字符串、dict、函数指针以及
  任何以 nullable 局部为根的左值（即使已收窄）都被拒绝并给出明确诊断。
- **`stdlib/re.ax`**：回溯式正则匹配器，覆盖 Python `re` 子集（分组、
  字符类、懒惰量词、锚点、单词边界；match/search/fullmatch/findall/sub/
  split），与 CPython 差分测试一致，并可被自举编译器编译。
- **标准库路线图收官**。本批再落地 10 个模块——`shutil`、`collections`、
  `itertools`、`net`（urllib 风格 URL 层 + HTTP 门面）、`threading`、
  `futures`（线程子集）、`sys`、`argparse`、`logging`、`unittest`——
  至此 **`docs/stdlib-todo.md` §2 的 31 项 Python 标准库路线图全部处置完毕：
  27 项已交付，4 项为 §4 长期不做决定并给出替代**（pickle → JSON、
  multiprocessing → socket/subprocess、traceback → 编译器诊断、
  typing → 静态类型系统本身）。§7 完成度审计把每一项映射到模块、测试与
  示例。§5 的 15 个第三方库仍明确不在标准库范围内。

已发布的 0.55.0 线交付了 pyproject 形状的 `aoxn.toml` 清单与 pip 风格依赖
CLI（snapshot 6），以及 **Rust 桥**（snapshot 5）：`import "./native.rs"`
把自包含 Rust 文件用 rustc 编译为静态库，并把其中 `#[no_mangle] extern "C"`
函数绑定为普通 Aoxn `extern def` 声明。这些都不引入 C 源码兼容，见
[C 兼容边界](docs/c-compatibility.md)。

```python
import * from "stdlib"

struct Point:
    x: int
    y: int

def main() -> int:
    print(sizeof(int), sizeof(Point), sizeof([int; 4]))   # 8 16 32
    p = Point(x=3, y=4)
    a = addr_of(p.x)          # 字段的活地址
    store_i64(a, 30)
    print(p.x)                # 30
    return 0
```

```python
import * from "./native.rs"   # 绑定 native.rs 的 #[no_mangle] extern "C" 函数

def main() -> int:
    print(rs_add(2, 40))      # 42
    return 0
```

```rust
// native.rs
#[no_mangle]
pub extern "C" fn rs_add(a: i64, b: i64) -> i64 {
    a + b
}
```

rustc 是**可选**外部工具链，发现顺序与 clang 相同。`check`、`c`、`symbols`
从不调用它——绑定扫描是纯源码分析；只有 `build` 与 `run` 需要安装 Rust。

### 平台、安装和命令

Windows 为一等平台，支持 Linux，不支持 macOS，以
[平台政策](docs/platform-support.md) 为准。[Windows 发布包](https://github.com/Ryan-178/Aoxn-Programme-Language/releases)
为 `Aoxn-<version>-Setup.exe`，解包至 `%LOCALAPPDATA%\aoxn`、设置 PATH、
运行 doctor。**默认不下载**；`-InstallClang` 显式启用下载。
clang 为外部前提，Windows 链接需要 MSVC 兼容工具；Rust 仅在 `.rs` 导入时需要，
`aoxn doctor` 把 rustc 列为可选行，缺失也不判失败。未签名程序可能被系统隔离。
见[安装指南](docs/install.md)。

构建编译器需要 Rust，编译 Aoxn 程序还需 clang；Linux 图形程序另需
X11/Xft/fontconfig 开发库。

```powershell
cargo build --release
cargo run -- run examples/hello.ax
cargo run -- check examples/fib.ax
cargo run -- c examples/fib.ax
cargo run -- build examples/fib.ax -o fib.exe
cargo run -- symbols examples/ui_gallery.ax --json
cargo run -- doctor --json
cargo run -- version
```

Linux 输出可写 `-o fib`。check/c/symbols 既不调用 clang 也不调用 rustc；
`--json` 提供结构化诊断。`--O0` 至 `--O3` 仅选择 clang 优化级别（默认 O3），
不改变 C 文本。`--clang-arg`、`-g`、`--cc-warnings` 用于透传参数、调试信息和警告。
run/build 共享内容缓存；AOXN_NO_CACHE 禁用，AOXN_CACHE_DIR 改位置。
AOXN_CLANG 指定 clang，AOXN_RUSTC 指定 rustc，AOXN_HOME/AOXN_STDLIB 指定安装位置；
AOXN_DUMP_C 打印 C，AOXN_TIME 输出各编译阶段耗时。

### 语言、ABI 与自举

- 类型严格，int/float 显式转换，条件必须 bool，检查返回路径和不可达代码。
  支持默认参数、泛型、条件表达式、多参数 print、命名空间和选择性/星号导入。
- 结构体与定长数组按值复制；字符串是不可变 NUL 结束字节，字符串越界 clamp，
  数组索引不检查。dict 为保序共享堆句柄，查询线性。
- Rust 编译器支持 nullable 收窄、函数指针与异常；拼接字符串故意不释放，没有 GC。
- **`sizeof(T)` / `addr_of(x)`（未发布）**扩展原始内存逃生通道：编译期类型
  大小与左值活地址。不做生命周期检查——地址只在所有者存活期间有效，结构体
  参数的地址是被调方自己的副本（值语义不变）。
- C FFI/原始内存依赖调用方正确约定。int 对应 long long 而不是任意 C int；
  bool 对应 C int，并不保证兼容 _Bool。extern 固定参数，不支持可变参数；
  内部聚合 ABI 不等于任意 C struct-by-value extern 兼容。
- Rust 桥只接受精确 ABI 配对：`i64` 对应 `int`，`f64` 对应 `float`，
  `*const`/`*mut c_char` 对应 `string`，无返回值对应 `void`。其余一律是
  诊断而非猜测——Rust `bool`（1 字节，而 Aoxn `bool` 是 C `int`）、`i32`、
  `u64`、`String`、`&str`、`*const i64`、泛型、`where` 子句、`async`、可变参数、
  元组参数模式、`#[export_name]`、原始标识符、C 关键字或 `main` 符号名，以及
  非 `"C"` ABI 都被拒绝；没有 `#[no_mangle]` 的 `extern "C" fn` 也被拒绝。
  只绑定顶层导出，crate 必须是单文件（`mod name;` 与 `include!` 被拒绝）。
  绑定由加载器合成，因此没有 codegen 分支，也没有自举镜像义务。
- TS 前端为子集而非完整 TS/JS；对象、闭包、GC 仍有缺口，见[计划](docs/ts-m1-spec.md)。

自举覆盖 v0.40.0 语言面：None/`T | None`、`dict[V]`、函数指针与
raise/try/except；在已测试的共同子集上 C 与 Rust 发射器逐字节相同，不使用这些
特性的程序 C 不变。**`sizeof(T)` 与 `addr_of(x)` 仅 Rust 编译器支持：stage-2
拒绝这两种形式**（没有类型实参调用形式，没有 `addr_of` 内建），不用它们的
程序 C 逐字节不变，因此不动点无需 stage-2 镜像。跨阶段不动点与
Rust-vs-selfhost 直接比较仍是两项独立证据，有限语料都不证明全功能等价。

selfhost/cli.ax 是真正的 Aoxn 实现，提供 help/version/check/c/build/run/
symbols/doctor。run 以精确 argv 向量启动子进程（Windows CRT spawn、POSIX
fork/exec，无 shell），保留空串/空格/特殊字符并返回子进程退出码。`-o`、
`--O0..--O3`、`-l/-L`、`--cpu`、`--backend c`、`--clang-arg`、`-g`、
`--cc-warnings`、`--json` 进入真实编译/链接；JSON 报告（成功/错误）走 stderr；
W001、内容键 run 缓存（`AOXN_NO_CACHE`/`AOXN_CACHE_DIR`）与
`AOXN_STDLIB`/`AOXN_HOME` 解析已实现。

仍不在自举 CLI 内、且明确拒绝而非转发：包管理器命令、CSS 资源/
`--emit-assets`/`--tailwind`、命名空间/选择性导入，以及 sizeof/addr_of。
它同样不解析 `.rs` 导入，就像不解析 `.css` 资源。build 缓存、完整 doctor 行、
导入符号文件归属和精确诊断跨度仍不完整。生产 CLI 仍是 Rust。见
[C 边界](docs/c-compatibility.md)、[规范](docs/spec.md)、
[语言卡](AGENTS.md)和[语言缺口](docs/language-gaps.md)。

### 库与工具

[标准库](docs/stdlib.md)独立模块现已覆盖整个路线图：math/time/datetime/
calendar/pathlib/base64/hashlib/hmac/os/glob/json/bisect/heapq/csv/random/
socket/re，以及本批的 **shutil/collections/itertools/net（urllib）/threading/
futures/sys/argparse/logging/unittest**。核心容器与 I/O 仍在
stdlib/stdlib.ax——它零导入，每个模块都是独立文件，自举关键路径不受影响。
每个新模块都带 `examples/<name>_demo.ax`、`tests/<name>.rs` 驱动套件和
`docs/stdlib.md` 章节。

收官批次各一段：**shutil** 基于 os/pathlib 做真实文件复制/移动/删除，
绝不跟随重解析点（符号链接/junction），递归深度上限 64。**collections**
提供环形缓冲 Deque（写回式 pop）、基于 `dict[int]` 的 Counter（稳定
most_common）、defaultdict 助手与 OrderedDict move-to-end。**itertools**
全部为急切批量形式（语言没有生成器）：chain/repeat/range/take/count/cycle/
accumulate/zip/product/permutations/combinations，顺序与 CPython 差分一致。
**net** 是纯 URL 层（url_parse/url_build、quote/unquote、查询解析/编码；
非法输入返回干净的全零 Url），与 `urllib.parse` 差分测试一致，另加基于
WinHTTP 传输的 HTTP 门面（http_get/http_post/http_request）——WinHTTP 拒绝
空头块（错误 87）曾导致普通 GET 全部失败，现在两条请求路径在无额外头时
跳过该调用。**threading** 创建真实 OS 线程（`fn(int) -> int` 工作函数，
状态经堆地址传递——没有闭包），写回式 join 与互斥量；**futures** 增加
fut_submit/fut_wait/fut_result/fut_parallel，64 工作者上限、超出部分内联
执行。两者都可被自举 CLI 编译运行（测试钉住）。**sys** 基于 v0.42.0 内建
提供 argv/platform/executable/exit/maxsize。**argparse** 为声明式选项/
开关/位置参数，带 --help 与值错误（无子命令）。**logging** 采用 Python
10–50 级别、stdout 与追加式文件通道，无处理器树/轮转。**unittest** 为
函数式（基于 raise 的断言按测试捕获）；仍没有 `aoxn test` 运行器——
仓库级测试仍是 `cargo test`。

早前模块保持其 documented 边界：CSV 是 RFC 4180 + 值错误，不等于 Python
全部 dialect；random 是确定性 MINSTD，**明确非密码学安全**；socket 是
blocking IPv4 TCP（无 DNS/UDP/IPv6/TLS），Windows 需 `-l ws2_32`，Linux
分支已实现但本轮未原生运行。re 是 Python `re` 子集的回溯匹配器，与
CPython 差分一致；不支持的语法（反向引用、环视、内联标志、十六进制/
Unicode 转义）报具名编译错误而非静默降级，且模块可在自举编译器下编译。
[Rust 桥](docs/rust-interop.md)是 clang FFI 之外的第二条外部代码路径：
`.rs` 导入变成合成的 extern 声明加 rustc 构建的静态库，只有 build/run 需要 rustc。
[UI](docs/ui.md)共享控件层，后端为 Win32/GDI 与 X11/Xft。
[OpenAI](docs/openai-sdk.md)/[Anthropic](docs/anthropic-sdk.md) SDK 共享传输基础，
WinHTTP 为 Windows-only。

[包管理器](docs/pkg-manager.md)使用 pyproject 形状的 `aoxn.toml`
（`[project]` + `[project.optional-dependencies]` + `[tool.aoxn]`）、`aoxn.lock`
与 `aox_modules/`；旧 `aoxn.json` 项目仍可读，并用 `aoxn migrate` 转换。
`aoxn install <pkg>` 先声明再安装，`aoxn install` 按清单同步，其余采用 pip
词汇（uninstall/list/freeze/tree/why/outdated/audit/trust/publish/yank/cache/
npm-import），add/remove 保留为别名；注册表为目录/git/只读明文 HTTP。
npm-import 只收 Aoxn 元数据，不支持任意 JS。热安装复用前重算内容哈希，
仅排除自身 index.ax shim。
[CSS](docs/css-assets.md)支持 Modules、压缩、URL、emit-assets，内建 Tailwind 是子集。
[IDE](docs/ide.md)为独立测试的 Tauri/Next.js/Monaco 应用，界面采用
ZCode 衍生的 Zai 设计系统，并带助手、终端、Git 与预览面板。

### 仓库维护与 CI 范围

主 CI 在 Windows/Linux 显式执行 `cargo test -p aoxn --locked` 和
`cargo test -p aoxn-pkg --locked`；非虚拟工作区裸 cargo test 默认只测试根包。
独立命名步骤以 `AOXN_TEST_REQUIRE_CLANG=1` 重跑
`--test c_emitter_parity --test c_abi`，缺少 clang 会判定门禁失败，不再静默跳过比较。
Windows 与 Linux 另有命名步骤 `Test Rust bridge`，执行
`cargo test -p aoxn --locked --test rust_interop`，并设
`AOXN_TEST_REQUIRE_CLANG=1` 与 `AOXN_TEST_REQUIRE_RUSTC=1`：缺 rustc 会失败而不是跳过。
Linux 另跑 Xvfb 下 X11 probe。安装打包与冒烟在 tag/手动触发，不是每次 push；
Web 基准和 IDE 独立。web-bench 工作流的 bash 步骤改用正斜杠路径：Git Bash 把
反斜杠当转义吞掉（`web\server_win.ax` 变成 `webserver_win.ax`），曾导致构建步骤
直接失败。Windows 冒烟逐条检查原生命令退出码，发布文案与安装器
显式启用 clang 下载的行为一致。GitHub 上 `main` 分支受保护：变更必须经
pull request 合入（批准数要求为 0——单人维护者无法评审自己的 PR），
禁止直推与强推；因此会话工作以分支推送，经 PR 合并。

规范/贡献指南对齐当前 C 后端；LLVM 历史报告保留数据并标清边界，wiki 不修改。
[仓库审计](docs/repository-audit.md)记录产物来源，未盲删历史文件。
只读检查命令无需外部 Python 包：

```text
python tools/test_check_doc_links.py
python tools/check_doc_links.py
```

检查器默认仅警告，--strict 对所有发现返回失败；命令提示只是启发式，不是确认缺陷。
不访问网络、不执行文档命令、不校验锚点，也不是安全审计。

### 验证与证据

```powershell
$env:AOXN_CLANG = "E:/llvm/bin/clang.exe"  # 替换为已有 clang
$env:AOXN_TEST_REQUIRE_CLANG = "1"
$env:AOXN_TEST_REQUIRE_RUSTC = "1"
cargo test -p aoxn --locked --no-fail-fast
cargo test -p aoxn --locked --test c_emitter_parity --test c_abi
cargo test -p aoxn --locked --test rust_interop
cargo test -p aoxn-pkg --locked
cargo check --workspace --tests --target x86_64-unknown-linux-gnu
```

当前树 Windows 最终验证（2026-11-14，本机 clang 23.1.2 位于
`E:\llvm\bin\clang.exe`，rustc 来自 rustup）：**399 根 + 116 包 = 515 项
通过，0 失败，0 忽略**。根套件从 385 增至 399，新增收官批次的 14 项测试，
分布在 9 个文件：sys（1）、argparse（1）、logging（1）、unittest（2）、
collections（1）、itertools（2，含 CPython 顺序差分）、shutil（1，真实临时
目录树）、net（3：URL 层编解码、`urllib.parse` 差分、对本地 mock 服务器的
HTTP 门面限时等待测试）、threading（2：四线程互斥竞争加 futures，以及
构建 selfhost/cli.ax 并经其运行线程程序的自举钉测试）。完整运行包含不动点、
两项 parity、4 项 ABI（O0/O3）、两阶段 CLI、Rust 桥（11 项）、真实 socket
回环、随机数跨进程向量、Python `csv` 差分、`re` 的 CPython 差分与自举钉、
8 项 sizeof/addr_of 钉测试。
`cargo check --workspace --tests --target x86_64-unknown-linux-gnu` 通过，
未执行 Linux 原生链接/运行。包套件经 `run_pkg_tests.sh` 运行（Smart App
Control 哈希戳 workaround）。只读文档审计（2026-11-14）报告
`missing-path=0`、`historical-path=0` 与 25 条启发式命令漂移警告，其自身
测试 11/11 通过。完整安装器打包未运行；IDE 套件未重跑（IDE 源码未改动）。

**环境说明（2026-11-14 更正）**：本开发机 clang 23.1.2 位于
`E:\llvm\bin\clang.exe`；`C:\Program Files\LLVM` 并不存在。早前
（2026-11-10）声称相反情况的说明有误。通用教训仍然成立：指向失效路径的
过期 `AOXN_CLANG` 会造成分裂失败——Rust 解析器回退、Rust 侧测试全过，
但自举 driver 原样使用该环境变量，五个 driver 测试全部以 `_spawnvp`
exit 127 失败。该签名意为「AOXN_CLANG 路径已失效」，不是 codegen 缺陷。

**依赖安全 2026-11-01**：两份 JS 清单重新审计并升级——`ide` 到 `next` 16.4.3
加 dompurify 3.4.17，`web/next-app` 到 `next` 15.5.28，共关闭 7 项公告；两份清单
`pnpm audit` 均已无已知漏洞，IDE 前端 62/62、`pnpm typecheck`、两侧 `pnpm build`
与 `next start` 路由冒烟全部通过。唯一保留的 Rust 公告（glib
`RUSTSEC-2024-0429`）连同本次复核仍记在 `ide/src-tauri/audit.toml`。
本次未重跑编译器测试：src、tests、Aoxn 源码与任何 Cargo.lock 都未改动。

**IDE 重构 2026-11-07**：IDE 迁移到 ZCode v3.17.2 衍生的 Zai 设计系统，
新增助手、终端、Git 与预览面板。验证：`pnpm --dir ide typecheck`、
**IDE 前端 64/64**、`pnpm build`、**IDE Rust 43 项**、`pnpm audit`
（无已知漏洞）全部通过；git porcelain 解析与预览 MIME 映射有独立 Rust 测试。
未重跑编译器套件——未改动编译器源码。

**Linux CI 2026-11-01**：`aoxn-pkg` 有一项测试的预期只适用于 Windows。归档条目
`demo/C:/Users/victim/…` 在 Windows 是绝对路径，在 POSIX 只是名叫 `C:` 的普通目录，
解包会留在目标目录内——于是守卫放行的正是测试要求拒绝的。该测试改为从
`temp_dir()` 导出绝对拼法，两个平台断言同一条规则；另一项测试钉住这次分歧（本机
拒绝、他机内含，但都不逃逸）。守卫代码本身一字未改。已验证：Windows
`aoxn-pkg` 库测试 **105/105**，`cargo check --workspace --tests --target
x86_64-unknown-linux-gnu`，以及 main 上的托管 CI——**Windows 与 Linux 双双通过**；
Linux 的 `Test package manager` 不再中断，`Smoke test` 与 Xvfb 下的 X11 探测因此
重新执行并通过。本机无 WSL 与 Docker，Linux 测试二进制未本地执行，由 CI 补齐。
详见 `CHANGELOG.md`。

历史基线 **8f90aca** 在 Windows 显式 clang 23.1.2 下记录 343 根+104 包=447 项，
不是修改后实时计数。[性能报告](docs/p2-compiler-performance.md)保留具体实验与失败：
1600 结构体合成链 typecheck 中位数 602.866→2.398ms，不能推为所有项目端到端加速。
数组等包装类型间接结构体环仍为已知检查器限制。

目录：src/tests 为编译器与回归（C ABI/parity 及 Rust 桥套件），selfhost 为自举子集，
stdlib/examples 为库/示例，crates/aoxn-pkg 为包管理，ide/web 为 IDE/Windows Web
基准；dist/package.ps1 打包，tools/compiler_perf.py 测量编译耗时；
docs 为维护文档，**wiki 冻结且不作为当前权威**。

贡献遵守 [AGENTS.md](AGENTS.md)，漏洞私报见 [SECURITY.md](SECURITY.md)。
许可证 [Apache-2.0](LICENSE)。