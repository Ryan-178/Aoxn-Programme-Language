# Installing Aoxn

**One file.** `Aoxn-<version>-Setup.exe` carries the entire toolchain — the
compiler, the standard library, the UI toolkit and the examples — and
installing it is a double-click. The single-file installer targets
**Windows** (x86_64), the first-class platform. **Linux** and **macOS**
install from source — see [macOS](#macos-build-from-source) below and
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

If none matches, the install still succeeds and `aoxn doctor` says so, naming
what is missing. Pass `-InstallClang` to have the installer fetch LLVM itself.

Aoxn lowers programs to ISO C and hands them to **clang**; the compiler
executable is not self-contained. The installer looks for clang in this order:

1. `$AOXN_CLANG` — an explicit path wins over everything else
2. `PATH`
3. `%LOCALAPPDATA%\aoxn\toolchain\bin\clang.exe` — drop a portable LLVM here
   to make the install self-contained
4. `C:\Program Files\LLVM\bin\clang.exe`

If none matches and you did not pass `-NoClang`, the installer runs
`winget install LLVM.LLVM`.

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

## macOS (build from source)

macOS has no single-file packaged artifact yet. `dist/install-macos.sh` is the
native equivalent of the Windows installer: it builds the compiler with cargo
and lays the toolchain out exactly the way the compiler discovers it at
runtime (`src/paths.rs` — the install root is the parent of `bin/`).

Prerequisites (the script reports them and stops; it **downloads nothing**):

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

The native Cocoa UI backend and the libcurl HTTP transport are validated by
the `macos-latest` CI job (a Cocoa probe and a loopback HTTP probe). Until
that job runs green on real Apple hardware, treat macOS as a port under
validation rather than a fully supported platform — see
`docs/platform-support.md`.

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

前置条件（脚本只报告并停止，**绝不下载任何东西**）：Rust（`cargo`，
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
