# Aoxn

## English

**Compiler 0.55.0; released as pre-release tag `v0.55.0-snapshot-5`
(2026-11-16).** Aoxn is a statically typed
native language with Python-style indentation. The Rust compiler emits ISO C
and invokes external clang. The compiler library has zero external Rust crate
or LLVM library dependencies; package-manager dependencies are locked separately.

The tree includes generics, value-copy structs/arrays, byte strings, dicts,
nullable narrowing, function pointers, exceptions and namespaces. This snapshot
includes sizeof/addr_of, a Python re subset and the closing stdlib batch
(27 of 31 roadmap items implemented, four explicit decisions with substitutes).
The current tree also carries **unreleased** f-string format specifiers
(`f"{x:.2f}"`, `f"{n:08x}"`, `f"{n:b}"`), tuples with multiple assignment
(`def divmod(...) -> (int, int)` then `q, r = divmod(7, 2)`; literal-index reads
only), and the `aoxn test` runner; CHANGELOG `[Unreleased]` tracks them
and no release is implied.
The TS front end is a subset, not a JS runtime. TOML package management, CSS
assets, two API SDKs and a separate Tauri/Monaco IDE are included.

### Platforms and installation

Windows x86_64 is first-class; Linux compiler/portable-stdlib/X11 support dates
to 0.46.0. **The Linux HTTP, installer, socket and doctor work is included and
was natively validated on 2026-11-16**: PR #11 checks are green on Ubuntu
22.04/24.04 x86_64, Ubuntu 24.04 ARM64, macOS and Windows. That run also found
and fixed a Rust-bridge staging race (df4c3dd); a six-thread regression pins it.
Alpine/musl, other distributions, and native Wayland remain unverified. Intel
macOS has cross-check but no separate native evidence.
[Platform support](docs/platform-support.md) is authoritative.

Windows uses the single-file Setup.exe, clang and MSVC-compatible tools;
-InstallClang explicitly enables toolchain downloads, disabled by default.
Linux uses dist/install-linux.sh with Rust/cargo, clang and system C development
tools. HTTP additionally needs rustc, libcurl development files and CA certificates.
X11 requires X11/Xft/fontconfig development libraries and a display. macOS uses
dist/install-macos.sh with Rust and Apple Command Line Tools; Cocoa/HTTP use
opt-in Rust bridges. Linux binary archives can be built locally with
`bash dist/package-linux.sh`; this snapshot's published artifact is the Windows
installer. No native IDE package is claimed.

From a Linux checkout (install Rust separately):

```bash
sudo apt-get install clang build-essential libcurl4-openssl-dev ca-certificates
bash dist/install-linux.sh --prefix "$HOME/.local/aoxn"
export PATH="$HOME/.local/aoxn/bin:$PATH"
aoxn doctor
aoxn run "$HOME/.local/aoxn/examples/hello.ax"
```

The prefix must be an absolute dedicated directory. The complete stdlib tree,
examples and installation docs are copied. --skip-build reuses target/release/aoxn
without cargo. Profiles are untouched by default; --modify-path opts into a
safely quoted PATH entry in ~/.profile. Doctor failure fails installation.
No toolchain download is performed; cargo may fetch locked dependencies.
See [installation](docs/install.md).

### Linux HTTP and SDKs (snapshot 5)

The HTTP layer dispatches Linux/macOS to net_curl_* helpers backed by system
libcurl through the shared stdlib/net/http_macos_native.rs Rust bridge.
Explicitly import the provider alongside the facade, raw transport or SDK:

```python
import * from "stdlib/net"
import * from "stdlib/net/http_linux"

def main() -> int:
    r = http_get("https://example.com")
    if not http_ok(r):
        print(r.errmsg)
        return 1
    print(r.status)
    return 0
```

Save as app.ax; run `aoxn run app.ax` or `cargo run -- run app.ax`. Linux needs
rustc and libcurl development/link files, **not -l winhttp**; rustc propagates
-lcurl. Missing provider imports can produce unresolved aoxn_http_* symbols at
link time, not a guaranteed runtime error. macOS uses stdlib/net/http_macos;
Windows retains WinHTTP and -l winhttp without either provider. The selfhost
CLI cannot load .rs imports. See [OpenAI](docs/openai-sdk.md) and
[Anthropic](docs/anthropic-sdk.md).

### Development and boundaries

```bash
cargo build --release --locked --bin aoxn
cargo run -- check examples/fib.ax
cargo run -- c examples/fib.ax
cargo run -- build examples/fib.ax -o fib
cargo run -- test examples/test_demo.ax
cargo run -- doctor --json
cargo test -p aoxn --locked
cargo test -p aoxn-pkg --locked
```

These are commands, not this batch execution results; no current test totals or
new security audit results are claimed. Windows package tests use
`bash run_pkg_tests.sh` (Smart App Control workaround). IDE checks run separately:
`pnpm --dir ide test`, `pnpm --dir ide typecheck`, and
`cargo test --manifest-path ide/src-tauri/Cargo.toml`.

check/c/symbols do not invoke clang/rustc; test discovers tests without clang,
then builds and runs them like build. test also accepts a directory and runs
each test_*.ax directly inside it (sorted, no recursion), continuing past
failing suites and ending with SUMMARY files=N failed_suites=M. build/run need
clang, .rs additionally
needs rustc. --O0 through --O3 select clang optimization (default O3).
AOXN_CLANG/AOXN_RUSTC select tools; AOXN_HOME/AOXN_STDLIB select paths;
AOXN_NO_CACHE=1 disables the build/run cache and AOXN_CACHE_DIR relocates it.
C externs are fixed-arity; int is C long long, bool is C int. Rust bindings
accept exact scalar ABI pairs. Strings are NUL-terminated bytes; string bounds
clamp but arrays/raw memory are unchecked. Concatenation leaks; no GC, sandbox
or addr_of lifetime checking exists. Selfhost parity/fixed-point checks cover
subsets, not full equivalence. Namespace/selective imports, defaults,
sizeof/addr_of, tuples, .rs/CSS imports and package commands remain outside the
selfhost surface (tuple syntax is a clean stage-2 parse error); the Rust CLI is
the production entry point.

References: [spec](docs/spec.md), [stdlib](docs/stdlib.md),
[Rust interop](docs/rust-interop.md), [C compatibility](docs/c-compatibility.md),
[package manager](docs/pkg-manager.md), [IDE](docs/ide.md),
[security](SECURITY.md), [changelog](CHANGELOG.md). docs/ is maintained; wiki/
is frozen historical material. License: [Apache-2.0](LICENSE).

## 中文

**编译器 0.55.0，已发布预发布标签 `v0.55.0-snapshot-5`（2026-11-16）。**
Aoxn 使用 Python 风格
缩进与静态类型，Rust 编译器生成 ISO C，再调用 clang。编译器库无外部 Rust crate
或 LLVM 库依赖。语言支持泛型、值复制结构体/数组、字节字符串、字典、可空收窄、
函数指针、异常和命名空间。本快照含 sizeof/addr_of、re 子集及标准库收尾：
31 项中 27 项有实现，4 项明确不做并提供替代，不是完整 Python 兼容。当前树另含
**未发布**的 f-string 格式化说明符（`f"{x:.2f}"`、`f"{n:08x}"`、`f"{n:b}"`）、
元组/多返回值（`def divmod(...) -> (int, int)` 后 `q, r = divmod(7, 2)`，
元素只能用字面量下标读取）与 `aoxn test` 测试运行器（发现并运行文件内全部
`def test_*`，失败退出 1；也可传目录，跑目录直接子级里每个 `test_*.ax`，
结束打印 SUMMARY），记录在 CHANGELOG 的 `[Unreleased]` 中，不代表发布。TS 前端为
子集，另有 TOML 包管理、CSS、两个 SDK 和独立 IDE。

Windows 为一等平台，Linux 编译器/可移植标准库/X11 自 0.46.0 起受支持。
**本批 Linux HTTP、安装器、socket、doctor 改动已包含，并于 2026-11-16 原生验证**：
PR #11 的 Ubuntu 22.04/24.04 x86_64、Ubuntu 24.04 ARM64、macOS 与 Windows
检查全绿；该轮运行还发现并修复了 Rust 桥暂存目录竞争（df4c3dd），并有六线程
回归钉住。Alpine/musl、其他发行版与原生 Wayland 仍未验证；Intel macOS 仅有
交叉检查，无独立原生证据。[平台支持](docs/platform-support.md)为准。

Windows 安装器默认不下载工具链，-InstallClang 才启用；链接需 clang/MSVC 工具。
Linux 自备 Rust/cargo、clang、系统 C 开发环境；HTTP 另需 rustc、libcurl 开发文件
和 CA 证书；GUI 另需 X11/Xft/fontconfig 与显示服务。macOS 使用源码安装器、Rust
和 Apple Command Line Tools。Linux 可执行 `bash dist/package-linux.sh` 本地构建
二进制归档；本快照发布的产物为 Windows 安装器。不声明原生 IDE 包。

执行英文节安装命令：`bash dist/install-linux.sh --prefix "$HOME/.local/aoxn"`，
再设置 PATH 并运行 doctor。前缀须为绝对路径专用目录。复制完整标准库树/示例/文档；
--skip-build 复用 target/release/aoxn，无需 cargo；默认不改配置，--modify-path 才
安全引用后追加 PATH 至 ~/.profile；doctor 失败即安装失败。脚本不下载工具链，cargo
可能下载锁定依赖。[安装](docs/install.md)有完整说明。

Linux HTTP 与 SDK 应用必须额外添加 `import * from "stdlib/net/http_linux"`。
英文节有完整 app.ax，运行 `aoxn run app.ax`。公共 HTTP 层调用 net_curl_*，共享
http_macos_native.rs 的系统 libcurl Rust 桥，需要 rustc 和 libcurl 开发/链接文件，
不传 -l winhttp，-lcurl 由 rustc 传播。漏导入可能链接报 aoxn_http_* 未定义，不保证
返回运行时错误。macOS 改用 stdlib/net/http_macos；Windows 仍用 WinHTTP 和
-l winhttp，无需 provider。自举 CLI 不支持 .rs 导入。

英文节列出开发命令，不是本批执行结果；不声明实时测试总数或新安全审计。根包、包
管理器和 IDE 分别测试，Windows 包测试用 bash run_pkg_tests.sh。check/c/symbols
不调用工具链；test 的发现阶段不需要 clang，随后像 build 一样编译并运行；build/run
需 clang，.rs 另需 rustc。--O0 至 --O3 默认 O3；
AOXN_CLANG/AOXN_RUSTC 指定工具，AOXN_HOME/AOXN_STDLIB 指定路径，AOXN_NO_CACHE=1
禁用缓存，AOXN_CACHE_DIR 迁移缓存。C extern 固定参数，int 对应 C long long，bool
对应 C int；Rust 桥只接受精确标量 ABI。字符串是 NUL 结尾字节，字符串边界 clamp，
数组/裸内存不检查；拼接泄漏，无 GC、无沙箱，addr_of 无生命周期检查。自举固定点
与直接 C 比较只覆盖测试子集；命名空间/选择性导入、默认参数、sizeof/addr_of、
元组（stage-2 解析器给出干净的语法错误）、.rs/CSS 导入、`test` 和包命令在其
范围外，生产入口仍为 Rust CLI。

参阅[规范](docs/spec.md)、[标准库](docs/stdlib.md)、[安全政策](SECURITY.md)、
[变更记录](CHANGELOG.md)。docs/ 持续维护，wiki/ 冻结；[Apache-2.0](LICENSE)。
