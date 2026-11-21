# Security policy

## English

### Scope and reporting

**Compiler 0.55.0; released as pre-release tag `v0.55.0-snapshot-5`
(2026-11-16).** Fixes target current
0.55.x. The tree also carries unreleased f-string format specifiers and the
`aoxn test` runner (CHANGELOG `[Unreleased]`). Windows is first-class; Linux has an established compiler/stdlib/X11
baseline. The Linux HTTP, installer, socket and doctor changes were natively
validated on 2026-11-16 (PR #11: Ubuntu 22.04/24.04 x86_64, Ubuntu 24.04 ARM64,
macOS, Windows); Alpine/musl and native Wayland remain unverified. See
[platform support](docs/platform-support.md).

Report privately to **zmjsjsg3@163.com** before disclosure. Include commit,
platform, doctor output, minimal input, commands, behavior and impact; for
miscompilation attach generated C and O0 reproduction. Redact secrets/private
paths and state credit preference. Encryption can be arranged by email.
Targets: acknowledgement in 3 business days, assessment in 10, coordinated
disclosure within 90 days of acknowledgement or when a fix ships, whichever
comes first. Critical issues target urgent fixes; exploitation may require
earlier guidance. Credit needs consent; no bounty.

### Trust boundaries

**Compilation/execution is not a sandbox.** Imports read files, assets write
files, clang generates native code, .rs imports compile a whole Rust file.
Scanning creates bindings, not confinement: ignored inline modules still compile.
Tools/selfhost run use argv, not shell; programs/FFI/network/threads retain user
permissions. Isolate untrusted projects using OS/container controls.
Tool paths, PATH, environment overrides and flags are user configuration.
Writable executable/Rust-artifact caches are trusted and can substitute native
code; never share them with untrusted users. Zero compiler-library dependencies
do not eliminate package-manager, IDE/JS/Tauri, toolchain or system-library risks.

C externs are fixed-arity; int is C long long, bool is C int. Wrong signatures
can link yet cause UB. Rust bindings accept exact i64/f64/C character pointer/
void pairs, not arbitrary Rust types. Strings are NUL-terminated bytes; string
bounds clamp, arrays/raw memory do not. sizeof/addr_of add no lifetime/ownership/
alias checks; struct parameters remain copies. Concatenation leaks; no GC or
general memory-safety guarantee exists. Regex lacks a step budget and can take
exponential time; use OS resource limits. Compiler recursion has stack limits.
Random is not cryptographic; threads require synchronization. shutil link/depth
guards are not permissions or race-free confinement. Logging writes chosen paths.
Socket is blocking IPv4 TCP, not TLS; MSG_NOSIGNAL/fd0 fixes do not sanitize input.
An f-string format specifier must be literal text: the compiler parses it
(sign/zero/width/precision/kind, or base) and rebuilds the C format from those
fields, so spec bytes never reach the emitted C; width is capped at 1024,
precision at 512, and the selfhost mirror runs the same parse.
`aoxn test` generates a driver in the cache directory and compiles/runs the
entry file's collected tests with the user's permissions, like `run`;
discovery itself reads sources only and needs no clang. Given a directory it
repeats that compile/run for each `test_*.ax` directly inside it, and the file
name filter means it never opens anything else in that directory.

### HTTP, packages and installers

Windows retains WinHTTP. Linux/macOS opt into system libcurl through the shared
http_macos_native.rs Rust bridge. Linux requires `stdlib/net/http_linux`, rustc,
libcurl development/link files and CA certificates; macOS uses
`stdlib/net/http_macos`. Missing providers may fail at link time. Windows needs
neither; selfhost cannot load .rs. The bridge is trusted native code. HTTP(S)
is enabled, certificate/hostname checking remains on, automatic redirects are
disabled, and SDK policy handles credentials above transport. CA/proxy settings
and endpoints remain trust inputs. Plain HTTP lacks TLS confidentiality.
Streaming uses a worker and buffered memory, not a bounded-memory guarantee;
uploads are copied for worker ownership. Redact keys/bodies in logs/dumps.
No new TLS/security audit or native Linux success is claimed.

Package checksum pins unpacked content; tarball_sha256 checks served bytes when
supplied. Missing legacy wire hashes never waive content checks. Warm reuse
excludes only its own index.ax shim. Extraction rejects escapes/absolute paths/
links; containment must hold across host spellings. Untrusted indexes can replace
content and hashes. Plain HTTP registries/trust tiers are not authentication/audits.

Linux installation requires an absolute dedicated prefix, rejects dangerous roots,
copies the whole stdlib without deleting unrelated entries, and treats doctor
failure as fatal. It may overwrite installed files and has no transactional
rollback. Profiles are untouched by default; --modify-path appends safely quoted
PATH to ~/.profile. --skip-build trusts target/release/aoxn without cargo.
The script downloads no toolchains; cargo may fetch locked dependencies. Path
checks do not defeat hostile filesystem races. Windows downloads are opt-in
(-InstallClang), with post-doctor quarantine detection. macOS profile behavior
is separate. Linux local binary archives include checksums, but checksums alone
are not publisher authentication; no archive or release was published here.

IDE filesystem/preview commands are workspace-bounded, but Terminal deliberately
runs a user shell with only initial cwd bounded. A compromised webview can execute
outside the folder; do not expose remotely without authorization. Assistant keys
stay in Rust; previews cap at 16 MiB. CSS output names are checked; URLs do not
control roots. Tailwind does not silently install Node.

### Verification limits

Doctor checks portable modules and native backend/provider source completeness
and runs a smoke program, not every library/TLS/GUI. Optional rustc discovery
does not prove bridge linking. Deterministic C, parity and fixed points cover
finite subsets, not full equivalence or security. CI configuration does not
prove this changed tree ran. No fresh audit results/test totals are asserted.
Review lockfiles and rerun audits. The existing IDE glib exception stays in
ide/src-tauri/audit.toml, not newly endorsed by this update.

## 中文

编译器 0.55.0，已发布预发布标签 **v0.55.0-snapshot-5**（2026-11-16），修复当前
0.55.x；树中另有未发布的 f-string 格式化说明符与 `aoxn test` 运行器（CHANGELOG `[Unreleased]`）。Windows 为一等平台，Linux 已有编译器/库/X11 基线；本批 HTTP/安装器/
socket/doctor 已由 2026-11-16 PR #11 原生验证（Ubuntu 22.04/24.04 x86_64、
Ubuntu 24.04 ARM64、macOS、Windows）；Alpine/musl 与原生 Wayland 仍未验证；
Intel macOS 无独立原生证据。
漏洞先私信 **zmjsjsg3@163.com**，附提交/平台/doctor/最小输入/命令/影响，误编译
附 C/O0 复现，脱敏并说明署名，可协商加密。目标 3 工作日确认、10 工作日初评，
确认后 90 天或修复时披露（先到者）；紧急问题优先，利用中可提前提示，署名须同意，无奖金。

**编译执行不是沙箱**。导入读文件、资源写文件，clang/rustc 编译原生代码；扫描器
只生成绑定，被忽略模块仍编译。工具用 argv 非 shell，但程序/FFI/网络/线程有用户权限，
请用系统/容器隔离。路径/环境/链接配置由用户控制；缓存可信可替换原生代码，不可与
不可信用户共享。零编译器依赖不消除其他供应链风险。C extern 固定参数，int=C long long、
bool=C int，错误声明可产生 UB；Rust 只支持精确标量 ABI。字符串 NUL 结尾且边界 clamp，
数组/裸内存不检查。sizeof/addr_of 无生命周期/所有权检查，结构体参数是副本，拼接泄漏，
无 GC/通用内存安全保证。re 无预算可指数耗时、编译递归有栈限，需资源限制；random 非
密码学，线程自己同步，shutil 守卫不是权限/消除竞态，logging 写指定路径，socket 非 TLS。
f-string 说明符必须是字面文本：编译器解析符号/补零/宽度/精度/类型（或进制）后重建
C 格式串，原文不进入生成 C；宽度上限 1024、精度上限 512，自举编译器使用同一解析。
`aoxn test` 在缓存目录生成驱动并像 `run` 一样以用户权限编译/执行入口文件收集到的测试；
发现阶段只读源码、不需要 clang。传入目录时对直接子级里每个 `test_*.ax` 重复该编译/执行，
文件名过滤意味着不会打开该目录里的其他文件。

Windows 保留 WinHTTP；Linux/macOS 显式启用共享 libcurl Rust 桥。Linux 导入
`stdlib/net/http_linux`，需 rustc/libcurl 开发文件/CA；macOS 用
`stdlib/net/http_macos`。漏导入可链接失败，Windows 无需 provider，自举不支持 .rs。
桥为可信原生代码；仅 HTTP(S)，保留证书/主机名校验、禁自动重定向，SDK 管凭据。CA/
代理/端点是信任输入，HTTP 无 TLS。流式线程/缓冲非有界保证，上传复制持有；日志须脱敏。
不声明新安全审计或 Linux 原生成果。包内容与传输摘要分离，旧索引不免内容验证；热复用
只排除自有 shim，解包须内含。不可信索引可改内容与哈希；明文 HTTP/信任层级非认证审计。

Linux 须绝对专用目录，拒危险根，完整复制且不删无关条目；可覆盖且失败不回滚，doctor
失败即失败。默认不改配置，--modify-path 才安全引用追加 ~/.profile；--skip-build 信任
现有 release 程序，无需 cargo。脚本不下载工具链，cargo 可获取锁定依赖；路径检查不防
竞态。Windows 下载 opt-in、自检后检测隔离；macOS 配置行为独立。Linux 本地归档带摘要，
但摘要本身不认证发布者；本次未发布归档或版本。
IDE 文件/预览受限，但 Terminal shell 仅初始 cwd 受限，不可无授权远程暴露；助手密钥留
Rust，预览限 16 MiB。CSS 检查输出名，URL 不控制根；Tailwind 不偷偷装 Node。

doctor 检查源文件并跑小程序，不证明全部库/TLS/GUI；rustc 行不证明可链接。parity/
固定点仅有限子集，非安全审计；CI 配置不证明修改树执行。不声明新审计/测试数，须重审
锁文件。glib 例外仍在 ide/src-tauri/audit.toml，本次不重新背书。
