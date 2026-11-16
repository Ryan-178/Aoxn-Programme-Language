# Platform support

## Linux native adaptation (released as v0.55.0-snapshot-5)

The compiler remains 0.55.0; this batch ships as pre-release tag
`v0.55.0-snapshot-5` (2026-11-16). Linux compiler/portable-stdlib/X11 support
dates to 0.46.0; this batch adds the native HTTP provider and source installer
and fixes socket/doctor behavior. **It was natively validated on 2026-11-16**:
PR #11 checks are green on Ubuntu 22.04/24.04 x86_64, Ubuntu 24.04 ARM64, macOS
and Windows. That run exposed a Rust-bridge staging race, fixed in df4c3dd and
pinned by a six-thread regression. Alpine/musl, other distributions and native
Wayland remain unverified.

Linux prerequisites: Rust/cargo for building the compiler, clang and system C
development tools for linking. HTTP additionally needs rustc, libcurl development
files and CA certificates (Debian/Ubuntu: `sudo apt-get install clang build-essential
libcurl4-openssl-dev ca-certificates`). X11 GUI builds need libx11-dev, libxft-dev,
libfontconfig1-dev and a display (Xvfb for headless probes).

From the checkout: `bash dist/install-linux.sh --prefix "$HOME/.local/aoxn"`,
then `export PATH="$HOME/.local/aoxn/bin:$PATH"` and `aoxn doctor`.
The prefix must be absolute and dedicated; --skip-build reuses target/release/aoxn.
Profiles are untouched by default; --modify-path opts into a safely quoted PATH
entry in ~/.profile. The installer copies the complete stdlib tree and fails if
doctor fails. No toolchains are downloaded; cargo may fetch locked dependencies.

Linux HTTP applications must add `import * from "stdlib/net/http_linux"`
alongside stdlib/net, stdlib/net/http or either SDK, then use `aoxn run app.ax`
without -l winhttp. http.ax dispatches to net_curl_* helpers and the shared
http_macos_native.rs now compiles its libcurl implementation on Linux and macOS.
rustc propagates -lcurl. Omitting the provider can cause unresolved aoxn_http_*
link symbols, not a guaranteed runtime error. Windows WinHTTP is unchanged;
macOS keeps stdlib/net/http_macos. The selfhost CLI cannot load these .rs providers.
Linux socket sends use MSG_NOSIGNAL; descriptor zero is accepted for close.
Doctor requires portable module sources plus the native backend/provider files;
file completeness and its smoke test do not validate all HTTP/TLS/GUI behavior.
The new CI matrix targets Ubuntu 22.04/24.04 x86_64 and Ubuntu 24.04 ARM64,
including SDK retries/SSE, socket failures, installer contracts and installed
HTTP/X11 probes. Earlier native Linux/macOS runs failed because concurrent
Rust bridge builds shared a PID-only staging directory. Commit df4c3dd isolates
those directories; the rerun on 0fc443d is green (2026-11-16), including the
SDK, HTTPS-rejection, ABI and installed HTTP/X11 gates. Alpine/musl and other
distributions remain unverified; a green Ubuntu job does not prove them. Local
Apple Silicon/Intel workspace/test cross-checks pass but are not native execution.

中文：本批 Linux 原生适配仍在进行中且未发布，版本保持 0.55.0；新增改动尚待原生
Linux 验证，Windows/交叉检查不替代原生运行。安装需 Rust/cargo、clang、C 开发环境，
HTTP 另需 rustc、libcurl 开发文件和 CA；应用显式导入 stdlib/net/http_linux，不传
-l winhttp。安装命令及前缀见上，默认不改 profile，--modify-path 才追加 ~/.profile；
完整复制标准库，doctor 失败即安装失败，不下载工具链，但 cargo 可获取锁定依赖。
macOS 2026-11-15 成功记录是历史证据，不能验证本批共享桥变更。

## macOS core port (native evidence: 2026-11-15; included in snapshot 5)

The source tree adds macOS core compiler/CLI and portable-stdlib adaptations
for Apple Silicon and Intel. **macOS is natively validated on the
`macos-latest` CI job (Apple Silicon, 2026-11-15)**: the whole compiler/stdlib/
TS/UI suite, C parity/ABI, the Rust bridge, the package manager, a native Cocoa
window probe, a loopback libcurl HTTP probe and the from-source installer +
installed-toolchain smoke all build and run green on real macOS. This is
part of v0.55.0-snapshot-5; Intel macOS is cross-checked clean, and the
hosted runner is Apple Silicon, so Intel native execution is not separately
claimed. Build from source with Rust and Apple's Command
Line Tools (`xcode-select --install`); clang must resolve an installed SDK.
A build-from-source installer is provided (`dist/install-macos.sh`): it builds
the compiler with cargo, lays out `<prefix>/bin/aoxn` + the whole
`<prefix>/lib/stdlib` tree + examples, adds `<prefix>/bin` to PATH and runs
`aoxn doctor`, without installing toolchains (cargo may fetch dependencies). There is no single-file packaged macOS
artifact and no IDE packaging. A native Cocoa backend is available
through `stdlib/ui_cocoa`, using a single-file Rust AppKit/CoreGraphics bridge;
it requires rustc, and a native Cocoa window probe passes on the CI job. HTTP
transport on macOS rides
libcurl through an opt-in Rust bridge (`stdlib/net/http_macos` +
`http_macos_native.rs`); the WinHTTP implementation stays Windows-only. Linux has the
opt-in provider described above; the shared bridge passed the native Linux and
macOS jobs on 2026-11-16.

`target_os()` reports `"macos"`. Native objects are Mach-O `.o`, static
libraries `.a`, executables have no suffix, and libSystem supplies C math.
Darwin adaptations cover clock IDs, executable-path discovery, inode64
filesystem layouts/symbols, errno and IPv4 sockaddr layout. Darwin sockets
use SO_NOSIGPIPE on created and accepted connections. Both C emitters select
the Intel inode64 names before extern declarations; arm64 uses native names.
Local cross-checks are distinct from real Apple clang linking/runtime tests.
Local Windows regressions and Linux/Apple Silicon/Intel macOS Rust
workspace/tests cross-checks passed; the `macos-latest` CI job now supplies the
real Apple clang linking/runtime evidence on Apple Silicon — compiler, package,
C ABI/parity, Rust bridge and socket suites, plus the Darwin SDK layout fixture
and fixed-timezone POSIX test, all run natively there. Intel macOS still rests
on cross-checks: run the same suites on an Intel runner before calling that
architecture fully supported.

The selfhost CLI selects `-mcpu=` for macOS arm64 and `-march=` for Intel
using Darwin uname's machine field. Both CLI stages exercise `--cpu native`
in the regression suite, and the `macos-latest` CI job validates the arm64
path against Apple clang natively; the Intel `-march=` path is cross-checked
and awaits an Intel runner.

The platform history below describes the established Windows/Linux baseline.

**Aoxn's first-class platform is Windows x86_64. Linux x86_64 is supported
for the compiler, the standard library and the X11 UI backend as of
v0.46.0.**

v0.30.0 cut the platform list down to Windows alone (one installer, one CI
job, one windowing backend). v0.46.0 brings Linux back as a supported
target — the compiler, the whole test suite and an X11 UI backend
rewritten against the v3 `plat_*` contract all run on it — while Windows
keeps everything that is Windows-specific: the installer, the release
packaging and the Win32/GDI UI backend. The macOS core port is
separate from Cocoa UI support; see the validation scope above.

## What ships where

| | Windows (first-class) | Linux (v0.46.0; snapshot 5) | macOS (snapshot 5) |
|---|---|---|---|
| Compiler | `aoxn.exe` | `aoxn` (ELF objects, `-lm` linked automatically) | `aoxn` (Mach-O `.o`, libSystem C math) |
| Installer | `Aoxn-<version>-Setup.exe` | `dist/install-linux.sh` (snapshot 5 source installer; `dist/package-linux.sh` archives) | `dist/install-macos.sh` (build from source) |
| Standard library | full, including `stdlib/net` (WinHTTP) | portable modules; HTTP/SDKs via opt-in `stdlib/net/http_linux` | full; HTTP via opt-in libcurl bridge (`stdlib/net/http_macos`) |
| UI backend | Win32/GDI (`stdlib/ui_win.ax`) | Xlib + Xft (`stdlib/ui_x11.ax`) | Cocoa (`stdlib/ui_cocoa`, Rust AppKit bridge) |
| UI link flags | `-l user32 -l gdi32` | `-l X11 -l Xft` | AppKit/CoreGraphics frameworks (via rustc) |
| CI | `windows-latest`, whole suite | `ubuntu-latest`, whole suite + Xvfb probe | `macos-latest`, whole suite + Cocoa + libcurl + installer probes |
| External tools | clang + MSVC Build Tools | clang (distro packages) | Apple clang (Command Line Tools) + rustc |

## The invariants that make both platforms one codebase

- **`target_os()` folds the BUILD host's OS.** `"windows"` on a Windows
  build, `"linux"` on a Linux build (v0.46.0), `"macos"` in the macOS port
  macOS port, and `"other"` elsewhere. The
  self-hosted compiler has no platform check of its own:
  `selfhost/codegen.ax` folds `target_os()` by calling the builtin, whose
  value was burned in by the compiler that built it — so every stage of
  the fixed-point chain agrees, and the fixed point (both stages built
  and run on the same machine) stays byte-identical by construction.
- **The stdlib branches, it does not fork.** `os.ax`, `stdlib.ax`
  (`exe_path`/`asset_path`) and the UI backends select their platform
  code with `if target_os() == "windows"` at runtime. Every extern for
  both platforms is declared in both files; an unreferenced `extern def`
  produces no symbol reference in the emitted C, so the Windows build
  never links Xlib and the Linux build never links kernel32.
- **`stack_link_flag()`/`default_link_libs()`** (`src/platform.rs`):
  Windows links `-Wl,/STACK:8388608` and folds C math into the CRT;
  Linux passes no stack flag (RLIMIT_STACK governs) and adds `-lm`.
- **The PE installer stub stays Windows-only.** `src/setup/` is cfg-gated: on
  Linux the `aoxn-setup` binary builds as a stub that refuses to run.
  Linux users install from source with `dist/install-linux.sh`; the PE stub is not
  the Linux installer.

## What v0.46.0 brought back

| Restored | Notes |
|---|---|
| `stdlib/ui_x11.ax` | rewritten against the v3 `plat_*` contract: Xft text, selection-based clipboard, event-driven mouse, keymap-swept keyboard state, slot-824 close request |
| `examples/ui_probe_x11.ax` | 30-frame smoke probe, run under Xvfb in CI |
| The Linux CI job | whole suite + Xvfb probe (`apt install clang libx11-dev libxft-dev libfontconfig1-dev xvfb`) |
| POSIX link behavior | `-lm` at link time; no `/STACK` flag; `.o`/no-extension artifacts |

## Still Windows-only (deliberately)

- **The WinHTTP implementation** is Windows-only. The shared HTTP facade and
  OpenAI/Anthropic SDKs also use opt-in libcurl providers on Linux/macOS. Linux
  applications must import stdlib/net/http_linux; Windows remains -l winhttp
  without a Rust provider. SDK imports do not automatically select a provider.
- **The single-file installer / `aoxn-setup`** — one PE artifact, Windows
  packaging. macOS installs from source via `dist/install-macos.sh`; Linux
  installs from source via dist/install-linux.sh.
- **The web benchmark suite's server** (`web/`) — its sockets speak
  ws2_32. `stdlib/socket.ax` now provides a portable blocking IPv4 TCP subset
  (Winsock on Windows, POSIX on Linux) with per-function `target_os()` guards;
  it has Windows loopback evidence and now runs natively in the Ubuntu 22.04/
  24.04 x86_64 and 24.04 ARM64 jobs (2026-11-16). The web suite itself has not
  yet been migrated to the portable module.
- The selfhost CLI's process execution is argv-based on both platforms
  (Windows CRT `_spawnvp` + CRT argument quoting, POSIX `fork`/`execvp`); it does
  not use a shell. The selfhost CLI was exercised on Windows this round; no
  Linux-native CLI result is claimed here.
- **macOS UI** — the `ui_cocoa` backend requires the Rust bridge;
  a native Cocoa window probe (30 frames in an NSWindow) passes on the
  `macos-latest` CI job. The build-from-source installer
  (`dist/install-macos.sh`) ships the backend's sources with the rest of the
  stdlib tree; there is no packaged macOS UI artifact.

## History

- **v0.30.0 (2026-10-02)**: platform list cut to Windows alone; the
  v0.29-era X11 backend, POSIX sockets, POSIX link flags and the
  Linux/macOS CI jobs were removed rather than left to rot.
- **v0.46.0 (2026-10-06)**: Linux returns as a supported target —
  compiler + stdlib + suite green on ubuntu-latest, and the UI toolkit
  gains a second backend through the same `plat_*` contract that kept
  `ui_draw.ax` portable all along.
