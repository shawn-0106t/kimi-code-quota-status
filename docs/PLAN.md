# quota-status 实现计划（PLAN）

- 版本：v1.3（2026-10-01；同步 SPEC v1.3——纯 mtime 新鲜判定与 send/body 阶段错误归类细分）
- 版本历史：v1.2（2026-10-01；同步 SPEC v1.2——P5 体积验收区间改为 ≤5MB）；v1.1（2026-10-01；同步 SPEC v1.1 修订）
- **完成状态：P0–P5 已于 2026-10-01 全部实现并通过阶段验收**；2026-10-01 第三轮独立 code review 发现 2 Major + 3 Minor 已全部修复并通过复核（结论"可交付"，报告见 `docs/REVIEW3.md`）。逐项验收数据、体积偏差说明与待真机确认项见 README「验收记录」；下文任务清单与验收标准保留撰写时原貌，不作勾选回填。**注意**：P2 任务清单中「缓存损坏（JSON 非法）→ 锚定 + 回拨 + 派生」一条按 SPEC v1.3 §4.1/§9 的纯 mtime 语义执行（损坏但新鲜不派生），冲突时以 SPEC 为准。
- 契约依据：`docs/SPEC.md` v1.3（唯一事实来源；本计划一切行为要求以 SPEC 条目为准，冲突时以 SPEC 为准）
- 路径约定：相对本仓库根；`<kimi_home>` = `~/.kimi-code`（受 env `KIMI_CODE_HOME` 覆盖，SPEC §5.2）
- 工程布局决策（SPEC 未规定，本计划定为如下，可调整）：cargo 工程 = 本仓库根，即 `Cargo.toml`、`.cargo/config.toml`、`src/*.rs`、`tests/golden.rs`、`testdata/golden/`（本项目新增 golden 用，绝不回写 `repos/` 下参考仓库）
- 参考资产路径勘误：golden 矩阵实际位于 `repos/kimi-planbar-tui/go/testdata/golden/`（30 个 `quota-*.txt`，本计划撰写时已 `ls | grep -c` 核实为 30）；`repos/kimi-planbar/go/testdata/golden/` 不存在（SPEC §2.3 勘误 1）

## 阶段总览

| 阶段 | 名称 | 核心产出 | 依赖 |
|---|---|---|---|
| P0 | 工程骨架 | 可编译的单 crate + release profile + 依赖锁定 | 无 |
| P1 | 核心逻辑移植 | credentials / http / quota 解析 + 单测 | P0 |
| P2 | 缓存与刷新进程模型 | cache 模块 + `--refresh` + detached spawn + 防风暴 | P0, P1 |
| P3 | 渲染与字段配置 | render / config 模块 + quota-bar.toml + 宽度降级 + UTF-8 | P0, P2 |
| P4 | 测试与 golden parity | 30 case 逐字节对齐 + month 新 case + `--test-fetch` | P1, P3 |
| P5 | 构建交付与安装文档 | 体积/性能达标 exe + README 安装文档 + 真机验收 | P0–P4 全部 |

---

## P0 工程骨架

### 目标

建立满足 SPEC §11 全部工程约束的 cargo 单 crate：Windows x64 静态 CRT、release profile 五项优化、体积可控的依赖集，以及三模式入口骨架（`--refresh` / `--test-fetch` / 默认渲染，均为 stub）。本阶段结束时 `cargo build --release` 可产出单 exe。

### 任务清单

- [ ] 仓库根创建 `Cargo.toml`：package 名 `quota-status`，edition 与 rust-version 按本机 toolchain 锁定
- [ ] `Cargo.toml` 配置 release profile（SPEC §11）：`opt-level = "s"`、`lto = true`、`codegen-units = 1`、`strip = true`、`panic = "abort"`
- [ ] 创建 `.cargo/config.toml`：`rustflags = ["-C", "target-feature=+crt-static"]`，target 锁定 `x86_64-pc-windows-msvc`
- [ ] 引入依赖（SPEC §11 白名单）：`serde`（derive）、`serde_json`、`chrono`、`toml`、`ureq`（+ rustls，打包根证书）、`filetime`、`windows-sys`；**不引入 tokio/async**
- [ ] `src/main.rs`：解析 argv 分发三模式（`--refresh` / `--test-fetch` / 无参数），各分支暂为 stub 直接 exit 0；预留模块文件（`mod` 声明可后补）
- [ ] 确认仓库 `.gitignore` 已含 `target/`（已有 `.gitignore`，核对补齐）

### 涉及文件

- 新建：`Cargo.toml`、`.cargo/config.toml`、`src/main.rs`
- 修改：`.gitignore`（按需补 `target/`）
- 生成：`Cargo.lock`（提交，保证可复现构建）

### 验收标准

1. `cargo build --release` 退出码 0，无 warning 级别以上的未处理项
2. `ls -l target/release/quota-status.exe` 存在且为单文件（无同目录 dll 依赖；`dumpbin /dependents` 或直接观察无 `*.dll`）
3. `target/release/quota-status.exe --refresh` 与 `--test-fetch`（stub）均立即退出、exit code 0（`echo $?` 验证）
4. 记录此时体积基线（`stat -c %s target/release/quota-status.exe`）——预算判定在 P5，此处仅留档

### 风险与回退

- **风险：ureq + rustls 静态链接后体积逼近或超出 ≤5MB 预算**（SPEC §11：超预算时优先换 HTTP/TLS 后端；撰写时预估 3–5MB，实测 1.7MB，风险未发生）。回退：换 rustls 后端feature 组合，或改用更轻 client；仍超则回报决策（SPEC 留了 `reqwest` blocking + rustls 的备选，但体积通常更大）
- **风险：`panic = "abort"` 改变错误处理语义**（无 unwind）。回退：P1 起所有解析路径坚持防御式 `Result` 传播（SPEC §6 本就要求不 panic），不依赖 catch_unwind
- **风险：crt-static 与某些依赖（如 windows-sys feature）组合编译失败**。回退：临时去掉 crt-static 定位是否为 CRT 静态链接问题，再针对性调整 feature

---

## P1 移植核心逻辑

### 目标

从 `repos/kimi-planbar-tui/rust/src/` 抽取凭证链、HTTP client 语义、额度解析三块（SPEC §2.2），完成 SPEC 要求的两处语义升级（超时 8s、5h 段 window 条件匹配）与一处扩展（month 段），并带上单元测试。本阶段结束时"取数 + 解析"的纯逻辑部分完整可用（尚无缓存/渲染）。

### 任务清单

- [ ] 抽取 `repos/kimi-planbar-tui/rust/src/credentials.rs`（147 行）为 `src/credentials.rs`：原样移植 `load_token` 凭证链（credentials.json `access_token` + `expires_at` > now+30s 校验 → config.toml 逐行扫 `providers.` 节兜底）与 `<kimi_home>` 解析（`KIMI_CODE_HOME` 覆盖）；将 kimi_home 解析提为 pub 供 P2/P3 复用
- [ ] 原样移植 credentials.rs:115-147 的两个既有单测函数（覆盖紧凑/带空格节名、节结算、空 api_key 拒绝场景，SPEC §10.1）
- [ ] 抽取 `repos/kimi-planbar-tui/rust/src/http.rs`（16 行）语义为 `src/http.rs`：OnceLock 共享 client、构建失败降级 `None`；**超时由 10s 改 8s**（SPEC §2.2/§5.1 决策）；注意 ureq 错误需映射为 .NET 风格类型名：超时 → `TaskCanceledException`，其他传输错误 → `HttpRequestException`（SPEC §6.10）
- [ ] 抽取 `repos/kimi-planbar-tui/rust/src/quota.rs`（374 行）为 `src/quota.rs`：
  - [ ] `QuotaResult` 模型（quota.rs:40-48）扩展 `month: Option<Segment>`，序列化 `skip_serializing_if = "Option::is_none"`（SPEC §5.3）
  - [ ] 移植全部防御规则（SPEC §6 十条）：数字 string 建模 + number 兜底、limit ≤ 0 钳 1、NaN/inf 归零、`i64::MIN` 敌意值 + `saturating_add`、resetTime 宽松阶梯、boosterWallet 防御、余额 `(raw + 500000) / 1_000_000` 四舍五入
  - [ ] **升级**：5h 段由"只取 `limits[0]`"（quota.rs:115-123 原状）改为 window 条件匹配——优先 `window.duration == 300 && window.timeUnit == "TIME_UNIT_MINUTE"`，找不到回落 `limits[0]`（SPEC §6.4）
  - [ ] **新增**：month 段取 `root.totalQuota`，有非零 limit 才产生段（SPEC §6.6）
  - [ ] 周段取顶层 `root.usage`（对象才解析）；错误类型名四种：`no-token` / `HttpRequestException` / `TaskCanceledException` / `JsonException`
- [ ] 移植 quota.rs:243-374 既有单测到 `src/quota.rs` 的 `#[cfg(test)]`
- [ ] 新增针对两处升级的单测：5h window 匹配（命中 / 不命中回落 `limits[0]`）、month 段（有 limit / 无 limit / limit 为 0）

### 涉及文件

- 新建：`src/credentials.rs`、`src/http.rs`、`src/quota.rs`
- 修改：`src/main.rs`（挂 `mod` 声明；`--test-fetch` 分支可临时调用"凭证链 → GET → 解析 → pretty JSON 打印"做手工冒烟，缓存写入留 P2）
- 只读参考：`repos/kimi-planbar-tui/rust/src/credentials.rs`、`http.rs`、`quota.rs`、`polling.rs`（polling.rs:20-41 的"LKG + fast-retry"语义在 P2 实现，本阶段不移植定时循环部分，SPEC §2.2）

### 验收标准

1. `cargo test` 全绿，且测试列表中至少包含：credentials 两个既有用例函数（覆盖三组场景）、quota.rs 既有用例全集、5h window 匹配两例、month 段三例（`cargo test -- --list | grep -c test` 计数留档）
2. SPEC §6 十条防御规则每条至少有一个对应用例（对照 §10.1 清单逐条勾验；蓝本 quota.rs:243-374 与 `repos/kimi-planbar-tui/go/internal/core/quota_test.go`）
3. 真机冒烟（可选，需 managed OAuth 已登录）：`target/release/quota-status.exe --test-fetch` 输出合法 pretty JSON（2 空格缩进、camelCase）且 `error == null`；无凭证环境输出 `error == "no-token"`（SPEC §10.3）

### 风险与回退

- **风险：ureq 错误分类与被抽取 client 不对齐**（超时/连接失败/非 2xx 的归类边界）。回退：以单测钉死三类映射（超时、传输错误、HTTP 状态非 2xx → `HttpRequestException`，SPEC §5.1），ureq 3.x 若无独立 Timeout 错误类型则用 `ElapsedTime`/duration 判定包装
- **风险：5h window 匹配升级改变既有 golden 行为**（原 quota.rs 只取 `limits[0]`，而 golden 矩阵是 Go 侧生成的）。处置：以 SPEC §6.4 为准（window 匹配），P4 golden 对齐时若因此出现 byte 差异，检查 Go 侧参考实现的匹配语义而非回退 Rust 实现
- **风险：抽取时带入 kimi-planbar-tui 的无关依赖**（TUI 框架等）。回退：抽取以"裸函数 + serde_json::Value 输入"为界，不携带 app/state 模块引用

---

## P2 缓存与刷新进程模型

### 目标

实现 SPEC §4.2/§4.4/§5.3 的完整数据层：缓存文件原子写、mtime 过期判定与回拨防风暴、30s fast-retry、detached `--refresh` 子进程派生。本阶段结束时，渲染模式骨架能"读缓存 → 判过期 → 回拨 → 派生"，`--refresh` 能端到端回填缓存。

### 任务清单

- [ ] 新建 `src/cache.rs`：缓存路径 `<kimi_home>/cache/quota-status.json`（随 `KIMI_CODE_HOME` 联动）；读写 `QuotaResult`（camelCase，schema 按 SPEC §5.3，含 `fetchedAt`/`error`）
- [ ] 原子写：`mkdir -p` 缓存目录 → 写同目录 `quota-status.json.tmp` → `fs::rename` 替换（Windows 下 std rename 带 `MOVEFILE_REPLACE_EXISTING`，SPEC §4.2）
- [ ] 过期判定与 mtime 回拨：`age >= TTL` 时先用 `filetime` crate（Windows `SetFileTime`）把 mtime 回拨为 `now - TTL + retry_seconds`（默认 `now - 60s + 30s`），再派生刷新（SPEC §4.4；回拨计算单测：`now-TTL+30`）
- [ ] 首跑/缓存缺失/缓存损坏（JSON 非法）：创建空缓存文件锚定 mtime → 回拨 → 派生；损坏当缺失处理（SPEC §5.3/§9）
- [ ] 新建 `src/console.rs`：定义 `CREATE_NO_WINDOW | DETACHED_PROCESS`（0x08000000 | 0x00000008）creation flags 常量（P3 再加宽度查询）
- [ ] 渲染模式派生逻辑：`Command::new(<自身绝对路径>).arg("--refresh")` + 上述 flags + stdio 三路 DEVNULL；spawn 后立即继续，不等待、不读其输出（SPEC §4.4）
- [ ] `--refresh` 完整流程：凭证链（无 token 静默退出不写缓存）→ `GET {base_url}/usages`（8s 超时）→ 防御解析 → 成功原子写缓存；**任何失败不写缓存**（旧数据与回拨后 mtime 原样保留 = LKG + 30s fast-retry，SPEC §4.2 步骤 5）；总是 exit 0
- [ ] base_url 覆盖链：env `KIMI_CODE_BASE_URL` > `quota-bar.toml [network] base_url` > 默认 `https://api.kimi.com/coding/v1`；去尾部 `/` 拼 `/usages`（SPEC §5.1；quota-bar.toml 解析本阶段可先用内置默认占位，P3 完整实现）
- [ ] TTL / retry 常量内置默认 60s / 30s（P3 接入 `quota-bar.toml [cache]` 覆盖）
- [ ] 单测：mtime 回拨计算、TTL 过期边界（age == TTL 触发）、缓存损坏当缺失、原子写后内容可 round-trip 解析

### 涉及文件

- 新建：`src/cache.rs`、`src/console.rs`
- 修改：`src/main.rs`（渲染分支接入"读缓存 → 判过期 → 回拨 → 派生"；`--refresh` 分支接入完整取数流程）

### 验收标准

1. `cargo test cache` 全绿（含上述单测清单）
2. 可观察行为（Git Bash，真机；**前提：managed OAuth 已登录**——无 token 时 `--refresh` 静默退出、缓存不含数据，本验收必挂）：删除 `<kimi_home>/cache/quota-status.json` → `echo '{"model":"m"}' | target/release/quota-status.exe` → 立即检查 `ls <kimi_home>/cache/` 出现 `quota-status.json`（空锚定文件）；等待 ≥15s（或每 2s 轮询至 20s，覆盖 8s HTTP 超时 + 进程启动 + TLS 抖动）后 `cat` 该文件含 `fiveHour`/`week` 数据（detached 刷新回填，SPEC §9"缓存缺失"行）
3. 刷新失败保留 LKG：手动把缓存改为含数据的合法 JSON、将 mtime 调旧（`touch -d '2 hours ago'`）、断网 → 渲染一次 → `cat` 缓存内容不变（未被清空/改写）；**等待 >8s（HTTP 超时上限，detached refresh 的存活期）后** `tasklist | grep -i quota-status` 无残留进程（detached 正常退出）
4. `--refresh` 在无 token 环境（临时 `KIMI_CODE_HOME` 指向空目录）静默退出且 `echo $?` 为 0，缓存目录不产生新写入

### 风险与回退

- **风险：多会话并发渲染同时判过期，派生多个 refresh**（竞态风暴）。缓解即设计本身：回拨先于派生，后续渲染 30s 内看到"未过期"（SPEC §4.4）；并发窗口内至多有限次重复派生，refresh 幂等（最后写赢），可接受；回退兜底为 refresh 侧再读一次 mtime 判定（可选优化，非必需）
- **风险：`filetime` 回拨在只读属性/占用文件上失败**。回退：回拨失败时跳过派生本轮（宁可晚一轮刷新也不 panic），stderr 写诊断（宿主丢弃 stderr，SPEC §3.2）
- **风险：Windows `fs::rename` 目标存在时偶发失败**（虽语义带 REPLACE_EXISTING，防病毒扫描占用等）。回退：rename 失败重试一次，仍失败保留旧缓存退出（LKG 语义不破坏）

---

## P3 渲染与字段配置

### 目标

实现 SPEC §7 渲染契约与 §8 配置契约：`quota-bar.toml` 字段开关与顺序、颜色表、额度段格式、thinking 阶梯旁路读取、宽度感知降级阶梯、UTF-8 stdout。本阶段结束时渲染模式完整可用，热路径满足 <10ms 预算。

### 任务清单

- [ ] 新建 `src/config.rs`：解析 `<kimi_home>/quota-bar.toml`（结构：`[render] order`、`[render.quota]` 五键（five_hour/week/month/reset_time/booster）、`[thresholds] green_below/yellow_below`、`[cache] ttl_seconds/retry_seconds`、`[network] base_url/http_timeout_seconds`）；缺失/非法/未知字段一律落回内置默认（默认值 = SPEC §8 示例注释即完整默认），渲染绝不因配置失败；路径随 `KIMI_CODE_HOME`
- [ ] 回接 P2：TTL / retry_seconds / base_url / http_timeout 由 config 提供
- [ ] 新建 `src/render.rs`：
  - [ ] stdin 读到 EOF，`String::from_utf8_lossy` 后解析 JSON；失败按空 payload（SPEC §4.1）
  - [ ] 字段渲染：permissionMode（色表 31/33/32/37 按值）、model（cyan 36）、thinking、额度组（5h/week/month，`{label} {percent:.0}%` + ` (rst HH:MM)` / 跨天 `(rst MM/DD HH:MM)`）、gitBranch（magenta 35）；每段尾部 `\033[0m`
  - [ ] 额度组内 ` · `（灰）、段间 ` | `（灰）拼接；有值才显示；`contextTokens`/`maxContextTokens` 不进本行（SPEC §3.4）；booster 默认不渲染
  - [ ] 颜色阈值：percent < green_below 绿 / < yellow_below 黄 / 否则红（默认 60/85，边界 `>= 85` 红、`>= 60` 黄，单测钉死 59.x/60/84.x/85）
  - [ ] thinking 阶梯：`[thinking] enabled=false` → 灰 `off`；否则 `[thinking] effort`；缺失回退 `[models.*]` 中 `display_name` 或 `model` 匹配 stdin model 的 `overrides.default_effort` 再退 `default_effort`；都无则省略段；字段开关关闭时跳过整段连同 config.toml 读取（省 IO，SPEC §7.1）
  - [ ] 宽度感知与降级（SPEC §7.5）：`src/console.rs` 增加 `CreateFileW("CONOUT$")` + `GetConsoleScreenBufferInfo().dwSize.X` 查宽，headless 按 120；降级只删不重排：丢 reset 后缀 → 丢 gitBranch → 只留额度组 → 原样输出交宿主截断
  - [ ] stdout 强制 UTF-8 + lossy（`U+FFFD`）语义；输出恰一行 + `\n`，exit 0；无字段时输出空行（宿主回退内置布局，SPEC §4.1 步骤 5）
- [ ] 单测（SPEC §10.1）：thinking 阶梯四分支、颜色阈值边界、reset 跨天格式、宽度降级四级、UTF-8 lossy、percent 舍入边界（`%.0f` = round-half-to-even：60.5→60、61.5→62）、order 重排与未知字段忽略

### 涉及文件

- 新建：`src/config.rs`、`src/render.rs`
- 修改：`src/main.rs`（渲染分支调 render；三模式定型）、`src/console.rs`（宽度查询）、`src/cache.rs`（TTL/retry 接 config）
- 只读参考：`repos/kimi-planbar/quota-status.py`（227 行；颜色码/分隔符/reset/thinking 阶梯/UTF-8 的语义来源，SPEC §2.2）

### 验收标准

1. `cargo test render` 与 `cargo test config` 全绿
2. 手工渲染观察（Git Bash）：
   `printf '%s' '{"model":"Kimi","permissionMode":"yolo","gitBranch":"main"}' | target/release/quota-status.exe | cat -v`
   输出恰一行，含 `\033[31m`（yolo 红）、`\033[36m`（model cyan）、`\033[35m`（git magenta）、灰分隔 `\033[90m|\033[0m`，行以换行结束，exit 0
3. 空/非法 stdin：`printf '' | target/release/quota-status.exe; echo $?` → exit 0；`printf 'not-json' | target/release/quota-status.exe` 同样 exit 0 且不 panic
4. quota-bar.toml 关闭字段：临时 `KIMI_CODE_HOME` 下放一份 `order = ["quota"]` 的配置 → 渲染输出只剩额度组相关内容；删配置文件 → 回完整默认
5. 热路径预算抽查：预热后连续 20 次 `echo '<payload>' | target/release/quota-status.exe > /dev/null` 用 bash `time` 循环计时，平均耗时留档（P5 正式验收 <10ms/<50ms）

### 风险与回退

- **风险：中文 Windows console 代码页导致 UTF-8 输出乱码**。缓解：输出字节强制 UTF-8 + lossy（SPEC §7.6），宿主侧本就按 UTF-8 消费 stdout pipe；若真机 footer 出现乱码，检查是否误用了 println! 的本地化路径（应锁定 raw bytes 写 stdout）
- **风险：CONOUT$ 宽度在宿主 pipe 环境下查询失败或值异常**（如 0）。回退：任何查询异常按 120 兜底（SPEC §7.5 已规定）；单测覆盖"宽度不可得"分支
- **风险：config.toml（thinking 段）解析与 quota-bar.toml 解析耦合混淆**。处置：两者独立解析函数——config.toml 只做 thinking/models 段局部解析（与 P1 credentials 的逐行 provider 扫描互不干扰），quota-bar.toml 用 toml crate 完整解析

---

## P4 测试与 golden parity

### 目标

复刻 Go 侧 golden 测试机制（`repos/kimi-planbar-tui/go/internal/core/quota_test.go:206-293`），实现 30 case 逐字节对齐 + month 新 case，并完成 `--test-fetch` 的 headless 自检验证（SPEC §10.2/§10.3）。parity 通过即同时覆盖二进制 `--test-fetch` 输出格式（共用同一序列化函数）。

### 任务清单

- [ ] 新建 `tests/golden.rs`：输入 payload 内联（拷贝自 `quota_test.go:240-266`）→ 注入解析函数 → 固定时钟序列化 → 与 `repos/kimi-planbar-tui/go/testdata/golden/quota-*.txt` 逐字节比对（比较前 CRLF 归一化，同 `goldens_test.go:23-25` 机制，SPEC §10.2）
- [ ] 固定时钟两常量：`atZero` = 1893456000000ms 整、`atFracs` = +123456789ns（quota_test.go:14-15）
- [ ] 序列化与 `--test-fetch` 输出共用同一函数：serde pretty、2 空格缩进、camelCase
- [ ] `quota-error-*` 4 个 case 不走网络，直接构造 error 结果比对（quota_test.go:272-275 同款）
- [ ] month 新 case 本项目 `testdata/golden/` 新增（如 `quota-month-full`、`quota-month-no-limit`），**不回写参考仓库**；既有 30 case 保持 byte-identical（month 用 `skip_serializing_if` 保证无 month 键）
- [ ] `--test-fetch` 完整实现检查：headless（不读 stdin、不写缓存）、错误也输出 JSON 且 exit 0（SPEC §4.3）
- [ ] 真机自检脚本化断言（SPEC §10.3）：managed OAuth 环境 stdout 为合法 JSON、`error == null`、`fiveHour`/`week` 有值、`fetchedAt` 接近当前时间；无凭证环境 `error == "no-token"`；运行前后缓存文件 mtime/内容不变（无副作用）

### 涉及文件

- 新建：`tests/golden.rs`、`testdata/golden/quota-month-full.txt`、`testdata/golden/quota-month-no-limit.txt`
- 修改：`src/quota.rs`（如 parity 暴露序列化差异，调整共用序列化函数）
- 只读参考：`repos/kimi-planbar-tui/go/internal/core/quota_test.go`、`repos/kimi-planbar-tui/go/testdata/golden/`（30 个 `quota-*.txt`）

### 验收标准

1. `cargo test --test golden` 全绿，测试计数 = 既有 30 case + month 新 case + error 4 case（后两者若已含在 30 内则按实际矩阵计，以 `cargo test --test golden -- --list` 输出留档）
2. 抽查逐字节性：任选 1 个 case（如 `quota-mixed_string_number`），临时去掉 CRLF 归一化后在 CRLF 换行的 golden 上应失败、加回归一化后应通过（验证归一化逻辑真实生效）
3. 真机（managed OAuth）：`target/release/quota-status.exe --test-fetch` 三项断言通过（JSON 合法 / `error == null` / `fetchedAt` 距执行时刻 < 5s）；执行前后 `stat` 缓存文件 mtime 不变
4. 无凭证环境（临时 `KIMI_CODE_HOME`）：`--test-fetch` 输出含 `"error": "no-token"`（pretty JSON），exit 0

### 风险与回退

- **风险：serde_json pretty 与 Go `encoding/json` 缩进/转义细节不一致**（Go 默认 HTML 转义 `<>&`、浮点格式化差异如 `21.0` vs `21`）。这是本阶段最大风险。处置：先对既有 30 个 golden 做一次差异扫描（`grep -l '[<>&]'` 于 golden 目录确认是否存在被转义字符）；浮点按 Go 的 JSON number 规则对齐（必要时对 f64 序列化做定制 wrapper），所有对齐逻辑集中在共用序列化函数内，不散落解析代码
- **风险：固定时钟注入需要解析函数接受时钟参数**，与 P1 抽取的函数签名冲突。回退：P1 设计时即让"当前时间"经由参数/闭包注入（生产路径传 `Utc::now`，测试传常量），避免 P4 返工——已在 P1 任务中隐含，此处显式提醒
- **风险：month 新 case 无上游参照，golden 由本项目自证**。处置：新 case 的期望输出由人工按 SPEC §5.3 schema 手写并 code review，不由实现反推（防止自我实现预言）

---

## P5 构建交付与安装文档

### 目标

产出满足 SPEC §10.4 六条真机验收的交付物：体积 ≤5MB 的静态单 exe（撰写时预估 3–5MB）、性能达标、安装文档（tui.toml 配置 + `/reload-tui` + 已知风险提示）。

### 任务清单

- [ ] `cargo build --release` 最终构建，确认静态 CRT、无 runtime 依赖（`dumpbin /dependents target/release/quota-status.exe` 仅系统 dll）
- [ ] 体积验收：`stat -c %s` ≤ 5242880 字节（撰写时预估区间 3145728–5242880 字节；实测 1788928 字节）
- [ ] 性能验收：热路径 <10ms、端到端（含进程启动）<50ms——预热后循环计时取均值（如 bash 循环 100 次 `time` 或 PowerShell `Measure-Command`）
- [ ] 编写 `README.md` 安装文档：放置 exe（如 `C:\tools\quota-status.exe`）、`~/.kimi-code/tui.toml` 配置 `[status_line] command`（TOML 反斜杠转义提醒，SPEC §3.1）、`/reload-tui` 生效步骤、`quota-bar.toml` 完整示例与字段说明、`--test-fetch` 自检用法、已知风险（SPEC §12 三条：偏好保存整文件重写 tui.toml 的说明——当前 main 中 command/items 值 round-trip 保真、旧版本可能整段丢失——排查恢复步骤优先置顶）
- [ ] 真机验收六条逐项执行并留档（SPEC §10.4）：单 exe 体积 / 性能 / footer 显示与活跃会话 1 分钟自动更新 / 断网 LKG 保留且 ≤90s 自愈 / 非 managed OAuth 账号无额度组不报错 / tui.toml 重写场景演练恢复（round-trip 验证；旧版本整段丢失时补回）
- [ ] 清理：移除各阶段遗留的调试代码与临时 stub

### 涉及文件

- 新建：`README.md`
- 修改：`src/main.rs`（清理 stub 与调试输出）
- 产物：`target/release/quota-status.exe`

### 验收标准

1. `stat -c %s target/release/quota-status.exe` ≤ 5242880 字节（v1.2：原 [3145728, 5242880] 区间为撰写时预估，实测 1788928 字节 ≈1.7MB，预算按上限约束执行）
2. 性能：100 次循环渲染计时均值 <50ms/次（含进程启动）；若拆分统计，热路径部分 <10ms（SPEC §10.4 条 2）
3. 可观察行为：配置 `tui.toml [status_line] command` 并 `/reload-tui` 后，footer 第 1 行显示额度行；**活跃会话**（保持输入/流式或活动 goal）中 ≤1 分钟内缓存文件 mtime/`fetchedAt` 更新（或额度百分比发生变化）即判刷新机制生效；纯空闲会话宿主不调用 command、刷新不触发（事件驱动，SPEC §3.3），恢复活动后数秒内随一次渲染自愈——以「活跃 1 分钟内 mtime 更新」+「空闲→活跃自愈」两个观察点联合判定（SPEC §10.4 条 3）
4. 可观察行为：断网（或临时把 `base_url` 指向不可达地址）→ 旧额度数据保留显示不清空；恢复网络后 ≤90s 内数据自愈更新（SPEC §10.4 条 4）
5. 可观察行为：`/theme` 切换一次后检查额度行——当前 main 中 command 值应 round-trip 存活、额度行不消失（丢失的仅段内注释，不影响功能）；若使用旧版本遇整段丢失 → 按 README 排查步骤补回 `[status_line]` 并 `/reload-tui` 恢复（SPEC §10.4 条 6、§12 风险 ①）
6. `README.md` 存在且包含：tui.toml 配置片段、`/reload-tui` 步骤、quota-bar.toml 完整示例、`--test-fetch` 用法、风险 ① 排查恢复章节

### 风险与回退

- **风险：体积超 5MB**。回退阶梯（SPEC §11）：先换 HTTP/TLS 后端或裁 feature（如 rustls 精简 cipher suite）→ 再评估 `opt-level = "z"` → 仍超则回报决策放宽预算（需用户确认，属 SPEC 变更）
- **风险：端到端耗时偶发超 50ms**（Windows 进程启动抖动、防病毒扫描首启）。缓解：预热后测量取均值；确认无重 IO（渲染只读 3 个小文件 + 1 次宽度查询，SPEC §4.1）；防病毒排除项写入 README 建议
- **风险：真机验收 3/4 依赖真实账号与网络**，环境不可控。处置：验收记录注明执行日期与环境；断网自愈用 `base_url` 指向不可达地址等价模拟，恢复即改回

---

## 里程碑依赖关系（文字版）

严格串行主链：**P0 → P1 → P2 → P3 → P4 → P5**。

- P0 是全部阶段的地基（crate、profile、依赖、入口骨架）；依赖版本与 feature 在此锁定，后续阶段不新增 SPEC §11 白名单之外的依赖。
- P1 依赖 P0；产出纯逻辑层（凭证/HTTP/解析），是 P2 取数流程与 P4 golden parity 的共同前置。P1 的固定时钟注入设计直接决定 P4 能否无返工复刻（见 P4 风险 2）。
- P2 依赖 P1（`--refresh` 消费凭证链与解析结果）；产出的缓存 schema 与 TTL/回拨机制是 P3 渲染的数据源。
- P3 依赖 P2（渲染读缓存、判过期走 P2 的回拨派生路径）；产出完整渲染模式。P3 结束即工具功能完整，P4/P5 是质量与交付 gate。
- P4 依赖 P1（解析函数与时钟注入）+ P3（`--test-fetch` 与渲染共用序列化/模型定型）；parity 不通过时可能回改 `src/quota.rs` 序列化（不动解析语义）。
- P5 依赖 P0–P4 全部：只在全部测试绿的基础上做最终构建、性能、真机验收与文档。

允许的并行点：P2 的 cache 模块与 P3 的 config 模块纯代码部分可并行编写，但集成验证仍按 P2 → P3 顺序（渲染依赖缓存行为就绪）。

## 整体工作量估算

口径：现有约 720 行可复用 + 新写 300–500 行（本 PLAN 撰写时实测锚点：`repos/kimi-planbar-tui/rust/src/` 四文件 `wc -l` = credentials 147 + http 16 + quota 374 + polling 74 = 611 行直接抽取；`repos/kimi-planbar/quota-status.py` 227 行为渲染语义参考非逐行移植；合计参考资产 838 行，约 720 行为其中实际复用部分）。

| 阶段 | 主要工作 | 估算 |
|---|---|---|
| P0 | 骨架 + 依赖调通（体积风险试探） | 0.5 人日 |
| P1 | 611 行抽取 + 两处升级 + month 扩展 + 单测移植 | 1.5–2 人日 |
| P2 | cache/console 新写（~150–200 行）+ 进程模型集成 | 1–1.5 人日 |
| P3 | render/config 新写（~200–300 行）+ 单测 | 1.5–2 人日 |
| P4 | golden 机制复刻 + 序列化对齐 + month 新 case | 1–1.5 人日 |
| P5 | 构建/性能/真机验收 + README | 0.5–1 人日 |
| 合计 | 新写代码 300–500 行（render + config + cache 三模块 + main 串联） | **约 6–8.5 人日** |

最大不确定项：P4 的 serde_json 与 Go `encoding/json` 逐字节对齐（浮点/转义差异），最坏 +1 人日。

## 实现时禁止事项

1. **不得**实现 hooks 数据层、plugin/MCP 集成，**不得** fork 官方 Kimi Code 源码（SPEC §1.2）。
2. **不得**自动刷新 OAuth token——token 新鲜度依赖运行中的 CLI，本工具绝不自行刷新（SPEC §1.2/§5.2；凭证链只读不写）。
3. **首版仅 Windows x64**（`x86_64-pc-windows-msvc` + crt-static），不做跨平台抽象（SPEC §1.2/§11）。
4. **不做交互式 TUI**；booster 钱包余额默认不显示（解析仍入库，供配置扩展）（SPEC §1.2/§7.1）。
5. **不引入 tokio/async**——取数是一次性阻塞调用，渲染派生后即退出（SPEC §11）。
6. **渲染路径禁网络、禁重 IO**：只允许读 3 个小文件 + 一次 console 宽度查询；热路径 <10ms、端到端 <50ms（SPEC §4.1/§11）。
7. **不回写 `repos/` 下任何参考仓库**（只读参考）；month 新 golden 只放本项目 `testdata/`（SPEC §10.2）。
8. **任何失败保留 LKG**：刷新失败不写缓存、渲染不清空已有额度数据（SPEC §5.3/§9）；渲染模式任何情况下 exit 0，stdout 首行为空时即宿主回退内置布局——不输出报错文本占行（SPEC §3.2/§4.1）。
9. `contextTokens` / `maxContextTokens` **不进渲染行**（宿主 footer 第 2 行原生已显示，SPEC §3.4）。
10. SPEC §1.2 Non-goals 全部继承，后续任何阶段均不得以"顺手实现"为由扩边；扩边需求一律先修订 SPEC 再改 PLAN。
