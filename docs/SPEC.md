# quota-status 契约文档（SPEC）

- 项目：quota-status —— Rust 实现的 Kimi Code CLI statusline 额度显示器（单二进制 `quota-status.exe`）
- 版本：v1.3.2（2026-10-01；§11 补录 CI/Release 工程化设施与 CHANGELOG 约定）
- 版本历史：v1.3.1（2026-10-01；golden 测试数据内化本仓库 `testdata/golden/`，CI 自足，§10.2 同步）；v1.3（2026-10-01；第三轮独立 code review 后修订：§4.1 新鲜判定钉死为纯 mtime 语义、§5.1 错误归类按 send/body 阶段细分、§9 缓存损坏行同步表述）；v1.2（2026-10-01；P0–P5 实现落定后修订）；v1.1（2026-10-01；按第二轮复核 REVIEW.md 的 8 项发现修订 F1–F8）；v1.0
- 本文档与已定决策清单冲突时，以决策清单为准（本文已按决策清单如实收录；两处事实勘误见 §2.3）
- 路径约定：所有 `path:line` 引用相对本仓库根（即本文件所在 `docs/` 目录的上一级）

---

## 1. 概述与目标

### 1.1 问题与形态

Kimi Code CLI 的 footer 支持用户自定义 statusline command（`tui.toml [status_line] command`）：宿主以 JSON 快照喂 stdin，取 stdout 第一行替换 footer 第 1 行，但整个子进程被限制在 **300ms** 内完成（`repos/kimi-code/apps/kimi-code/src/tui/utils/status-line-command.ts:13`）。额度数据来自远端 `GET /usages`，网络往返不可能稳定压进 300ms，因此：

**quota-status 是一个静态单二进制，三种运行模式协作**：渲染模式只读本地缓存（毫秒级完成），缓存过期时派生 detached 子进程执行取数模式回填缓存，父进程绝不等待网络。由此同时满足用户三条需求：

1. **额度数据 1 分钟刷新** —— 缓存 TTL 60s + detached 后台取数；
2. **statusline 字段可自定义** —— `~/.kimi-code/quota-bar.toml` 控制字段开关与行内顺序；
3. **轻量、性能优秀** —— 单 exe、无 runtime 依赖、渲染路径禁网络禁重 IO（预算 <10ms）、目标体积 ≤5MB（v1.2：原"3–5MB"为预估区间，实测 ≈1.7MB）。

### 1.2 Non-goals（明确不做）

- hooks 数据层、plugin/MCP 集成
- fork 官方 Kimi Code 源码
- 交互式 TUI
- booster 钱包余额默认显示（解析仍入库，供配置扩展）
- 自动刷新 OAuth token（token 新鲜度依赖运行中的 CLI，本工具绝不自行刷新）
- 跨平台（首版仅 Windows x64）

---

## 2. 术语与参考仓库

### 2.1 术语

| 术语 | 含义 |
|---|---|
| 宿主 / host | Kimi Code CLI TUI，即调用 statusline command 的进程 |
| 渲染模式 | quota-status 默认行为：读 stdin 快照 + 本地缓存 → 拼一行 ANSI 文本 |
| 取数模式 | `--refresh`：凭证链 → GET /usages → 解析 → 原子写缓存 |
| 自检模式 | `--test-fetch`：完整取数一次，JSON 打印解析结果后退出（headless） |
| 额度组 | 5h / week / month 三段百分比构成的组合段 |
| LKG | last-known-good：最后一次成功取数的缓存数据 |

### 2.2 参考仓库与复用资产（实现时直接抽取）

| 资产 | 路径 | 行数（已核实） | 抽取方式 |
|---|---|---|---|
| 凭证链 | `repos/kimi-planbar-tui/rust/src/credentials.rs` | 147 | 原样抽取（含单测，credentials.rs:115-147） |
| HTTP client 语义 | `repos/kimi-planbar-tui/rust/src/http.rs` | 16 | 抽取"OnceLock 共享 client + 构建失败降级 None"语义（http.rs:11-16）；**超时 10s 改为 8s**（决策清单定 8s） |
| 额度解析 | `repos/kimi-planbar-tui/rust/src/quota.rs` | 374 | 抽取解析与防御规则；**5h 段匹配升级**为 window 条件匹配（见 §6.4）、**新增 month 段**（见 §6.6） |
| 重试规则 | `repos/kimi-planbar-tui/rust/src/polling.rs` | 74 | 抽取"失败保留 LKG + 30s fast-retry"语义（polling.rs:20-41）；定时循环部分不移植（本工具无驻留进程，由 mtime 回拨实现等价重试，见 §5.3） |
| golden 测试矩阵 | `repos/kimi-planbar-tui/go/testdata/golden/quota-*.txt` | 30 case | parity 对齐（见 §10.2）；输入 payload 内联在 `repos/kimi-planbar-tui/go/internal/core/quota_test.go:240-266` |
| API 契约参考 | `repos/kimi-planbar-tui/docs/SPEC.md` §16（694 行文档的 406-465 行） | — | §16.1-16.5 即本 SPEC §5/§6 的上游契约 |
| 渲染段落参考 | `repos/kimi-planbar/quota-status.py` | 227 | 颜色码、分隔符、reset 格式、thinking 阶梯、UTF-8 处理的语义来源（Python 原型） |

新写代码仅三块：**渲染模块 + 字段配置模块 + 缓存模块**，预计 300–500 行。

### 2.3 决策清单事实勘误（如实收录）

1. 决策清单写 golden 矩阵位于 `repos/kimi-planbar/go/testdata/golden/quota-*.txt`——该路径**不存在**（已核实：`ls` 返回 No such file or directory）。实际位置为 `repos/kimi-planbar-tui/go/testdata/golden/`（目录 37 个文件，其中 `quota-*.txt` 恰 30 个；`ts/test/golden/`、`ts-nodejs/test/golden/` 各有一份同内容拷贝）。本文档一律按实际路径引用。
2. 决策清单说宿主契约"1s 节流"：准确语义是**同一 command 的执行间隔至少 1s**（`status-line-command.ts:14` 的 `STATUS_LINE_RERUN_INTERVAL_MS = 1_000`，节流判断在 status-line-command.ts:139-149），期间触发的更新延迟落地、不丢弃。

---

## 3. 宿主契约：Kimi Code CLI statusline

> 本章全部论断已对照官方源码核实。

### 3.1 启用方式

`~/.kimi-code/tui.toml`：

```toml
[status_line]
command = "C:\\tools\\quota-status.exe"
```

- 宿主读取 `status_line.command`，trim 后非空才启用（`repos/kimi-code/apps/kimi-code/src/tui/config.ts:238`，空串归 null，config.ts:287-290）。
- 生效方式：TUI 内执行 `/reload-tui`（命令注册于 `repos/kimi-code/apps/kimi-code/src/tui/commands/registry.ts:304`，分发于 dispatch.ts:512-513）或重启 CLI。
- TOML 字符串中 Windows 路径反斜杠须转义；宿主回写 tui.toml 时用 `escapeTomlBasicString` 做同样转义（config.ts:311）。

### 3.2 进程调用契约

- Windows 下宿主经 `cmd.exe /d /s /c <command>` 启动（`status-line-command.ts:46`，取 `%ComSpec%` 兜底 `cmd.exe`）；POSIX 经 `sh -c`。command 是完整命令字符串，exe 用绝对路径。
- `stdio: ['pipe', 'pipe', 'ignore']` —— **stderr 被宿主丢弃**（status-line-command.ts:47），诊断输出可写 stderr 不影响渲染。
- 注入 env `KIMI_CODE_STATUS_LINE=1`（status-line-command.ts:48），可用于识别调用上下文，但不得依赖它存在。
- stdin：宿主写入 payload 的 JSON 序列化后关闭（`child.stdin?.end(JSON.stringify(payload))`，status-line-command.ts:113）。渲染模式必须读到 EOF 再解析。
- stdout：**只取第一行**（`stdout.split('\n')[0].trimEnd()`，status-line-command.ts:106）；后续行忽略。首行为空 → 视为失败（同文件 :107）。
- 退出码：非 0 → 视为失败（status-line-command.ts:100-104）。渲染模式任何情况下 exit 0。
- 失败（spawn 错误 / 非零退出 / 超时 / 空输出）统一返回 null，宿主回退内置 footer 布局（status-line-command.ts:4-8 注释、footer.ts:316-319）。

### 3.3 超时、节流与捕获上限

| 项 | 值 | 证据 |
|---|---|---|
| 单次执行超时 | 300ms，超时后 Windows 上 `taskkill /pid <pid> /T /F` 杀整棵进程树 | status-line-command.ts:13, 58-78 |
| 重跑节流 | ≥1s 间隔；in-flight 期间的新 payload 记为 pending，间隔到期后落地 | status-line-command.ts:14, 139-149 |
| stdout 捕获上限 | 64KB（65536 字节）；只累积到首个换行为止 | status-line-command.ts:15, 84-95 |
| 结果缓存 | runner 缓存 last-good 行；**失败不清空缓存**，继续显示上一条成功输出 | status-line-command.ts:174-179 |

推论：调用是**事件驱动**的——仅当 footer 重渲染时才 spawn（活跃会话中至多 1 次/秒，受节流上限约束；空闲会话——无输入、无流式、无活动 goal——完全不调用：唯一触发点 footer.ts:312，唯一周期器为 goal 活动时的 syncGoalTimer footer.ts:529-537）。因此额度行更新语义为「活跃会话内 ≤1s + TTL 到期后的下一次渲染」，空闲期间数据保持不变（§10.4 条 3）；我们的输出行（最坏 ~200 字符）远小于 64KB 上限。

### 3.4 footer 两行行为

- **第 1 行**：statusline command 的 stdout 首行**整体接管**（footer.ts:311-318），宿主再包一层主题前景色 `chalk.hex(colors.text)(customLine)`（footer.ts:318）——我们输出的内嵌 ANSI SGR 序列依旧按序生效，不受影响。
- **第 2 行**：原生渲染，不可定制。command 接管第 1 行时：左侧为 ctrl+o 提示（transient/warning 提示出现时优先占据左侧，footer.ts:374-384、386-394），右侧为 `context: N% (缩写tokens/缩写max)`——tokens 按 1024 进制缩写（footer.ts:177-183 `formatContextStatus` + `formatTokenCount`）；原生布局（无 command）时左侧为空。
- 因此 `contextTokens` / `maxContextTokens` **明确不进本工具渲染行**——footer 第 2 行原生已显示。
- 两行最终都经 `truncateToWidth` 截到终端宽（footer.ts:397）——这是宿主侧兜底截断；我们应在输出前自行降级（§7.5），避免被宿主硬截。

### 3.5 stdin payload schema（`StatusLinePayload`，status-line-command.ts:17-28）

| 字段 | 类型 | 本工具用途 |
|---|---|---|
| `model` | string（displayName，构造处 footer.ts:509） | 渲染 model 段；thinking 阶梯回退匹配用 |
| `cwd` | string | 不用（决策：字段清单无 cwd） |
| `gitBranch` | string \| null | 渲染 gitBranch 段 |
| `permissionMode` | string | 渲染 permissionMode 段 |
| `planMode` | boolean | 不用 |
| `contextUsage` | number | 不用（§3.4） |
| `contextTokens` / `maxContextTokens` | number | 不用（§3.4） |
| `sessionId` | string | 不用 |
| `version` | string | 不用 |

---

## 4. 运行模式与进程模型

```
宿主(1s节流) ──spawn──> cmd.exe ──> quota-status.exe（渲染模式, <300ms）
                                          │ 读 stdin 快照 / 缓存 / quota-bar.toml / config.toml[thinking]
                                          │ 缓存 age ≥ TTL → 回拨 mtime + 派生 detached:
                                          └──spawn(DETACHED|NO_WINDOW, stdio DEVNULL)──> quota-status.exe --refresh
                                                                                              │ 凭证链 → GET /usages（8s 超时）
                                                                                              │ 解析 → 原子写缓存 → 退出
```

### 4.1 渲染模式（默认，无参数）

1. 读 stdin 到 EOF，`String::from_utf8_lossy` 后解析 JSON（解析失败按空 payload 处理，语义同 quota-status.py:177-181）。
2. 读缓存 `~/.kimi-code/cache/quota-status.json`（§5.3）。
3. 判定缓存年龄（文件 mtime）：`age ≥ TTL` → 先做 mtime 回拨，再派生 detached `--refresh` 子进程，**不等待、不读取其任何输出**。新鲜判定**只看 mtime**，与缓存内容是否可解析无关（v1.3 钉死：空锚定/损坏/refresh 持续失败时，回拨后的 mtime 同样压住派生，retry 秒后自动重试——quota-status.py:135-149 `maybe_refresh` 同款纯 age 语义）。
4. 按 `quota-bar.toml` 的字段开关与顺序拼一行带 ANSI 颜色的文本（§7），写入 stdout（UTF-8）+ 换行，exit 0。
5. 无任何可渲染字段时输出空行——宿主拿到空首行会回退内置布局（§3.2），这是期望行为。
6. 渲染路径禁网络、禁重 IO：只允许读 3 个小文件（缓存 JSON、`config.toml` 的 thinking 相关段、`quota-bar.toml`）+ 一次 console 宽度查询；热路径预算 **<10ms**，进程端到端（含启动）预算 <50ms。

### 4.2 取数模式 `--refresh`

1. 凭证链取 token（§5.2）；无 token → 直接退出（不写缓存）。
2. `GET {base_url}/usages`（HTTP 超时 8s）。
3. 防御式解析（§6）。
4. 成功：`mkdir -p` 缓存目录，写 `quota-status.json.<pid>.tmp` 后 `fs::rename` 原子替换（v1.2：tmp 带 PID 后缀，消除并发 refresh 交错写同一 tmp 的竞态窗口；Windows 下 std rename 带 `MOVEFILE_REPLACE_EXISTING`，等价 `os.replace`，语义同 quota-status.py:127-131）。写入的是**解析后的结构化 JSON**，不是渲染串。
5. 失败（网络/非 2xx/解析失败）：**不写缓存**——旧数据与回拨后的 mtime 原样保留，30s 后渲染模式自然再触发（fast-retry，见 §5.3）。
6. `--refresh` 总是 exit 0（无消费者读取其退出码；失败语义由"缓存未更新"表达）。

### 4.3 自检模式 `--test-fetch`

- 执行一次与 `--refresh` 完全相同的取数+解析，把解析结果以 pretty JSON（2 空格缩进，camelCase，§5.3 schema）打印到 stdout 后退出。
- headless：不读 stdin、**不写缓存**（保证自动化测试无副作用、可重复）。
- 错误也输出 JSON（`error` 字段为错误类型名，§6.10），exit 0。
- 用途：真机端到端自检 + parity 测试的输出格式基准（§10.2/§10.3）。

### 4.4 detached 刷新与防风暴

- 派生子进程：`Command::new(<自身绝对路径>).arg("--refresh")`，`creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)`（0x08000000 | 0x00000008），stdio 三路 DEVNULL。语义对应 Python 原型的 `start_new_session` + DEVNULL（quota-status.py:146-149）。
- 父进程 spawn 后立即继续渲染并退出；刷新任务独立存活。渲染进程远早于 300ms 超时退出，宿主的 `taskkill /T` 不会波及 detached 子进程。
- **防刷新风暴**：渲染模式在派生前把缓存 mtime 回拨为 `now - TTL + 30s`（quota-status.py:143-145 同款语义）——后续渲染在 30s 内看到"未过期"，不再派生；若刷新失败（缓存未被改写），30s 后自动重试。多会话并发渲染读到同一回拨后 mtime，同样只派生有限次。

---

## 5. 数据层契约

### 5.1 端点

- 默认：`GET https://api.kimi.com/coding/v1/usages`；国际站 `https://api.kimi.ai/coding/v1`（双站出处：官方 oauth 包 managed-usage.ts:8-9 `DEFAULT_KIMI_CODE_BASE_URL` / `GLOBAL_KIMI_CODE_BASE_URL`；env 覆盖见 :18）。
- 覆盖优先级：env `KIMI_CODE_BASE_URL` > `quota-bar.toml [network] base_url` > 默认值。base 值须含 version 段（如 `.../coding/v1`），工具去掉尾部 `/` 后拼 `/usages`。
- Header：`Authorization: Bearer <token>`、`Accept: application/json`（quota.rs:85-86、kimi-planbar-tui SPEC §16.1）。
- HTTP 超时 **8s**（决策清单；注意被抽取的 http.rs 原值 10s，http.rs:14，实现时改）。错误分类：超时 → `TaskCanceledException`，其他传输错误 → `HttpRequestException`（quota.rs:92-98）。
- 错误归类按阶段细分（v1.3 钉死，消除"其余传输错误"的表述歧义）：send 阶段超时 → `TaskCanceledException`、非 2xx 与其余传输错误 → `HttpRequestException`；body 阶段（headers 已回）超时 → `TaskCanceledException`，body 阶段其余错误（连接 reset、非法 UTF-8 等）→ `JsonException`（body 非超时归类与参考实现 reqwest `.json()` 语义一致，repos/kimi-planbar-tui/rust/src/quota.rs:103-106；body 超时按本工具既定规则归 `TaskCanceledException`，属与参考实现的有意偏差）。
- 仅 managed OAuth（Kimi For Coding）账号有此端点；纯 API key 账号返回 404（官方提示逐字 "Usage endpoint not available. Try Kimi For Coding."：managed-usage.ts:240-241）。非 2xx 一律归入 `HttpRequestException` 失败路径（quota.rs:100-102）。

### 5.2 凭证链（`load_token`，抽取自 credentials.rs:44-66）

`<kimi_home>` = `%USERPROFILE%/.kimi-code`，可被 env `KIMI_CODE_HOME` 覆盖（credentials.rs:35-42）：

1. **`<kimi_home>/credentials/kimi-code.json`** 的 `access_token`（string）；校验 `expires_at`（Unix 秒，number-or-string）> now + 30s，过期视为无效继续下一步（credentials.rs:49-60）。token 刷新依赖运行中的 CLI，**本工具绝不自行刷新**。
2. **兜底 `<kimi_home>/config.toml`**：逐行解析（非完整 TOML parser），找节名以 `providers.` 开头、`base_url` 包含 `api.kimi.com/coding`、`api_key` 非空的 provider，返回其 `api_key`；遇新节先结算上一节（credentials.rs:62-113）。v1.2 落定：匹配串已扩展为同时含 `api.kimi.ai/coding`（国际站纯 API key 用户同样走此兜底；原参考实现仅含 `api.kimi.com/coding`）；解析为手写键值抽取而非 regex（regex 不在 §11 依赖白名单），语义由移植单测钉死。
3. 两者皆无 → 无 token：`--refresh` 静默退出，`--test-fetch` 输出 `error = "no-token"`，渲染模式额度组整体省略。

### 5.3 缓存

- 路径：`<kimi_home>/cache/quota-status.json`（随 `KIMI_CODE_HOME` 联动）。
- 内容：解析后的结构化 JSON（camelCase），schema 在 kimi-planbar-tui `QuotaResult`（quota.rs:40-48）之上**扩展 `month` 字段**：

```jsonc
{
  "fiveHour": { "percent": 21.0, "resetAt": "2030-01-01T00:00:00+08:00" },  // Option
  "week":     { "percent": 18.5, "resetAt": "2030-01-08T00:00:00+08:00" },  // Option
  "month":    { "percent": 43.0, "resetAt": "2030-10-31T00:00:00+08:00" },  // Option，本工具新增；None 时跳过序列化
  "extra":    { "state": "Ready", "balanceCents": 1235, "monthlyEnabled": true,
                "monthlyUsedCents": 4567, "monthlyLimitCents": 10000 },       // Option，booster 解析入库但默认不渲染
  "fetchedAt": "2030-01-01T08:00:00.123456789+08:00",
  "error": null
}
```

- TTL 默认 **60s**（`quota-bar.toml [cache] ttl_seconds` 可覆盖；决策清单定 60s，Python 原型的 300s 不采用）。
- 原子写：同目录带 PID 后缀的 `.tmp` + rename（§4.2 步骤 4）。
- 过期判定与 mtime 回拨：见 §4.4；回拨写 mtime 用 `filetime` crate（Windows `SetFileTime`）。
- 首跑/缓存缺失：渲染模式创建空缓存文件以锚定 mtime（quota-status.py:137-139 同款），再回拨+派生；额度组暂不显示。
- **任何失败保留 LKG**：刷新失败不写缓存（§4.2 步骤 5），渲染永远不清空已有额度数据——语义等价于 kimi-planbar-tui 的 `fill_missing_from`（quota.rs:63-73、polling.rs:22-27、SPEC §16.5 步骤 2）。

---

## 6. 解析防御规则（逐条收录，沿用 kimi-planbar-tui 踩坑契约）

> 上游契约：`repos/kimi-planbar-tui/docs/SPEC.md` §16.3（429-441 行）；参考实现 quota.rs（含单测 quota.rs:243-374）。

1. **数字一律按字符串建模，兼容真数字兜底**：`used`/`limit`/`amountLeft`/`priceInCents`/`expires_at` 等服务端数字字段按 string 解析，遇到真 JSON number 也能解析出同一结果（`get_f64`/`get_i64`，quota.rs:135-150；golden `quota-mixed_string_number`）。
2. **limit ≤ 0 钳位为 1 防除零**：`percent = used/limit*100` 计算前钳位（quota.rs:180-183；golden `quota-div_zero`、`quota-neg_limit`）。
3. **NaN/inf/敌意整数归零**：敌意字符串（`"1e999"`、`"NaN"`）产生的非有限 percent 归 0，序列化永不输出非有限 double（quota.rs:188-193）；`i64` 敌意值（如字符串 `"-9223372036854775808"` 即 `i64::MIN`）必须可解析——该用例是 monthly `priceInCents`，经 `parse_cents` 直存、不经换算（quota.rs:359-373 单测）；balance 换算处的 `saturating_add` 防的是 amountLeft 近 `i64::MAX` 的**正溢出**（quota.rs:229-230 注释逐字 "a pathological amountLeft near i64::MAX must not overflow"；golden `quota-amount_huge`）。
4. **5h 段优先 window 匹配**：在 `root.limits[]` 中优先取 `window.duration == 300` 且 `window.timeUnit == "TIME_UNIT_MINUTE"` 的元素，找不到回落 `limits[0]`；段的 `detail` 进 `parse_segment`（quota-status.py:78-82）。注意被抽取的 quota.rs:115-123 目前只取 `limits[0]`，实现时以本条 window 匹配语义为准（决策清单）。
5. **周段取顶层 `root.usage`**（对象才解析；quota.rs:124-129）。
6. **月段取 `root.totalQuota`，有 limit 才产生段**：`totalQuota.limit` 缺失、为 0 或非有限值（NaN——`NaN != 0.0` 恒真，须显式排除，v1.2 补录）时不渲染 month 段（quota-status.py:93-97）。此段为本工具新增，quota.rs 与既有 golden 无 month 字段。
7. **resetTime 宽松解析**：RFC3339 优先；失败后按宽松阶梯（quota.rs:158-174）——带偏移变体 4 个（空格或 `T` 分隔 + `%:z`/`%z` 偏移），其中**仅 `%H:%M:%S%.f %:z`（空格分隔+偏移）支持小数秒**；naive 变体 2 个（`%Y-%m-%d %H:%M:%S`、`%Y-%m-%dT%H:%M:%S`）不支持小数秒；无 offset 按本地时区（golden `reset_time.txt`、`quota-reset_fraction`）。
8. **boosterWallet 防御**：非对象或缺失 → `NotActivated`；`isEnabled === false` → `NotActivated`——**此时 `amountLeft` 是"月度上限-已用"的估算值而非真实余额，不可当真实余额**（quota.rs:204-220；golden `quota-disabled_wallet`）。仅严格布尔 `false` 触发上述防御；`isEnabled` 为非布尔值（字符串 `"false"`、数字 0 等）不触发防御，按启用路径继续解析 balance（quota.rs:218 `as_bool` 仅识别严格布尔；字符串 "false" 用例 quota_test.go:137-141，数字 0 用例 golden `quota-isenabled_zero` 内联 payload（quota_test.go:255）；两 golden 均 state=Ready）。
9. **余额单位换算**：`balance.amountLeft` 单位为 1e-8 元，`cents = (raw + 500000) / 1_000_000`（四舍五入；quota.rs:222-233；golden `quota-amount_frac_number`、`quota-amount_negative_round*`）。
10. **错误类型名沿用 .NET 风格**（跨版本 diff `--test-fetch` 输出用）：`"no-token"` / `"HttpRequestException"` / `"TaskCanceledException"`（超时）/ `"JsonException"`（quota.rs:77-105、kimi-planbar-tui SPEC §16.4）。

---

## 7. 渲染契约

### 7.1 字段与顺序（用户选定；开关与顺序由 quota-bar.toml 控制）

默认行内顺序：**permissionMode → model → thinking 级别 → 额度组(5h/week/month) → gitBranch**。每段"有值才显示"；段与段之间用灰色 `|` 分隔（§7.4）。

| 段 | 数据源 | 有值条件 |
|---|---|---|
| permissionMode | stdin `permissionMode` | 非空 string |
| model | stdin `model` | 非空 string（schema 已保证 string，status-line-command.ts:18；非 string 忽略） |
| thinking | 旁路读 `<kimi_home>/config.toml` | 见 §7.2 |
| 额度组 | 缓存 fiveHour/week/month | 对应段存在且已开启 |
| gitBranch | stdin `gitBranch` | 非 null 非空 |

thinking 阶梯（不在 stdin 快照里；语义同 quota-status.py:101-119）：

1. `[thinking] enabled = false` → 渲染灰色 `off`；
2. 否则取 `[thinking] effort`；
3. 缺失则回退：在 `[models.*]` 中找 `display_name` 或 `model` 等于当前 stdin `model` 的条目，取 `overrides.default_effort`，再退 `default_effort`；
4. 都取不到 → 该段省略。字段开关关闭时跳过整段（也跳过 config.toml 读取，省 IO）。

`contextTokens`/`maxContextTokens` 不进本行（§3.4）。booster 钱包默认不显示（解析仍入库，供配置扩展）；v1.2 落定：`[render.quota] booster = true` 且 state=Ready 且 balanceCents 可用时，以 cyan `boost <余额元>`（纯 ASCII 两位小数，避免 ¥ 在非 UTF-8 终端的兼容性问题）附加在额度组内。

### 7.2 颜色码（basic SGR，宿主 chalk 包装不影响）

| 元素 | 颜色 | ANSI |
|---|---|---|
| permissionMode：`yolo` | 红 | `31` |
| permissionMode：`auto` | 黄 | `33` |
| permissionMode：`manual` | 绿 | `32` |
| permissionMode：未知值 | 白 | `37` |
| model | cyan | `36` |
| thinking effort | cyan | `36` |
| thinking off | 亮黑（灰） | `90` |
| gitBranch | magenta | `35` |
| 额度段 percent < 60 | 绿 | `32` |
| 额度段 60 ≤ percent < 85 | 黄 | `33` |
| 额度段 percent ≥ 85 | 红 | `31` |
| 段间分隔符 `|`、额度组内分隔符 `·` | 灰 | `90` |

依据：quota-status.py:21（阈值 85/60）、:195（permissionMode 色表）、:202（model cyan）、:111/:119（thinking 色）、:222（git magenta）。阈值边界：60 起黄、85 起红（`>= 85`、`>= 60`，quota-status.py:21）。每段以 `\033[0m` 结束重置。permissionMode 显示原值字符串。

### 7.3 额度段与 reset 时间格式

- 段内格式：`{label} {percent:.0}%{reset}`，label 为 `5h` / `week` / `month`；percent 按 `%.0f` 语义取整（**round-half-to-even**：60.5→60、61.5→62，非四舍五入；边界由单测钉死）。
- reset 后缀：同一颜色 span 内追加 ` (rst HH:MM)`；重置时间跨天时 `(rst MM/DD HH:MM)`（quota-status.py:27-31；`reset_time` 来自缓存 `resetAt`，本地时区）。
- 额度组内三段用 ` · `（灰点）连接：`" \033[90m·\033[0m "`（quota-status.py:98）。

### 7.4 行拼接

- 段间：`" \033[90m|\033[0m "`（空格 + 灰色竖线 + 空格，quota-status.py:224）。
- 整行 = 存活段按配置顺序 join；输出一行 + `\n`，exit 0。
- 可见宽度计算排除 ANSI 转义序列；宽度按字符数近似可接受（字段以 ASCII 为主），最终由宿主 `truncateToWidth` 兜底（footer.ts:397）。

### 7.5 宽度感知与降级阶梯（脚本内置，先于宿主截断生效）

- 宽度来源：payload 无终端宽度字段（§3.5），渲染进程虽 stdout 为 pipe 但仍附着宿主 console——用 `CreateFileW("CONOUT$")` + `GetConsoleScreenBufferInfo().dwSize.X` 查询；查不到（headless）按 **120** 兜底。
- 降级顺序（只删不重排，逐级尝试直到可容纳）：
  1. 丢所有 reset 时间后缀；
  2. 丢 gitBranch 段；
  3. 只保留额度组；
  4. 仍超宽则原样输出，交给宿主截断（footer.ts:397）。

### 7.6 stdout UTF-8 与输出约束

- 输出字节强制 UTF-8；来自外部的字符串（路径、model 名等）先做 lossy 替换（`U+FFFD`），对应 Python 原型 `sys.stdout.reconfigure(encoding='utf-8', errors='replace')`（quota-status.py:171-175）——防中文 Windows GBK 管道乱码与 lone surrogate 崩溃。stdin 同样按 lossy UTF-8 读（§4.1）。
- 输出恰一行（首行被宿主取用，§3.2），总长（含 ANSI）须 < 64KB（§3.3，实际远低于）。

---

## 8. 配置文件 `~/.kimi-code/quota-bar.toml`

无文件时用内置默认（下例即完整默认值）；解析失败的键忽略并落回默认，渲染绝不因配置问题失败。未知字段忽略。路径随 `KIMI_CODE_HOME` 联动。

```toml
# quota-status 配置（可选；缺失时全部取以下默认值）

[render]
# 行内字段顺序（可删减、可重排；未列出的字段不显示）
order = ["permission_mode", "model", "thinking", "quota", "git_branch"]

[render.quota]
# 额度组内子段开关
five_hour  = true
week       = true
month      = true
reset_time = true   # false 等价于降级第 1 级（永久丢 reset 后缀）
booster    = false  # 默认不显示 booster 钱包（数据仍解析入缓存）

[thresholds]
# 百分比颜色阈值：percent < green_below → 绿；< yellow_below → 黄；否则红
green_below  = 60
yellow_below = 85

[cache]
ttl_seconds   = 60   # 缓存有效期（默认 60s）
retry_seconds = 30   # 刷新触发后的 mtime 回拨窗口 = fast-retry 间隔

[network]
base_url = "https://api.kimi.com/coding/v1"  # 国际站改 https://api.kimi.ai/coding/v1
http_timeout_seconds = 8                      # 默认 8s
```

优先级：env `KIMI_CODE_BASE_URL` > `[network] base_url` > 内置默认。`order` 中出现未知字段名时忽略该字段（不失败）。

---

## 9. 错误与降级矩阵

| 场景 | 取数侧行为 | 渲染侧表现 |
|---|---|---|
| 无 token（凭证链两步皆空） | `--refresh` 静默退出，不发请求；`--test-fetch` 输出 `error:"no-token"` | 额度组整体省略，其余字段正常，不阻塞 |
| 404（纯 API key / 非 managed OAuth 账号） | 归入 `HttpRequestException`（quota.rs:100-102），不写缓存 | 同上：额度组省略，其余字段正常 |
| 网络失败 / 超时（8s） | `HttpRequestException` / `TaskCanceledException`；不写缓存 | 旧缓存（LKG）继续渲染，**不清空**；30s 后 fast-retry |
| 响应 JSON 解析失败 | `JsonException`；不写缓存 | 同上 |
| 缓存缺失 / 首跑 | 渲染模式创建空缓存锚定 mtime → 回拨 → 派生 refresh | 首次额度组省略；成功写入后 ≤1s 内（宿主节流）出现 |
| 缓存损坏（JSON 非法） | — | 当作缺失渲染（额度组省略），垃圾内容不清空；派生仍由纯 mtime 判定驱动（§4.4）：age ≥ TTL → 回拨 + 派生，TTL 内不反复派生 |
| 终端宽度不足 | — | 内置降级阶梯（§7.5）：丢 reset → 丢 gitBranch → 只留额度组 → 宿主截断兜底 |
| stdin JSON 非法 / 为空 | — | 按空 payload 渲染（字段缺失则省略），exit 0 |
| 渲染进程超 300ms / 崩溃 | — | 宿主杀进程树（status-line-command.ts:75-78），沿用 last-good 行（:174-179）或回退内置布局 |
| quota-bar.toml 缺失 / 非法 | — | 全部内置默认，渲染照常 |
| tui.toml 丢 `[status_line]` | — | 额度行整体消失（宿主不再调用），见风险① |
| `KIMI_CODE_HOME` 覆盖 | 凭证/缓存/配置三处路径全部随之（§5.2/§5.3/§8） | 同左 |

---

## 10. 测试契约

### 10.1 单元测试

- 解析防御规则 §6 每条至少一个 case，以 quota.rs:243-374 与 `repos/kimi-planbar-tui/go/internal/core/quota_test.go` 为蓝本（字符串/数字混排、除零、`isEnabled=false`、单位四舍五入、`i64::MIN` 敌意值、reset 阶梯）。
- 凭证链：credentials.rs:115-147 的两个既有单测函数原样移植（覆盖紧凑/带空格节名、节结算、空 api_key 拒绝场景）。
- 本工具新增：thinking 阶梯（enabled=false / effort / models 回退）、颜色阈值边界（59.x/60/84.x/85）、reset 跨天格式、宽度降级阶梯、UTF-8 lossy、mtime 回拨计算（`now-TTL+30`）、月段有/无 limit。

### 10.2 golden parity（30 case 对齐）

- 机制复刻 `quota_test.go:206-293`：输入 payload 内联（拷贝自 quota_test.go:240-266）→ 注入解析函数 → 以固定时钟序列化 → 与本仓库 `testdata/golden/quota-*.txt` **逐字节对齐**（30 个既有 case 于 2026-10-01 从 `repos/kimi-planbar-tui/go/testdata/golden/` 逐字节复制内化，CI 自足；比较前做 CRLF 归一化，goldens_test.go:23-25 同款）。
- 固定时钟沿用 quota_test.go:14-15 的两个常量：`atZero`（1893456000000ms 整）、`atFracs`（+123456789ns）。
- 序列化格式与 `--test-fetch` 输出共用同一函数（serde pretty、2 空格缩进、camelCase），保证 parity 即覆盖二进制输出格式。
- `month` 字段以 `skip_serializing_if = "Option::is_none"` 序列化：既有 30 个 golden（无 month 键）保持 byte-identical（2026-10-01 已内化 `testdata/golden/`）；month 新 case 3 个（`quota-month-full`、`quota-month-no-limit`、`quota-month-zero-limit`）原生于本项目 `testdata/golden/`，不回写参考仓库。
- golden 断言含 +08:00 时区偏移（固定时钟按本机时区序列化，沿袭参考仓库 Go 测试的 time.Local 语义）：非 +08:00 机器上 parity 测试 fail-fast 并给出明确原因，不作静默跳过。
- `quota-error-*` 4 个 case 不走网络，直接构造 error 结果比对（quota_test.go:272-275 同款）。

### 10.3 `--test-fetch` 自检

- 真机（managed OAuth 已登录）执行 `quota-status.exe --test-fetch`：断言 stdout 为合法 JSON、`error == null`、`fiveHour`/`week` 有值、`fetchedAt` 接近当前时间。
- 无凭证环境：断言输出 `error == "no-token"`。
- 断言不产生缓存副作用（运行前后缓存文件 mtime/内容不变）。

### 10.4 真机验收标准

1. 产物为单个静态 exe，体积 ≤5MB（§11；实测 ≈1.7MB）。
2. 渲染进程端到端 <50ms（宿主 300ms 上限内留 6 倍余量）；热路径 <10ms。
3. 配好 tui.toml 并 `/reload-tui` 后，额度行显示于 footer 第 1 行；**活跃会话**（有输入/流式/活动 goal）中额度数据在 **1 分钟内**自动更新（TTL 60s + 事件驱动重渲染，§3.3）；纯空闲会话宿主不调用 command、刷新不触发，恢复活动后数秒内随一次渲染自愈。
4. 断网/接口失败后，旧额度数据保留显示不清空；网络恢复后 ≤90s 内自愈（30s fast-retry + 60s TTL 内）。
5. 非 managed OAuth 账号：permissionMode/model/gitBranch 正常，无额度组，无报错行。
6. `/theme` 或 CLI 升级后执行排查演练：当前 main 中偏好保存整文件重写 tui.toml，但 status_line 的 command/items 值 round-trip 保真（config.ts:298-312）、丢失的仅段内注释与未知键——验证 command 值存活即可；旧版本（如 0.31.1 时代报告）遇整段丢失，补回 `[status_line]` 并 `/reload-tui` 可恢复（风险①）。

---

## 11. 工程约束与构建

- cargo 单 crate；仅 Windows x64 目标（`x86_64-pc-windows-msvc` + `.cargo/config.toml` 里 `rustflags = ["-C", "target-feature=+crt-static"]` 静态链接 CRT），产物无 runtime 依赖。
- release profile：`opt-level = "s"`、`lto = true`、`codegen-units = 1`、`strip = true`、`panic = "abort"`。
- 依赖取向（控制体积）：`serde`/`serde_json`、`chrono`、`toml`（quota-bar.toml 与 config.toml 的 thinking/models 段）、HTTP 用阻塞式轻量 client（首选 `ureq` + rustls + 打包根证书；若换 `reqwest` 必须 blocking + rustls），`filetime`（mtime 回拨）、`windows-sys`（console 宽度、creation flags）。**不引入 tokio/async**——取数是一次性阻塞调用，渲染是派生后即退出。
- 体积预算：静态 exe **≤5MB**（v1.2：原"3–5MB"为撰写时的预估区间，实现实测 1,788,928 字节 ≈1.7MB，预算按上限约束执行）；超预算时优先换 HTTP/TLS 后端。
- 渲染路径禁网络、禁重 IO（<10ms 预算）；取数路径 HTTP 超时 8s。
- stdout 强制 UTF-8（errors=replace 语义，§7.6）；detached 子进程 `CREATE_NO_WINDOW | DETACHED_PROCESS` + stdio DEVNULL（§4.4）。
- 新写代码量预计 300–500 行（渲染 + 字段配置 + 缓存三模块）。
- CI/Release 工程化（v1.3.2 补录）：`.github/workflows/ci.yml`（push/PR 触发：`cargo fmt --check` + `cargo clippy --all-targets -- -D warnings` + 单元 + golden + release 构建 + 5MB 体积门禁）；`.github/workflows/release.yml`（tag `v*` 触发：tag↔Cargo.toml version 一致性校验 + 全量测试 + 构建 + 自动建 GitHub Release 挂 exe/sha256）。变更历史手写维护于 `CHANGELOG.md`（Keep a Changelog 格式）。

---

## 12. 风险与缓解

| # | 风险 | 缓解 |
|---|---|---|
| ① | 偏好保存（如 `/theme`）会整文件重写 tui.toml：当前 main 中 status_line 的 command/items 值 round-trip 保真（config.ts:298-312），丢失的是段内注释与未知键；0.31.1 时代报告的"整段静默丢失"属旧版本行为，未在当前源码复现 | 文档写明"额度行消失先查此"（§10.4 验收 6）：先确认 command 值是否存活（round-trip 应保真）；旧版本遇整段丢失则补回 `[status_line] command` 并 `/reload-tui` |
| ② | usages API 结构随版本漂移 | 防御解析（§6）+ golden 回归（§10.2）：漂移通常表现为字段缺失/类型变化，落入既有兜底而非 panic；新增漂移形态补 golden case |
| ③ | 非 managed OAuth 账号无端点（404） | 404/无 token 时额度段整体省略，不阻塞其余字段（§9）；`--test-fetch` 可自证 `error` 类型 |
