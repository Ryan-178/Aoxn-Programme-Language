# 标准库路线 TODO（stdlib roadmap）

> 工作文档，不是规范。语言规则见 `docs/spec.md`，两套 API SDK 见
> `docs/openai-sdk.md` 与 `docs/anthropic-sdk.md`（它们共用的底层在
> `stdlib/net/`）。本文件只回答一个问题：**接下来按什么顺序、把哪些
> Python 标准库模块搬进 Aoxn，卡在哪里。**
>
> 模块编号沿用两份需求清单的原编号：§2 是标准库 1–31，§5 是第三方 1–15，
> 方便对照。

---

## 0. 动手之前：六条硬约束

这六条是**已核实**的，不是猜测。它们决定了后面每个模块的可行形态；
写模块之前先读这一节，能省掉大部分返工。

（0.1–0.4 对标准库模块适用；0.5–0.6 主要是为第二份清单里的
科学计算 / 数据分析库准备的，见 §5。）

### 0.1 `stdlib/stdlib.ax` 在自举关键路径上 —— 新模块必须独立成文件

`selfhost/codegen.ax`、`selfhost/driver.ax`、`selfhost/lex_demo.ax` 等
**全部** `import * from "../stdlib/stdlib.ax"`。自举镜像现在已覆盖
v0.40.0 的 dict / `None` / fn-ptr / `raise`（见 AGENTS.md 与
`docs/selfhost.md`），但命名空间绑定、默认参数和选择性导入仍在自举子集
之外，所以这条规则**仍然成立**：自举编译器的依赖越少越稳。

**规则**：新模块一律写成独立文件 `stdlib/<name>.ax`，以
`import * from "stdlib/<name>"` 引入。**只有**当某个模块必须被自举编译器
自身引用时，才写进 `stdlib.ax`，并把语法约束在 selfhost 能编译的子集内。

- ✅ `stdlib/os.ax`、`stdlib/datetime.ax`、`stdlib/re.ax` … 各自成文件
- ❌ 把它们 `import` 进 `stdlib/stdlib.ax`（一步毁掉固定点测试）

### 0.2 argv —— **v0.42.0 已闭合**（`argc()` / `arg(i)`）

不再是阻塞项。当前形态：

- `argc()` 返回参数个数（不含程序自身路径），`arg(i)` 取第 i 个（0 起算，
  越界给 `""`）
- **已落地（unreleased）**：`stdlib/sys.ax`（`sys_argv`/`sys_argc`/
  `sys_platform`/`sys_executable`/`sys_exit`/`sys_maxsize`）与
  `stdlib/argparse.ax`（声明式选项/标志/位置参数解析，见 §2 #24/#25）
- `sys.stdin` **仍然阻塞**（没有控制台输入内建），`sys.executable` 由
  `exe_path()` 承担
- `argparse` 的用例发现仍缺一个语言内测试运行器（见 §0.7）

### 0.3 没有泛型堆容器 —— `collections` / `itertools` 无处落脚

`stdlib.ax` 的 `struct Vec` 是 **int-only**（`vec_push(v: Vec, item: int)`），
另有 `VecVec` 硬编码的嵌套版本。泛型是**单态化**的（按 (类型, 长度)），
所以 `Vec[T]` 可写但会按类型各编一份 —— 需要一个显式的堆容器基座。

**P0 必须先做 `Vec[T]`**，否则 `collections` 整个落不了地。

**v0.44.0 更新**：以**槽位视图纪律**解决，而非新容器类型——`Vec` 槽就是
8 字节：int/bool 原生，f64 走 `store_f64`/`load_f64` 视图（`stdlib/bisect.ax`
的 `_f` 族是成例），string 走 `as_ptr`（`vec_push_str`/`vec_get_str`）。
泛型**结构体**容器仍不可能（无 struct 取址/无 sizeof），异构记录继续走
tagged-slab（`stdlib/json.ax`）。`collections`/`itertools`（P2）可以在此之上动工了。

### 0.4 extern 只能传 int/f64/string/bool，且是定长

- 不能按值传结构体 → 想接 Win32 的 `SYSTEMTIME`、`WIN32_FIND_DATAA` 这类
  结构体，只能**自己在堆上分配 + `load_i64/store_i64` 逐字段读写**
- **变参函数不可用**：`printf`/`sprintf` 会读到未初始化的 XMM 溢出槽
  （已实测：打印出 `2.47e-323`）。格式化必须手写整数数学
- `web/sock_win.ax` 里已有 `load_i64(ts + 8)` 这种写法，直接照抄

### 0.5 没有 `enum`、没有可运行时标记的联合、没有 `Any` —— 异构记录做不了

v0.40.0 的唯一联合形式是 `T | None`，且 `dict[V]` 的**值类型只有一个**。
所以「一列里既有 int 又有 string」「一行数据库记录有多种类型」这类数据结构
**在语言层面无法表达**。

被这条直接判死的：`pandas` 的 DataFrame（§5 #3）、所有 ORM 的行对象
（§5 #12/#14）、`numpy` 的运行期 dtype 分发（§5 #2）。

**已有的正确范式就在仓库里**：`stdlib/json.ax`（v0.44.0 自 `net/` 提升）的 JSON DOM 是一个
**40 字节带 tag 的 slab**（kind / i64 / f64 / ptrA / ptrB），用它绕开了「JSON
值可以是任意类型」这个问题。凡是要做动态值的库（Variant、DataFrame、ORM 行），
**照抄这个 tagged-slab 设计**，不要试图发明别的。

代价是访问要走 accessor（类型检查从静态掉到运行期），换来的是通用性。
文档里要写明这层访问开销，别让用户以为是零成本的。

### 0.6 造不出 `inf` / `nan`，浮点除零也没有保护 —— 所有数值代码都要自己兜

两条都核实过：

- **词法层没有 `inf` / `nan` 字面量**。`src/lexer.rs::scan_number` 只接受
  `数字 [. 数字]`，`codegen_c.rs:537` 那个 `contains("inf")` 是 Rust 侧
  **打印** f64 常量时的分支，不是词法支持。
- **整数的位运算戳不进 float**。v0.38.0 的 `& | ^ ~ << >>` 与 `%` 一样是
  **int-only**，不能拿 IEEE754 的位模式去造 NaN。
- **浮点除零原样发射**（`src/codegen_c.rs:2153`，`Div => "/"`），C 里
  `0.0/0.0` 是**未定义行为**，不是保证给 NaN。

**可行的绕法（都实测可写）**：

- 造 NaN：`extern def log(x: float) -> float`，调 `log(-1.0)`
- 造 ±Inf：`log(0.0)` 给 `-inf`，`-log(0.0)` 给 `+inf`
- 除法：库里统一走一个 `fdiv(a, b)` 辅助函数，自己判零并返回哨兵值

`numpy` / `scipy` / `matplotlib` 这三个库**全部**踩在这条上，它们的每一个
公式（归一化、插值、积分、缩放坐标轴）都得考虑除零与 NaN 传播。
**已闭合（v0.44.0）**：`stdlib/math.ax` 提供 `fdiv` / `is_nan` / `is_inf` /
`is_fin` 与 `NaN()` / `Inf()` / `NInf()`（经 `log(-1)`/`log(0)` 合成）。注意
小写 `nan`/`inf`/`copysign` 的**定义**会与 UCRT 链接符号冲突（数学 extern
与其共享对象文件），大写/带后缀拼写是有意的。

### 0.7 断言有了，运行器还没有 —— `unittest` / `pytest` 差一半

v0.42.0 补上了 `assert(cond[, message])`：失败时打印
`assertion failed at line N: message` 并 exit 1。所以**一个库现在可以自带
自测文件**，只要有人把那些文件跑起来。

**unreleased 更新**：`stdlib/unittest.ax` 已落地（§2 #31）——注册表 +
fn-ptr 用例 + `ut_run` 汇总报告，语言内自测**可以写了**。仍然缺的是
编译器侧的发现/驱动：

- 没有 `aoxn test` 子命令，也没有用例发现约定（`test_*.ax`？）
- 每个测试程序要自己 `def main()` 调 `ut_run` 并把失败汇总成退出码；
  多文件用例要么每个文件编译一次，要么由一个 runner 编译一次再逐个调用
- 在此之前，stdlib 模块的验证仍落在 `tests/<name>.rs`（Rust 侧、内嵌
  `.ax` 源码），所以「给 stdlib 加模块 = 必须会 Rust」这条约束还在

**成本**：小～中（一个 `aoxn test <dir>` 走 loader + 每个 `test_*.ax` 编一次、
跑、汇总；不需要语言改动）。

---

## 1. 批次总览

| 批次 | 主题 | 模块 | 定位 |
|---|---|---|---|
| **P0** | 基座 + 纯 extern/math 能吃的 | `vec` 基座、`time`、`datetime`、`calendar`、`hashlib`、`hmac`、`base64`(补 decode)、`os`(核心)、`pathlib`、`glob` | 立刻能做，风险最低 |
| **P1** | 解析与序列化 | `json`(已有，需提升)、`csv`、`re`、`shutil`、`bisect`、`heapq`、`socket`(已有需上提) | 有工程量但形状清晰 |
| **P2** | 容器与并发 | `collections`、`itertools`、`urllib`、`threading`、`concurrent.futures` | 前两个依赖 P0 基座；线程可行（见 #18），但同步原语要从 extern 起步 |
| **P3** | 语言级阻塞 | `sys`、`argparse`、`multiprocessing`、`pickle`、`logging`、`traceback`、`typing`、`unittest` | 需要先改语言或改需求形态 |

第二份清单（第三方 / 大型库，见 §5）**不套用这四个批次**——它们不是标准库
模块，而是**依赖链上的项目**：`numpy` 在 `scipy` 之前，`Pillow` 在 `opencv`
之前，`matplotlib` 在 `seaborn` 之前，`socket` 在 `requests` 之前。按依赖顺序
排，不按批次排。

**建议起手**：P0 全做完 → P1 的 `json` 提升 + `bisect`/`heapq` →
再评估 P2 并发。P3 整体挂起，等语言侧排期。

---

## 2. 模块清单

可行性图例：
✅ 现在就能写 · ⚠️ 有前置依赖（已注明）· ⛔ 当前语言不支持（已注明原因）

### 1. 文件、路径、操作系统

| # | 模块 | 批次 | 可行性 | 备注 |
|---|---|---|---|---|
| 1 | `os` | P0/P1 | ✅ **已落地 v0.44.0（核心）** | 底座是 Win32：`CreateDirectoryW`、`RemoveDirectoryW`、`GetEnvironmentVariableW`、`SetEnvironmentVariableW`、`GetFileAttributesW`、`CopyFileW`、`MoveFileW`。遍历（`os_listdir`）已随 v0.44.0 落地；文件时间戳仍待 P1（见 `glob`）。所有路径走 **宽字符**（`W` 版本），非 ASCII 路径必须支持。 |
| 2 | `pathlib` | P0 | ✅ **已落地 v0.44.0** | 纯字符串运算（`path_*` 函数族，未做 `Path` struct 包装）（`/ \` 分割、`join`、`suffix`、`stem`、`parent`、`with_suffix`…）。**推荐作为日常写法**，`os.path` 只做兼容门面。 |
| 3 | `shutil` | P1 | ✅ **已落地（unreleased）** | `stdlib/shutil.ax`：copyfile/copytree/rmtree/move，组合 `os.ax`+`pathlib.ax`，**自身零 extern**（双平台由构造保证）。防环两条都做了：reparse point 绝不跟随（copytree 跳过链接子项、拒绝链接根；rmtree 删链接本身）+ 深度上限 64。错误是值（False）。无 archive/which/disk_usage。`tests/shutil.rs` 真实临时树验证。 |
| 4 | `glob` | P0 | ✅ **已落地 v0.44.0** | 建立在 `FindFirstFileW` 上，`*`/`?` 匹配自己写（不用 Win32 的 `FindFirstFile` 通配，它自己就是通配）。`[a-z]` 字符类要写。v0.44.0 落地：matcher + `glob()` 遍历（排序输出、dotfile 规则）；`**` 明确不支持。 |

### 2. 时间、日期

| # | 模块 | 批次 | 可行性 | 备注 |
|---|---|---|---|---|
| 5 | `time` | P0 | ✅ **已落地 v0.44.0** | `QueryPerformanceCounter`（**注意：`timespec_get` 在 Windows 上不链接**，AGENTS.md 已记）+ `GetSystemTimeAsFileTime` / `GetTickCount64`。`sleep` → `Sleep`（毫秒，Python 是秒，要自己换算）。 |
| 6 | `datetime` | P0 | ✅ **已落地 v0.44.0** | 纯整数数学：儒略日/民用日历互转、闰年、`strftime` 的 `%Y-%m-%d %H:%M:%S` 子集。**格式串自己解析**，不要用 `wcsftime`（走 CRT 路径且依赖 locale）。 |
| 7 | `calendar` | P0 | ✅ **已落地 v0.44.0** | 月历网格、日序数、`weekday`/`monthrange`。直接建在 `datetime` 上。 |

### 3. 数据序列化 / 文本解析

| # | 模块 | 批次 | 可行性 | 备注 |
|---|---|---|---|---|
| 8 | `json` | P1 | ✅ **已提升 v0.44.0** | `stdlib/json.ax`（自 `stdlib/net/json.ax` 提升，SDK 同步改 import）+ 补上 `j_read_file`/`j_write_file`（err 2 = I/O 失败，1 = 解析错误）。 |
| 9 | `csv` | P1 | ✅ **已落地（unreleased）** | `stdlib/csv.ax`：RFC 4180 引号转义、内嵌换行、CRLF/LF、空行/空字段/尾部无换行；错误是值（err/msg/line/col）。**不含** Python 的全部 dialect 参数与流式 API。Python `csv` 差分测试见 `tests/csv.rs`。CRLF 直接写 `"\r\n"`（v0.42.0 起有 `\r` 转义）。 |
| 10 | `pickle` | P3 | ⛔ **无反射** | 语言没有「遍历一个值的类型/字段」。可行替代是**显式注册**：程序自己声明要序列化的字段清单，`pickle_dump(recs)`。这样能往返，但和 Python pickle 格式不兼容 —— 若目标是「读 `.pkl` 文件」，**直接不做**。 |
| 11 | `re` | P1 | ✅ **已落地（unreleased）** | `stdlib/re.ax`：回溯匹配器（显式回溯栈 + capture-undo trail，无递归），支持 `* + ? \| () (?:) [] {m,n} .`、字符类/范围/取反、非贪婪、锚点 `^$`、`\b\B`、`\d\w\s\D\W\S`。API：`re_compile/exec/search/match/fullmatch/group/start/end/groups/findall/sub/split`。**不含**：模式内反向引用、lookaround、命名组、内联 flag、`\xHH/\uHHHH`、MULTILINE/IGNORECASE/DOTALL（均干净报错）；POSIX 类按字面类解析（与 Python 同）。字节级而非字符级。`tests/re.rs` 对拍 Python `re` + selfhost 可编译。 |

### 4. 容器、数据结构

| # | 模块 | 批次 | 可行性 | 备注 |
|---|---|---|---|---|
| 12 | `collections` | P2 | ✅ **已落地（unreleased）** | `stdlib/collections.ax`：Deque（环形缓冲，pop 返回 `DqPop{dq,value,ok}` 写回）、Counter（dict[int] 上，`counter_most_common` 稳定排序、平局保插入序）、defaultdict 助手（`dd_int_get`/`dd_str_get`）、OrderedDict 的 `od_move_to_end_*`（dict 本就保序，成本仍是 O(n) 查找）。槽位视图基座（§0.3）之上，无 extern。`tests/collections.rs`。 |
| 13 | `itertools` | P2 | ✅ **已落地（unreleased，降级形态）** | `stdlib/itertools.ax`：**无 `yield`，全部批量返回容器**（模块头已写明）。chain/repeat/range/take/count/cycle/accumulate/zip/product + permutations/combinations（Python 索引序，`VecVec` 行必须 `iv_copy` 拷贝否则全行别名；n>9 置换护栏）。`tests/itertools.rs` 精确输出 + 对拍 CPython 顺序。 |
| 14 | `bisect` | P1 | ✅ **已落地 v0.44.0** | 便宜。`bisect_left/right` 对有序 `Vec[T]`（**注意：泛型按长度单态化，长度是类型的一部分** → 变长容器要用堆容器）。 |
| 15 | `heapq` | P1 | ✅ **已落地 v0.44.0** | 标准二叉堆，`heap_push`/`heap_pop`/`heapify`。和 `bisect` 一起做。 |

### 5. 网络、请求

| # | 模块 | 批次 | 可行性 | 备注 |
|---|---|---|---|---|
| 16 | `urllib` | P2 | ✅ **已落地（unreleased）** | `stdlib/net.ax`：纯 URL 层（`url_parse`/`url_build`/quote/unquote/plus 变体/query 并行 Vec 解析与编码，IPv6 括号主机，默认端口 80/443）+ HTTP 门面（`http_get`/`http_post`/`http_request`/`http_ok`，5s 连接/30s 接收，值错误）。传输仍是 `net/http.ax` 的 WinHTTP（Windows `-l winhttp`，其他平台干净报错）。**不含** urljoin/userinfo/Request 对象；门面不暴露任意响应头（用 `net_header_value`）。`tests/net.rs` 对拍 CPython `urllib.parse` + 本地 mock 服务器。 |
| 17 | `socket` | P1 | ✅ **已落地（unreleased）** | `stdlib/socket.ax` 提供最小 blocking IPv4 TCP：init/connect/listen/accept/send-all/recv/close，错误是 `Sock{fd,err,msg}`。Windows 需 `-l ws2_32`，Linux 无附加库。**不含** DNS/UDP/IPv6/TLS/超时/非阻塞；默认监听不设 SO_REUSEADDR。Windows 实测双向回环；Linux 分支已实现但本轮未原生执行。`web/sock_win.ax` 暂未改为 import 它。 |

### 6. 多线程 / 多进程

| # | 模块 | 批次 | 可行性 | 备注 |
|---|---|---|---|---|
| 18 | `threading` | P2 | ✅ **已落地（unreleased）** | `stdlib/threading.ax`：`thread_spawn(f: fn(int)->int, arg)` 走文档逃生口 `to_int(f)`（Windows `CreateThread`，POSIX `pthread_create`，每个调用都在自己函数内 `target_os()` 守卫）；`thread_join` 写回（退出码进 `.result`）；`Mutex` = `CreateMutexW`/`pthread_mutex_init`，lock/unlock/free；`thread_sleep_ms` 委托 time.ax。**不含** 条件变量/信号量/线程局部存储/守护线程语义。Windows 实测 4 线程×1000 次加锁计数恰为 4000；`tests/threading.rs` + selfhost CLI 钉。 |
| 19 | `multiprocessing` | P3 | ⛔ **明确不做（§4 替代）** | 线程入口不再是问题，但**共享内存 / 管道 / 句柄继承**要写，语言没有共享可变状态的一切原语（没有类、没有闭包、没有 `with`）。替代方案是本地 socket（`stdlib/socket.ax`）或子进程 + `system()`。完成审计见 §7。 |
| 20 | `concurrent.futures` | P2 | ✅ **已落地（unreleased，线程子集）** | `stdlib/futures.ax` 建在 #18 上：`fut_submit`/`fut_wait`/`fut_result`（Future 包 Thread）+ `fut_parallel(f, args)` 批量并行（上限 `FUT_MAX_THREADS()`=64，超出部分**内联执行**而非排队，结果按输入序回填，错误计数进 `FutResults.errors`）。**进程池不做**（#19 ⛔）；无回调/超时/取消。`tests/threading.rs` 同钉。 |

### 7. 编码、哈希加密

| # | 模块 | 批次 | 可行性 | 备注 |
|---|---|---|---|---|
| 21 | `base64` | P0 | ✅ **已落地 v0.44.0** | encode（标准+URL-safe）+ decode（`B64{p,n}`，两套 alphabet 通用、orphan 拒收）；codec 的 `net_b64_*` 原地保留。 |
| 22 | `hashlib` | P0 | ✅ **已落地 v0.44.0** | SHA-256 在 TLS/HTTP2 工作里已经用位运算跑过（v0.38.0 加 `&\|^~<<>>` 就是为此）。v0.44.0 已补 SHA-1 / MD5 与增量上下文（`*_new/_update/_final`），全部对拍已知向量。 |
| 23 | `hmac` | P0 | ✅ **已落地 v0.44.0** | 建在 22 上，标准两趟 HMAC；经 fn-ptr 单实现驱动（`selfhost 尚不可编译`）。 |

### 8. 命令行、工具

| # | 模块 | 批次 | 可行性 | 备注 |
|---|---|---|---|---|
| 24 | `sys` | P3 | ✅ **已落地（unreleased，子集）** | `stdlib/sys.ax`：`sys_argv()`（`argc()`/`arg(i)` 的 Vec 包装）、`sys_argc()`、`sys_platform()`（= `target_os()`）、`sys_executable()`（= `exe_path()`）、`sys_exit(code)`、`sys_maxsize()`（i64 上界）。**不含** `sys.stdin/stdout` 对象（无控制台输入内建）、`sys.modules`（无反射）、`sys.path`（导入路径由编译器决定）。`tests/sys.rs` + `examples/sys_demo.ax`。 |
| 25 | `argparse` | P3 | ✅ **已落地（unreleased，子集）** | `stdlib/argparse.ax`：声明式 `ap_new/ap_opt/ap_flag/ap_pos` + `ap_parse(ap, args: Vec)`，值是 `dict[string]`（`ap_get` 读取），`--help` 置 `help_requested`（`ap_help` 生成文本），错误是值（`ArgResult.err`）。支持 `--name value`/`--name=value`/短选项/位置参数；**不含** 子命令、互斥组、`type=` 回调（`ap_to_int` 助手代替）、nargs。argv 已通（#24），`tests/argparse.rs` + `examples/argparse_demo.ax`。 |
| 26 | `logging` | P3 | ✅ **已落地（unreleased，子集）** | `stdlib/logging.ax`：级别常量 10–50（Python 数值）、`log_new(name)` + `log_set_level` + `log_add_file`（追加 `"ab"`）+ `log_set_stdout`，两通道独立开关；行格式 `YYYY-MM-DD HH:MM:SS LEVEL name: msg`（建在 datetime 上）。**不含** handler 对象树/过滤器/Formatter 类/日志轮转/多 logger 层级传播。`tests/logging.rs` + `examples/logging_demo.ax`。 |
| 27 | `traceback` | P3 | ⛔ 需编译器支持 | `raise`/`try` 只给了控制流，**没有运行时调用栈对象**。要真做，需要 codegen 在每个 raise 点发一条位置常量并在 unwind 时收集 —— **这是语言功能，不是库功能**，且要同步 selfhost 镜像。 |

### 9. 其他

| # | 模块 | 批次 | 可行性 | 备注 |
|---|---|---|---|---|
| 28 | `math` | P0 | ✅ **已落地 v0.44.0** | `stdlib.ax` 已有 `sqrt/floor/ceil/abs/min/max/clamp/gcd/lcm/isqrt/is_prime/hypot`。v0.44.0 已补（FFI + `pi()/e()`，Python 风格名字），并内置 §0.6 工具箱（小写 `nan`/`inf`/`copysign` 定义撞 UCRT 链接符号，故 `NaN()/Inf()/copysign_f`）。 |
| 29 | `random` | P1 | ✅ **已落地（unreleased）** | `stdlib/random.ax`：Park–Miller MINSTD（16807 / 2^31−1），拒绝采样消除取模偏差，错误不推进状态，跨进程可复现。**非密码学安全**；不提供系统熵 API（不用时间种子冒充）。不是 Python 的 Mersenne Twister 序列。 |
| 30 | `typing` | P3 | ⚠️ 多数是 no-op | Aoxn **已经是静态类型 + 单态化**，`typing` 的运行时部分（`TypeVar`/`Generic`）在这里没有对应物。真正缺的是**别名与协议**：需要 `type X = Vec[int]` 形式的类型别名（spec roadmap 第 4 条「模块限定」的近邻）。**先别做**，文档里说明等价物即可。 |
| 31 | `unittest` | P3 | ✅ **已落地（unreleased，降级形态）** | `stdlib/unittest.ax`：无类/无 `with`，改为**注册表 + fn-ptr**：`ut_suite()` + `ut_add(s, name, f: fn() -> void)` + `ut_run(s) -> TestReport`（通过/失败计数与失败名单）；断言是自由函数 `ut_assert_true/eq_i/eq_s/eq_f/raises`（失败经 `raise` 上报，`ut_run` 逐个捕获）。**不含** fixture 类、setUp/tearDown、参数化、用例发现（仍无 `aoxn test`，见 §0.7）。**仓库级测试基建仍是 `cargo test`**；本模块服务于语言内自测。`tests/unittest.rs` + `examples/unittest_demo.ax`。 |

---

## 3. 每批次的通用任务清单

任何一个模块落地，下面这几件**都要做完**（AGENTS.md 的硬要求）：

- [ ] 模块本体 `stdlib/<name>.ax`，头部注释写清：**依赖哪些 extern、
      需要哪个 `-l` 链接库**（如 `ws2_32` / `winhttp` / `bcrypt`）
- [ ] **不放进 `stdlib/stdlib.ax`**（§0.1），除非 selfhost 真的要用
- [ ] `examples/<name>_demo.ax` 一个最小可跑示例
- [ ] `tests/<name>.rs` 测试；纯计算模块可无外部依赖，网络/文件模块
      用 **mock 服务**（照抄 `tests/openai_sdk.rs` 里那套 Aoxn 写的 mock server）
- [ ] `docs/<name>.md` 文档；`CHANGELOG.md` 记版本
- [ ] `docs/spec.md` 的 Roadmap 第 7 条「Standard library expansion」打勾
- [ ] 若用了 v0.40.0 新特性（dict/None/fn-ptr/raise），在文件头注明
      「**selfhost 尚不可编译**」，并确认固定点测试没把它纳入范围

（v0.44.0 批次以合并式 `docs/stdlib.md` 交付参考文档，未逐模块开 `docs/<name>.md`；
后续单模块可以继续遵循逐文件约定。）

---

## 4. 明确不做 / 用别的替代

写在这里，免得每次都重新讨论：

| 需求 | 不做的东西 | 用这个 |
|---|---|---|
| 进程间通信 | `multiprocessing` | `web/sock_win.ax` 的本地 socket；或起子进程 + stdin/stdout |
| 对象持久化 | `pickle`（无反射） | **JSON**（`stdlib/json.ax` 已有） |
| 调用栈回溯 | `traceback` | 编译器 `Diag`（诊断是编译期的，够用） |
| 正则引擎的极致性能 | `re` 的 NFA/DFA 优化 | 回溯版够用；真要快就换 `str.find` 手写扫描 |
| 类型体操 | `typing` | 现有静态类型 + 泛型单态化 |
| 随机可复现 | `rand()` | 自实现 PCG32（见 #29） |
| 测试框架 | `pytest`；`unittest` 的类/fixture 形态 | 仓库级测试仍是 `cargo test`（`tests/*.rs`）；语言内自测用 `stdlib/unittest.ax`（注册表 + fn-ptr 降级形态，#31） |
| 深度学习 | `pytorch` | numpy + scipy 那一层；要 GPU 就不在 Aoxn 里做 |
| 计算机视觉 | `opencv-python` 全家桶 | 明确只做子集（灰度/模糊/Sobel/阈值/缩放） |
| 数据库对象映射 | ORM（sqlalchemy / django 的那一半） | `stdlib/db.ax` 的参数化查询 + 按表生成 struct |

---

## 5. 第三方 / 大型库（第二份清单）

这 15 个不是标准库模块，是**依赖链上的项目**。编号沿用需求清单原编号 1–15。
图例同 §2。

### 5.1 依赖顺序

```
stdlib/socket.ax ──► requests(1)
stdlib/os.ax ──────► Pillow(7) ──► opencv(8)
stdlib/math.ax ────► numpy(2) ──► scipy(6)
                      │           pandas(3, 需先解决 §0.5)
                      └───────► matplotlib(4) ──► seaborn(5)
stdlib/net.ax ─────► flask(13) ──► django(12)
stdlib/re.ax ──────► lxml(10), beautifulsoup4(9)
```

### 5.2 清单

| # | 库 | 可行性 | 备注 |
|---|---|---|---|
| 1 | `requests` | ✅ **P1，高回报** | `stdlib/net/http.ax` 的 WinHTTP 传输已经在了，`requests` 就是它上面一层：session/cookie 持久化、重定向策略、`params`/`json=` 便捷参数、`.raise_for_status()`。**接口层一天能写完**，是第三方清单里最便宜的高价值项。 |
| 13 | `flask` | ✅✅ **最高 ROI，建议排第一** | 零件基本齐了：`web/serve.ax` 是一个能跑的 HTTP/1.1 服务器（静态文件 / ETag / Range / `/metrics` 都有，`docs/web-benchmark.md` 里有实测数据）。**v0.40.0 的 fn-ptr 让路由表第一次可表达**——`struct Route{pattern, handler: fn(Request) -> Response}`。剩下的是路由匹配、`Request`/`Response`、Jinja 子集模板、session cookie。**把 `web/serve.ax` 泛化成 `stdlib/wsgi.ax`**，flask 建在上面。 |
| 9 | `beautifulsoup4` | ✅ **P1，高回报** | HTML 分词 + 树构建，**和 `selfhost/parser.ax` 是同一类活**，有现成参照可抄结构。要补的是：几百个 HTML 实体的解码表（`&amp;` `&#x4e2d;` …）、HTML5 的容错规则（隐式闭合、错误嵌套）、CSS 选择器求值。**和 `re`(§2 #11) 一起做最划算**，两者互为用例。 |
| 10 | `lxml` | ✅ P1 | XML 比 HTML 简单得多（**没有容错要求**，解析器可以短一半），XPath 求值也直接。⚠️ 但 lxml 的核心卖点是「比 bs4 快」——在 Aoxn 里 bs4 还不存在，比较没有意义。**按「XML + XPath」定位就行**，别承诺性能。 |
| 2 | `numpy` | ⚠️ **P2，最大工程** | 需要 `struct NDArray{data, shape, stride, ndim}` + 步进索引 + 广播 + 花式索引 + 归约轴。**但 dtype 的运行期分发被 §0.5 判死**——现实做法是**按 dtype 各一个单态化类型**（`f64arr` / `i64arr`），靠泛型让同一份算法为每种类型各编一份（§0.3 的泛型机制支持这个）。程序自己选类型，编译器保证没有混合运算。**这个降级必须在模块头写明**，它和 numpy 的语义不同。 |
| 6 | `scipy` | ⚠️ **P2/P3，最机械** | 纯数值、无 I/O，所以**语言约束最少**：积分（自适应辛普森）、优化（BFGS/牛顿）、线性代数（LU / QR / 特征值）、信号（FFT / 滤波）。每个算法都是循环和算术，没有语言层面的坎。**但每一个都踩 §0.6**（除零、NaN 传播）和**没有 `**` 运算符**（`pow_f` 自己写，spec roadmap 第 1 条）。建议在 `numpy` 之后做。 |
| 4 | `matplotlib` | ✅ **P2** | **现成的路已经铺好了**：绘图后端直接建在 UI 工具包的 `plat_fill_rect` / `plat_text` / `plat_measure` 上（`stdlib/ui_draw.ax`，已经是 GDI 实测可用），再包一层坐标变换、刻度、图例。⚠️ **但 `plat_*` 目前没有画线/多边形的图元**（只有矩形、文字、裁剪、pump），要先给 `ui_draw.ax` 加 `plat_line` / `plat_polyline`，并**在 `ui_win.ax` 里实现**——`tests/ui.rs` 那三个契约测试（widget 层不得出现平台符号、两个后端实现同一 `plat_*` 集、每个被调的 `plat_*` 两边都存在）会强制你两边都做，**不要只改一边的后端**。 |
| 5 | `seaborn` | ⚠️ 依赖 4 | 统计默认值（分箱、置信带）、KDE、配色板。`matplotlib` 落地后成本很低，**跟着 4 一起做，不要单独立项**。 |
| 7 | `Pillow(PIL)` | ⚠️ P2，分档 | BMP 读写很容易（头 + 像素）。**PNG 需要自己实现 zlib inflate**（可行，~600 行，还要配 deflate 才能写）。**JPEG 解码是大工程**（基线解码器 ~1500 行，含霍夫曼、IDCT、YCbCr）。另一条路是 WIC（Windows 自带的图像组件），但它是 COM 接口，定长 `extern def` 调 COM 会很别扭——**优先自己写**。图像算子（裁剪/缩放/灰度/旋转）都建立在解码之上，所以格式支持决定了这个库的规模。 |
| 3 | `pandas` | ⛔ **需要重新设计** | DataFrame 的核心卖点就是**一列可以混合类型**，正好撞在 §0.5 上。两条出路：(a) 每列固定类型 → 那就是 numpy 的列式视图，不是 pandas；(b) 走 `json.ax` 那种 **tagged slab** → 通用性有了，访问要经 accessor、运行期判类型、性能大幅下降。**建议 (b) 作为「够用就好」的版本，明确写进文档它不是 pandas。** 依赖 `numpy`(2) 与 `csv`(§2 #9)。 |
| 14 | `sqlalchemy` | ⛔ ORM 层阻塞 / ✅ 底层可行 | 一行数据库记录必然混合类型 → §0.5，ORM 的行对象建不出来。**分解做法**：先做 `stdlib/db.ax`（ODBC 或 WinSQL 的参数化查询、连接池、事务、结果集游标），**把 ORM 砍掉**；真要对象映射，改成「按表生成 struct + 代码生成」，而不是运行期反射。这是有价值的部分，也是唯一可行的部分。 |
| 12 | `django` | ⚠️ **拆开看，一半可行** | 模板引擎、请求/响应、中间件、认证 session —— 在 flask 那套之上都能做。**但 admin / forms / ORM 全部依赖 ORM 与反射**（§0.5）。**结论：不要做「django」，做 flask + 模板引擎。** django 的价值主要在 ORM 和 admin，正是这里做不了的部分。 |
| 8 | `opencv-python` | ⛔ **远期，只做子集** | 依赖 `numpy`(2) + `Pillow`(7)，而「opencv」的本体是几百个精心优化的算子。现实可行的子集：灰度化、高斯模糊、Sobel、二值化、缩放、形态学开闭运算。**性能不可能接近 C++ 版本**（Aoxn 无 SIMD、无内联汇编、逃逸分析有限）。**要张量运算就做 numpy 那一层，不要追 opencv 的性能。** |
| 11 | `pytorch` | ⛔ **建议不做** | GPU 要 CUDA，语言层没有相邻的任何东西。CPU 反向自动微分的 tape 引擎理论上可行（显式栈代替闭包），但真实代价远超收益。**要做数值计算就做 numpy + scipy 那一层**，深度学习框架是另一个量级的工程。 |
| 15 | `pytest` | ⚠️ **优先级低** | 发现用例要 argv（§0.2）；fixture 需要 `yield`（没有）→ 只能改成 setup/teardown 显式配对；`parametrize` 可做。v0.40.0 的 `raise`/`try` 让 `assert_raises` 能写。**当前阶段的真正答案仍是 `cargo test` + `tests/*.rs`**，`pytest` 只有在语言模块本身需要自测时才值得做。 |

### 5.3 第三方库通用约定

- [ ] **先确认它依赖的标准库模块已落地**（见 §5.1 依赖图），不要在缺基座时开工
- [ ] 若绕过 §0.5 / §0.6，**在模块头注释写明降级形态**（同 §3 最后一条）
- [ ] 每个数值库都要跑一遍**边界用例**：除零、空数组、单元素、`NaN` 输入
- [ ] `docs/<name>.md` 里**明确写「这不等于 Python 的同名库，差异在哪」**——
      降级过的库（numpy 的 dtype、pandas 的列类型、db 的无 ORM）尤其要写
- [ ] 若扩展了 UI 工具包的 `plat_*`，**两个后端都要实现**，`tests/ui.rs` 会查

---

## 6. 建议的起手顺序（可直接开工）

1. ✅ **`stdlib/time.ax` + `stdlib/datetime.ax`**（v0.44.0，连同 `calendar`）—— 最纯粹的 extern + 整数数学，
   零语言风险，且 `logging`/`calendar` 都等它。
2. ✅ **`stdlib/pathlib.ax`**（v0.44.0）—— 纯字符串运算，立刻能用，`os` 的门面基础。
3. ✅ **`stdlib/hashlib.ax` + `hmac` + `base64` decode**（v0.44.0）—— 算法验证密集、
   测试好写（对拍已知向量）。
4. ✅ **`stdlib/glob.ax` + `os` 遍历**（v0.44.0）—— 第一次碰 Win32 目录枚举。
5. ✅ **`Vec[T]` 堆容器基座**（v0.44.0 以槽位视图纪律解决，见 §0.3）—— P2 的入场券。
6. ✅ **把 `stdlib/net/json.ax` 提升为 `stdlib/json.ax`**（v0.44.0，含文件 I/O）—— 已有实现，是最快的一块 P1。

第三方清单接在后面（依赖齐了才动）：

7. ✅ **`stdlib/socket.ax` 上提**（unreleased，最小 blocking IPv4 TCP；Windows 实测，
   Linux 分支未原生执行）—— `requests`(1) / `flask`(13) 的网络底座。
   同一批还落地了 **`csv`**(#9) 与 **`random`**(#29)；CSV/random 不解决 `re`/HTML，
   也不等于 numpy/pandas 的数据能力。
8. ✅ **`stdlib/re.ax`**（unreleased，回溯子集，对拍 Python `re`）—— 和 `lxml`(10)、
   `beautifulsoup4`(9) 共用分词基础；HTML 标记提取用例见 `examples/re_demo.ax`。
9. **`flask`(13)** —— 第三方清单里 ROI 最高；`net.ax` 与 `re` 现已落地，
   零件齐了，但 flask 本体属 §5 第三方范围，**标准库批次不做**。
10. ✅ **`stdlib/math.ax` 补 `fdiv` / `is_nan` / `is_inf`**（v0.44.0 已做）—— §0.6 说这是
    三个数值库的公共前置，**先做它，比做 numpy 更划算**。

标准库收尾批次（unreleased，§2 剩余全部可行模块）：

11. ✅ **`stdlib/shutil.ax`**（#3）—— 组合 os/pathlib，零 extern，防环两条。
12. ✅ **`stdlib/collections.ax` + `stdlib/itertools.ax`**（#12/#13）—— 槽位视图
    基座之上；itertools 全批量形态（无 yield）。
13. ✅ **`stdlib/net.ax`**（#16 urllib）—— 纯 URL 层对拍 CPython + WinHTTP 门面
    对本地 mock；传输层补上空 header 块守卫（WinHTTP 对空头块报 87）。
14. ✅ **`stdlib/threading.ax` + `stdlib/futures.ax`**（#18/#20）—— `to_int(f)`
    逃生口 + 每函数 `target_os()` 守卫；4×1000 加锁计数钉 + selfhost CLI 钉。
15. ✅ **`stdlib/sys.ax` + `argparse.ax` + `logging.ax` + `unittest.ax`**
    （#24/#25/#26/#31）—— P3 里语言已解锁的四项，全部降级形态落地。

**至此 §2 的 31 项全部处置完毕**：27 项落地（含降级形态），4 项按 §4 决定
不做并有替代（pickle/multiprocessing/traceback/typing）。逐项证据见 §7。

---

## 7. 完成审计（§2 全部 31 项，2026-11-14）

**结论：标准库路线（§2）关闭。** 27 项有模块 + 测试 + 示例 + 文档；
4 项是 §4 的明确不做决定，替代物已在仓库里。§5 第三方清单**不属于**
标准库范围，保持原样（依赖底座 socket/net/re/csv 等已齐）。

| # | 模块 | 处置 | 证据（模块 / 测试 / 示例） |
|---|---|---|---|
| 1 | os | ✅ v0.44.0 | `stdlib/os.ax` / `tests/osglob.rs` / `examples/os_demo.ax` |
| 2 | pathlib | ✅ v0.44.0 | `stdlib/pathlib.ax` / `tests/pathlib.rs` / `examples/pathlib_demo.ax` |
| 3 | shutil | ✅ unreleased | `stdlib/shutil.ax` / `tests/shutil.rs` / `examples/shutil_demo.ax` |
| 4 | glob | ✅ v0.44.0 | `stdlib/glob.ax` / `tests/osglob.rs` |
| 5 | time | ✅ v0.44.0 | `stdlib/time.ax` / `tests/datetime.rs` / `examples/time_demo.ax` |
| 6 | datetime | ✅ v0.44.0 | `stdlib/datetime.ax` / `tests/datetime.rs` |
| 7 | calendar | ✅ v0.44.0 | `stdlib/calendar.ax` / `tests/datetime.rs` |
| 8 | json | ✅ v0.44.0 提升 | `stdlib/json.ax` / `tests/json_fileio.rs` / `examples/json_demo.ax` |
| 9 | csv | ✅ unreleased | `stdlib/csv.ax` / `tests/csv.rs`（对拍 CPython）/ `examples/csv_demo.ax` |
| 10 | pickle | ⛔ 不做（§4） | 无反射；替代 = `stdlib/json.ax`（显式字段序列化） |
| 11 | re | ✅ unreleased | `stdlib/re.ax` / `tests/re.rs`（对拍 CPython + selfhost 钉）/ `examples/re_demo.ax` |
| 12 | collections | ✅ unreleased | `stdlib/collections.ax` / `tests/collections.rs` / `examples/collections_demo.ax` |
| 13 | itertools | ✅ unreleased（批量形态） | `stdlib/itertools.ax` / `tests/itertools.rs`（对拍 CPython 顺序）/ `examples/itertools_demo.ax` |
| 14 | bisect | ✅ v0.44.0 | `stdlib/bisect.ax` / `tests/containers.rs` / `examples/bisect_demo.ax` |
| 15 | heapq | ✅ v0.44.0 | `stdlib/heapq.ax` / `tests/containers.rs` |
| 16 | urllib | ✅ unreleased | `stdlib/net.ax` / `tests/net.rs`（对拍 CPython `urllib.parse` + 本地 mock HTTP）/ `examples/net_demo.ax` |
| 17 | socket | ✅ unreleased | `stdlib/socket.ax` / `tests/socket.rs`（双向回环）/ `examples/socket_echo.ax` |
| 18 | threading | ✅ unreleased | `stdlib/threading.ax` / `tests/threading.rs`（4×1000 计数 + selfhost CLI 钉）/ `examples/threading_demo.ax` |
| 19 | multiprocessing | ⛔ 不做（§4） | 替代 = `stdlib/socket.ax` 本地回环 / `system()` 子进程 |
| 20 | concurrent.futures | ✅ unreleased（线程子集） | `stdlib/futures.ax` / `tests/threading.rs`（与 threading 合用）/ `examples/threading_demo.ax`（合用，含 `fut_parallel`/`fut_submit`） |
| 21 | base64 | ✅ v0.44.0 | `stdlib/base64.ax` / `tests/hashlib.rs` |
| 22 | hashlib | ✅ v0.44.0 | `stdlib/hashlib.ax` / `tests/hashlib.rs`（已知向量）/ `examples/hashlib_demo.ax` |
| 23 | hmac | ✅ v0.44.0 | `stdlib/hmac.ax` / `tests/hashlib.rs`（fn-ptr，selfhost 不可编译已注明） |
| 24 | sys | ✅ unreleased（子集） | `stdlib/sys.ax` / `tests/sys.rs` / `examples/sys_demo.ax` |
| 25 | argparse | ✅ unreleased（子集） | `stdlib/argparse.ax` / `tests/argparse.rs` / `examples/argparse_demo.ax` |
| 26 | logging | ✅ unreleased（子集） | `stdlib/logging.ax` / `tests/logging.rs` / `examples/logging_demo.ax` |
| 27 | traceback | ⛔ 不做（§4） | 无运行时调用栈对象；替代 = 编译器 `Diag` + `raise` 消息 |
| 28 | math | ✅ v0.44.0 | `stdlib/math.ax` / `tests/math.rs` / `examples/math_demo.ax` |
| 29 | random | ✅ unreleased | `stdlib/random.ax` / `tests/random.rs` / `examples/random_demo.ax` |
| 30 | typing | ⛔ 不做（§4） | 语言已是静态类型 + 单态化；等价物即语言本身，文档已说明 |
| 31 | unittest | ✅ unreleased（降级形态） | `stdlib/unittest.ax` / `tests/unittest.rs` / `examples/unittest_demo.ax` |

**§3 清单核对（unreleased 十模块）**：

- [x] 模块本体独立成文件，头部注明 extern / `-l` 依赖（threading：Windows
      无附加库、POSIX `-l pthread`；net 门面：Windows `-l winhttp`；
      shutil/collections/itertools/sys/argparse/unittest 零 extern，
      logging 经 datetime/os 间接用 CRT）
- [x] 均未 import 进 `stdlib/stdlib.ax`（该文件零 import，自举关键路径不受影响）
- [x] 每个模块都有可跑示例：futures 与 threading 合用
      `examples/threading_demo.ax`（含 `fut_parallel`/`fut_submit`），
      其余九个各一个 `examples/<name>_demo.ax`
- [x] 每个模块都有测试：futures 与 threading 合用 `tests/threading.rs`
      （net/threading 用 mock/线程实测，itertools 对拍 CPython）
- [x] 参考文档集中在 `docs/stdlib.md`（十节）；`CHANGELOG.md` 记 `[Unreleased]`
- [x] selfhost 边界：threading/futures 经 selfhost CLI 实测可编译运行；
      使用 dict/fn-ptr/raise 的模块（argparse/logging/unittest/collections）
      头部已注明降级形态与自举状态
- [x] 双平台纪律：`tests/stdlib_defect_pins.rs` 的 Linux 折叠符号扫描覆盖
      threading/futures/shutil/net；路径分隔符扫描覆盖全部跨平台模块

**遗留（不阻塞关闭，记录在案）**：`aoxn test` 运行器（§0.7）、socket 的
Linux 原生执行证据（等 CI）、§5 第三方库全部未开工（范围外）。