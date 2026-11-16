# Installing Aoxn

**One file.** `Aoxn-<version>-Setup.exe` carries the entire toolchain — the
compiler, the standard library, the UI toolkit and the examples — and
installing it is a double-click. The single-file installer targets
**Windows** (x86_64), the first-class platform. **Linux** and **macOS**
install from source — see [Linux](#linux-unreleased-source-installer) and [macOS](#macos-build-from-source) below and
`docs/platform-support.md` for what ships where.

## Install

Double-click it. A window appears, laid out like the Python installer for
Windows: a mark, a headline, and one large **Install Now** button. Nothing is
written to disk until you click it.

**Install Now** unpacks the toolchain, adds `aoxn` to your PATH and finishes by
running `aoxn doctor`, which compiles and executes a test program. The window
then says **Successfully installed** and shows you where — and only then. If
anything fails, the failure page names the cause.

Where things land:

```text
%LOCALAPPDATA%\aoxn\          (override with -Prefix <dir> or %AOXN_HOME%)
  bin\aoxn.exe                the compiler driver
  lib\stdlib\*.ax             standard library + UI toolkit
  examples\*.ax               runnable samples
  docs\install.md             this file
  LICENSE  README.md  CHANGELOG.md
```

Open a **new** terminal afterwards so it picks up the updated PATH.

Scripted / unattended installs (no window, progress on stdout):

```powershell
.\Aoxn-0.36.0-Setup.exe -Console -Prefix C:\Tools\aoxn
```


**Waiting for it from PowerShell.** The setup binary is a **GUI-subsystem**
executable (PE subsystem 2), because double-clicking it must not flash a
console window. The PowerShell call operator does **not** wait for a
GUI-subsystem process and does **not** set `$LASTEXITCODE` from it, so the
obvious gate does not do what it looks like it does:

```powershell
& $setup -Console -Prefix $prefix
if ($LASTEXITCODE -ne 0) { throw "install failed" }   # WRONG
```

`& $setup` returns immediately, the install is still running, `$LASTEXITCODE`
is still unset 鈥?and `$null -ne 0` is **true**, so the gate throws on the spot
while the install carries on in the background. Use:

```powershell
$p = Start-Process -FilePath $setup -ArgumentList @("-Console", "-Prefix", $prefix) -Wait -PassThru -NoNewWindow
if ($p.ExitCode -ne 0) { throw "install failed (exit $($p.ExitCode))" }
```

The tell in a log is the ordering: the failure appears *before* the install's
own last lines.
| flag | meaning |
| --- | --- |
| `-Console`, `-Quiet` | no window; write progress on stdout (CI, scripts) |
| `-Prefix <dir>` | install root (default: `$AOXN_HOME`, else `%LOCALAPPDATA%\aoxn`) |
| `-InstallClang` | download LLVM via winget if no clang is found (off by default) |
| `-NoClang` | never download anything (the default) |
| `-Uninstall` | remove the install and the PATH entry |

| `-?` | help |


Uninstalling is the same file with `-Uninstall`.

## What the installer does — and does not

It **downloads nothing**. It unpacks the toolchain, adds it to your PATH, and
runs `aoxn doctor`. That is the whole job, and it takes seconds.

An earlier version reached out to `winget install LLVM.LLVM` whenever it found
no clang. That turned a ten-second install into a multi-minute one, with a
progress bar that said nothing about a download running behind it — the kind of
thing that reads as *hung*. If you want the download, ask for it:
`-InstallClang`.

## The C toolchain

Aoxn lowers programs to ISO C and hands them to **clang**; the compiler
executable is not self-contained. The installer looks for clang in this order:

1. `$AOXN_CLANG` — an explicit path wins over everything else
2. `PATH`
3. `%LOCALAPPDATA%\aoxn\toolchain\bin\clang.exe` — drop a portable LLVM here
   to make the install self-contained
4. `C:\Program Files\LLVM\bin\clang.exe`

If no usable clang/toolchain is found, doctor reports the missing prerequisite.
Pass `-InstallClang` to opt into fetching LLVM on Windows; Linux installation
fails when doctor fails and never downloads a toolchain.

**Linking** on Windows also needs the **MSVC Build Tools** — clang detects
them automatically, and the `aoxn doctor` smoke test reports them when they
are missing:

```powershell
winget install Microsoft.VisualStudio.2022.BuildTools `
  --override "--quiet --add Microsoft.VisualStudio.Workload.VCTools"
```

## Verify

```powershell
aoxn doctor
```

```console
Aoxn 0.30.0 doctor
  [    ] version      0.30.0
  [    ] platform     windows x86_64 (target_os() = windows)
  [ok  ] install root C:\Users\you\AppData\Local\aoxn
  [ok  ] stdlib       C:\Users\you\AppData\Local\aoxn\lib\stdlib
  [    ] examples     C:\Users\you\AppData\Local\aoxn\examples (14 .ax files)
  [ok  ] clang        C:\Program Files\LLVM\bin\clang.exe  clang version 20.1.8
  [ok  ] smoke test   ok  (aoxn-doctor-ok)
  [    ] cache dir    C:\work\target\cache

status: ok — `aoxn run <file.ax>` is ready to use.
```

`aoxn doctor --json` emits the same report as JSON for scripts and agents;
`--no-smoke` skips the compile step.

## Use the standard library

With an install in place, `stdlib` resolves **by name** from any directory:

```Aoxn
import * from "stdlib"

def main() -> int:
    print(isqrt(99))
    return 0
```

`import * from "stdlib/ui_win"` pulls in the UI toolkit; link with
`-l user32 -l gdi32`:

```powershell
aoxn run examples\ui_gallery.ax -l user32 -l gdi32
```

Resolution order for a bare specifier is: a project-local package in
`aox_modules/<name>/`, then the installed stdlib. A local package always
wins. `$AOXN_STDLIB` overrides the stdlib location entirely — that is how to
test an unreleased stdlib.

## Build from source

```powershell
git clone https://github.com/AlonechatWorkspace/Aoxn-language
cd Aoxn-language
cargo build
cargo run -- run examples\hello.ax
```

To produce the installer yourself:

```powershell
powershell -ExecutionPolicy Bypass -File dist\package.ps1
# -> dist\release\Aoxn-<version>-Setup.exe
```

A source checkout also works in place — `cargo run -- run examples\hello.ax`
finds `stdlib/` next to the checkout.

## Linux (unreleased source installer)

Version remains 0.55.0. These Linux installer/HTTP changes are in progress;
**native Linux validation of the new batch is pending**, not implied by Windows
regressions or cross-checks. Prior macOS CI success is historical (2026-11-15).

Install Rust/cargo separately, plus clang and a system C development environment.
Debian/Ubuntu HTTP prerequisites (rustc is also required):

```bash
sudo apt-get install clang build-essential libcurl4-openssl-dev ca-certificates
# Optional X11 GUI development files:
sudo apt-get install libx11-dev libxft-dev libfontconfig1-dev
bash dist/install-linux.sh --prefix "$HOME/.local/aoxn"
export PATH="$HOME/.local/aoxn/bin:$PATH"
aoxn doctor
aoxn run "$HOME/.local/aoxn/examples/hello.ax"
```

The prefix must be an absolute dedicated directory, not /, HOME itself or a
shared system root. The default is ~/.local/aoxn. The script copies bin/aoxn,
the **complete** lib/stdlib tree (including nested SDKs and .rs bridge sources),
examples, docs/install.md and top-level license/readme/changelog. Reinstallation
merges trees and overwrites matching files, without deleting unrelated entries;
there is no transactional rollback. Doctor failure is fatal, not success with a
warning. Doctor checks source completeness and smoke compilation, not all TLS/GUI
functionality. The script never downloads toolchains; cargo may fetch locked
dependencies while building `cargo build --release --locked --bin aoxn`.

| Option | Behavior |
|---|---|
| --prefix PATH | Absolute dedicated install directory |
| --skip-build | Reuse executable target/release/aoxn; cargo not required |
| --no-modify-path | Leave profiles untouched (default) |
| --modify-path | Append a safely quoted PATH entry to ~/.profile |

PATH changes do not alter the parent shell; export PATH manually as above or
start a login session that reads ~/.profile. No sudo is needed for a writable
user prefix. GUI execution needs a display; Xvfb can supply headless X11 tests.

For HTTP, add `import * from "stdlib/net/http_linux"` in your application
alongside stdlib/net, stdlib/net/http or either SDK. Run `aoxn run app.ax` without
-l winhttp. rustc links the shared http_macos_native.rs bridge with system libcurl;
-lcurl propagates automatically. Missing provider imports can cause unresolved
aoxn_http_* link symbols. The bridge requires libcurl development/link files and
CA certificates; core programs need neither provider nor libcurl. The selfhost
CLI cannot load .rs imports. Windows WinHTTP remains unchanged; macOS uses
stdlib/net/http_macos. See [stdlib](stdlib.md), [OpenAI](openai-sdk.md) and
[Anthropic](anthropic-sdk.md).

## Linux binary archive (unreleased)

On a native Linux x86_64 or aarch64 host, run `bash dist/package-linux.sh`.
The script builds the release compiler and produces a `.tar.gz` plus SHA-256
sidecar under `dist/release`; `--output DIR` chooses an empty output location,
`--skip-build` reuses the release executable. It does not publish anything.
Verify with `sha256sum -c *.sha256`, extract the archive, then run its
`dist/install-linux.sh --prefix "$HOME/.local/aoxn"`. The packaged installer
uses its bundled compiler and whole stdlib tree without cargo; clang remains
external, and HTTP applications still require rustc and libcurl development files.
Ubuntu CI installs from this archive and runs installed HTTP/X11 probes.
Native results for this branch remain pending.

中文：Linux 原生运行 `bash dist/package-linux.sh` 本地打包，生成 tar.gz 与 SHA-256，
不发布版本。校验后解包，运行包内 dist/install-linux.sh；使用随包编译器，无需 cargo，
clang 仍外置，HTTP 仍需 rustc/libcurl 开发文件。Ubuntu CI 验证打包后安装与 HTTP/X11，
本分支原生结果待取得。

## macOS (build from source)

macOS has no single-file packaged artifact yet. `dist/install-macos.sh` is the
native equivalent of the Windows installer: it builds the compiler with cargo
and lays the toolchain out exactly the way the compiler discovers it at
runtime (`src/paths.rs` — the install root is the parent of `bin/`).

Prerequisites (the script does not install toolchains; cargo may fetch locked dependencies):

- **Rust** (`cargo`) — install from <https://rustup.rs>.
- **Apple clang** — `xcode-select --install` (the Command Line Tools). clang
  must resolve an installed SDK.
- **rustc** is also needed for the native Cocoa UI backend and the macOS
  libcurl HTTP bridge (both are Rust `.rs` imports); the core compiler and the
  portable stdlib do not need it.

From the repository root:

```bash
bash dist/install-macos.sh                  # installs to ~/.local/aoxn
bash dist/install-macos.sh --prefix /opt/aoxn
bash dist/install-macos.sh --skip-build     # reuse target/release/aoxn
```

Where things land (the **whole** stdlib tree, including the `net/`, `openai/`
and `anthropic/` subdirectories the Windows package omits):

```text
~/.local/aoxn/                (override with --prefix <dir>)
  bin/aoxn                    the compiler driver
  lib/stdlib/...              standard library + UI + SDK transports
  examples/...                runnable samples
  docs/install.md             this file
  LICENSE  README.md  CHANGELOG.md
```

The script adds `<prefix>/bin` to PATH in `~/.zshrc` (or `~/.bash_profile`
when that is your shell), then runs `aoxn doctor`. Open a **new** terminal
afterwards. `target_os()` reports `"macos"`; objects are Mach-O `.o` and
executables have no suffix.

The prior macOS Apple Silicon CI result (2026-11-15) includes Cocoa, libcurl
and installer probes. This is historical evidence, not validation of the new
Linux changes or the modified shared bridge; see `docs/platform-support.md`.

---

## 中文速查

**一个 exe 装全部。** 下载 `Aoxn-<version>-Setup.exe`，双击即可：窗口参照
Python 官方安装器的布局——一个标识、一句标题、一个大的 **Install Now** 按钮；
在你点击之前，磁盘上不会写入任何东西。点下后解包工具链、把 `aoxn` 加进 PATH，
再跑一次 `aoxn doctor` 自检（真的编译并运行一个程序），通过才显示
**Successfully installed** 并告诉你装到了哪里；失败则直接指出原因。
装完请**开一个新的终端**。

默认安装目录 `%LOCALAPPDATA%\aoxn`（可用 `-Prefix <dir>` 或 `%AOXN_HOME%` 改）：

```text
%LOCALAPPDATA%\aoxn\bin\aoxn.exe    编译器
%LOCALAPPDATA%\aoxn\lib\stdlib\    标准库 + UI 工具箱
%LOCALAPPDATA%\aoxn\examples\       示例程序
```

静默安装（脚本 / CI，无窗口）：

```powershell
.\Aoxn-0.36.0-Setup.exe -Console -Prefix C:\Tools\aoxn
```

参数：`-Console` / `-Quiet`（无窗口）、`-Prefix <dir>`（安装目录）、
`-InstallClang`（找不到 clang 时用 winget 下载 LLVM，默认关闭）、
`-NoClang`（绝不下载，默认值）、`-Uninstall`（卸载）、`-?`（帮助）。

**安装器默认不下载任何东西**：只解包、改 PATH、跑 doctor，几秒完成。
早期版本找不到 clang 就自动 `winget install LLVM.LLVM`，把十秒的安装变成好几分钟，
进度条对背后的下载只字不提——那种体验等同于「卡死」。想要下载就明确要求：
`-InstallClang`。

**必须另装 clang**（Aoxn 生成 C 交给它编译链接）。安装程序会依次找
`$AOXN_CLANG` → `PATH` → 安装目录下的 `toolchain\bin\clang.exe` →
`C:\Program Files\LLVM\bin`；都没有时安装依然成功，由 `aoxn doctor` 指明缺什么。
Windows 上**链接**还需要 MSVC Build Tools（clang 会自动探测）：

```powershell
winget install Microsoft.VisualStudio.2022.BuildTools `
  --override "--quiet --add Microsoft.VisualStudio.Workload.VCTools"
```

自检与用法：

```powershell
aoxn doctor            # 打印安装根目录、stdlib、clang，并真的编译运行一个程序
aoxn doctor --json     # 同样的信息，JSON 输出
aoxn run examples\hello.ax
```

装好后任何目录都能直接 `import * from "stdlib"`（不需要相对路径）。
从源码构建并自行打包：`cargo build`，然后
`powershell -ExecutionPolicy Bypass -File dist\package.ps1`。

### macOS（从源码安装）

macOS 暂时没有单文件安装包，`dist/install-macos.sh` 是 Windows 安装器的原生
对应物：用 cargo 编译编译器，再按编译器运行时的发现规则（`src/paths.rs`，
安装根目录是 `bin/` 的父目录）铺好工具链。

前置条件（脚本不安装工具链，但 cargo 可能获取锁定依赖）：Rust（`cargo`，
<https://rustup.rs>）、Apple clang（`xcode-select --install`，需能解析已安装
的 SDK）；原生 Cocoa UI 后端与 macOS libcurl HTTP 桥还需要 rustc（二者都是
Rust `.rs` 导入），核心编译器与可移植标准库不需要它。

在仓库根目录执行：

```bash
bash dist/install-macos.sh                  # 装到 ~/.local/aoxn
bash dist/install-macos.sh --prefix /opt/aoxn
bash dist/install-macos.sh --skip-build     # 复用 target/release/aoxn
```

它会把**整棵** stdlib 树（含 Windows 包省略的 `net/`、`openai/`、`anthropic/`
子目录）装到 `<prefix>/lib/stdlib`，把 `<prefix>/bin` 加进 `~/.zshrc`（若你的
shell 是 bash 则写 `~/.bash_profile`）的 PATH，再跑 `aoxn doctor`。装完请开一个
**新终端**。`target_os()` 返回 `"macos"`，目标文件是 Mach-O `.o`，可执行文件
无后缀。Cocoa UI 后端与 libcurl HTTP 传输由 `macos-latest` CI job（Cocoa 探针 +
环回 HTTP 探针）验证；在该 job 于真实 Apple 硬件上跑绿之前，macOS 仍属「验证中
的移植」而非完全支持的平台——详见 `docs/platform-support.md`。

### Linux 源码安装（未发布）

版本仍为 0.55.0，本批 Linux HTTP/安装器改动尚待原生 Linux 验证；Windows 回归/
交叉检查不替代原生执行，macOS 2026-11-15 成功为历史证据。
自行准备 Rust/cargo、clang、C 开发环境；Debian/Ubuntu HTTP 依赖使用
`sudo apt-get install clang build-essential libcurl4-openssl-dev ca-certificates`，
另需 rustc。X11 可装 libx11-dev/libxft-dev/libfontconfig1-dev，运行需显示服务。
仓库根执行 `bash dist/install-linux.sh --prefix "$HOME/.local/aoxn"`，然后
`export PATH="$HOME/.local/aoxn/bin:$PATH"`、`aoxn doctor`。
前缀须绝对专用目录；完整复制标准库含 SDK/.rs、示例/文档，不删除无关条目但可覆盖。
--skip-build 复用 target/release/aoxn，无需 cargo；默认不改 profile，--modify-path
才安全引用追加 ~/.profile，新登录或手工 export 生效。doctor 失败即失败，不回滚。
脚本不下载工具链，cargo 可获取锁定依赖；自检不证明所有 TLS/GUI。
HTTP 应用显式添加 `import * from "stdlib/net/http_linux"`，运行 `aoxn run app.ax`，
不传 -l winhttp；rustc 自动传递 -lcurl，漏导入可链接报 aoxn_http_* 未定义。
Windows 不变，macOS 用 stdlib/net/http_macos；自举不支持 .rs。
