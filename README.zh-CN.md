# quota-status

[![CI](https://github.com/shawn-0106t/kimi-code-quota-status/actions/workflows/ci.yml/badge.svg)](https://github.com/shawn-0106t/kimi-code-quota-status/actions/workflows/ci.yml) [![GitHub Release](https://img.shields.io/github/v/release/shawn-0106t/kimi-code-quota-status)](https://github.com/shawn-0106t/kimi-code-quota-status/releases)

[English（权威版本）](README.md) | 简体中文（本文为随版本同步的中文译本，不一致时以英文版为准）

Kimi Code CLI 的 statusline 额度显示器：静态单二进制 `quota-status.exe`（Rust，Windows x64），在 footer 第 1 行显示 Kimi For Coding 套餐额度（5h / week / month + reset 时间）与后台任务徽章（`[N task(s) running]` / `[M agent(s) running]`），字段开关与行内顺序可自定义。

## 工作原理

```
宿主(1s 节流, 300ms 硬超时) ──spawn──> quota-status.exe（渲染模式）
                                        │ 读 stdin 快照 / 缓存 / quota-bar.toml / config.toml[thinking]
                                        │ 缓存 age ≥ TTL → mtime 回拨 + 派生 detached:
                                        └──spawn(DETACHED|NO_WINDOW)──> quota-status.exe --refresh
                                                                                        │ GET {base_url}/usages（8s 超时）
                                                                                        │ 解析 → 原子写缓存 → 退出
```

- **渲染模式**（默认）：只读本地缓存拼一行 ANSI 文本，毫秒级退出，禁网络。
- **取数模式** `--refresh`：凭证链 → GET /usages → 防御解析 → 原子写缓存；任何失败不写缓存（LKG 保留），30s fast-retry。
- **自检模式** `--test-fetch`：完整取数一次，pretty JSON 打印到 stdout，不写缓存。
- **版本查询模式** `--version`：打印 `quota-status <版本>`（与 exe 文件属性同源）后退出，便于核验已部署副本的版本。
- 宿主是事件驱动的：空闲会话不调用 command，"1 分钟刷新"在活跃会话成立；缓存 TTL 默认 60s。
- 凭证只读不写：token 新鲜度依赖运行中的 CLI，本工具绝不自行刷新 OAuth token。

## 安装

1. 从 [Releases](https://github.com/shawn-0106t/kimi-code-quota-status/releases) 下载 `quota-status.exe`（或 `cargo build --release` 自行构建，产物在 `target/release/`），放到固定位置（示例用 `C:\tools\`，路径可自定）。
2. 编辑 `~/.kimi-code/tui.toml`，加入：

   ```toml
   [status_line]
   command = "C:\\tools\\quota-status.exe"
   ```

   **注意**：TOML 基本字符串中 Windows 路径的反斜杠必须写成 `\\`。
3. 在 Kimi Code TUI 内执行 `/reload-tui`（或重启 CLI）生效。
4. 自检：`C:\tools\quota-status.exe --test-fetch` 输出 `"error": null` 即取数链路正常（`"error": "no-token"` 表示当前无可用凭证——见下文故障排查）。

## 配置 `~/.kimi-code/quota-bar.toml`（可选）

无此文件时全部取以下默认值；非法键落回默认，渲染绝不因配置失败，未知字段忽略。路径随 `KIMI_CODE_HOME` 联动。

```toml
[render]
# 行内字段顺序（可删减、可重排；未列出的字段不显示）
order = ["permission_mode", "model", "thinking", "tasks", "quota", "git_branch"]
# 单色开关：false 时输出纯文本（无任何颜色码），整行由 Kimi Code 包装为主题
# text 色——与第 2 行 context 读数同色，随 /theme 切换联动；默认 true 多彩
colors = true

[render.quota]
five_hour  = true
week       = true
month      = true
reset_time = true   # false = 永久丢 reset 时间后缀
booster    = false  # 默认不显示 booster 钱包余额（数据仍解析入缓存）

[thresholds]
# 百分比颜色阈值：percent < green_below → 绿；< yellow_below → 黄；否则红
green_below  = 60
yellow_below = 85

[cache]
ttl_seconds   = 60   # 缓存有效期
retry_seconds = 30   # 刷新触发后的 mtime 回拨窗口 = fast-retry 间隔

[network]
base_url = "https://api.kimi.com/coding/v1"  # 国际站改 https://api.kimi.ai/coding/v1
http_timeout_seconds = 8
```

环境变量优先级：`KIMI_CODE_BASE_URL` > `[network] base_url` > 内置默认。

**tasks 徽章**（默认 order 已含 `"tasks"`）：显示当前会话的后台任务计数——bash 后台任务 `[N task(s) running]`（经 pid 存活校验）、后台 subagent `[M agent(s) running]`，cyan 配色，位于额度组之前，两者皆零时整段省略。不新增配置键：从 `order` 删去 `"tasks"` 即关闭该段（连带跳过会话任务目录扫描）；v1.0 时代已显式写了 `order` 的配置需手动把 `"tasks"` 加回才会显示。

## 故障排查（先读这里）

**① 额度行整体消失**——按顺序检查：

1. `tui.toml` 里 `[status_line]` 段和 `command` 值是否还在？Kimi Code 的偏好保存（如 `/theme`）会**整文件重写** tui.toml：当前版本对 `command`/`items` 的值 round-trip 保真，丢失的只是段内注释与未知键；但**旧版本 CLI**（如 0.31.1 时代）可能整段静默丢失。
2. 整段丢失的恢复：把上面的 `[status_line]` 配置补回 tui.toml，再执行 `/reload-tui`。
3. 排查取数：`quota-status.exe --test-fetch`——
   - `"error": "no-token"`：凭证链无可用 token。若 CLI 处于活跃会话，等其续期后重试；纯 API key 用户需确认 config.toml 的 provider `base_url` 含 `api.kimi.com/coding`（或 `api.kimi.ai/coding`）且 `api_key` 非空。
   - `"error": "HttpRequestException"`：网络不通或非 managed OAuth 账号（404，该账号类型无 usages 端点）。
   - `"error": "TaskCanceledException"`：超时（默认 8s）。
4. 额度组不显示但其余字段正常：`--test-fetch` 看缓存数据；首跑/缓存缺失时额度组会先省略，成功回填后 ≤1s 内出现。

**其他**：渲染被宿主 300ms 杀进程树时会沿用 last-good 输出；若怀疑防病毒拖慢首次启动，可将 exe 目录加入排除项。

## 构建与测试

```bash
cargo build --release          # 产物 target/release/quota-status.exe（静态 CRT，无 runtime 依赖）
cargo test                     # 68 单元测试
cargo test --test golden       # golden parity（33 个 golden 逐字节对齐）
```

golden 输入 payload 与期望值复刻自 `repos/kimi-planbar-tui`（只读参考仓库），33 个 golden 已全部内化本仓库 `testdata/golden/`（30 个既有 + month 3 个原生），测试运行时不依赖 `repos/`。固定时钟 `atZero`/`atFracs` 序列化结果按本机时区 `+08:00` 断言。

## 验收记录（2026-10-01）

| 项 | 结果 |
|---|---|
| 静态单 exe | dumpbin /dependents 仅系统 DLL（kernel32/bcrypt/advapi32/ntdll/ws2_32），无 vcruntime |
| 体积 | 1,787,904 字节（≈1.7MB）——**低于 SPEC §11 预估区间 3–5MB 的下限**；预算本意为上限约束（不超 5MB），实际显著优于预估，未做任何填充处理 |
| 端到端渲染 | 100 次均值 33.79ms/次（PowerShell CreateProcess，含进程启动）< 50ms |
| 热路径（进程内） | 渲染均值 ≈ 纯进程启动基线（35.7ms），进程内工作仅数 ms < 10ms |
| golden parity | 30 既有 case + 3 month 新 case 逐字节一致（CRLF 归一化后） |
| 断网 LKG | 不可达 base_url 下缓存内容不变、无残留进程（30s fast-retry + 60s TTL 内自愈，≤90s） |
| 非 managed OAuth | `error:"no-token"`/404 均只省略额度组，其余字段正常 |

### 实装验收（2026-10-01 22:43，用户 CLI 会话内确认通过）

| 项 | 结果 |
|---|---|
| 部署 | exe 部署至 `~/.kimi-code/bin/quota-status.exe`（脱离 target/，cargo clean 不影响；**重新构建后需重新复制到该路径**，当前二进制 1,793,024 字节，review 修复后 rebuild，仍 ≈1.7MB）；tui.toml 已配置 `[status_line] command`（TOML 解析验证通过，备份 `~/.kimi-code/backups/tui.toml.20261001-223956.bak`） |
| `--test-fetch` 自检（§10.3） | `error: null`，fiveHour/week/resetAt 均有值，`fetchedAt` 实时；运行前后缓存 mtime/内容不变，无副作用 |
| footer 显示（§10.4 条 3 前半） | `/reload-tui` 后 footer 第 1 行正确显示 `permissionMode \| model \| thinking \| 5h N% (rst …) · week N% (rst …) \| gitBranch`，颜色/分隔符/跨天 reset 格式符合 §7；真机 console 下 CONOUT$ 宽度查询正常（行完整、未误触发降级，REVIEW3 Major 1 修复点复核通过） |
| 1 分钟自动更新（§10.4 条 3 后半） | 活跃会话中自动回填真机成立：缓存 `fetchedAt` 22:35:34 → 22:43:26 连续推进，fiveHour percent 43→45 实时变化（TTL 60s + mtime 回拨 + detached refresh） |
| `/theme` 重写演练（§10.4 条 6） | 本轮未实际触发：验收期间 tui.toml mtime 保持 22:40:34、段内注释仍在，host 未整文件重写；command 值 round-trip 保真仍以当前 main 源码（config.ts:298-312）为准，旧版本整段丢失的恢复步骤见故障排查① |

渲染边界抽样（实装当日）：空 stdin → exit 0 按空 payload 降级；yolo→红、gitBranch null→省略；空锚定缓存 → mtime 回拨（精确 now−30s）→ detached refresh → 原子写缓存 401 字节，全链路观测通过。渲染端到端 20 次均值 ≈44ms（Git Bash 管道测量，含 shell fork 开销，为高估方向）< 50ms。

### P7 tasks/agents 徽章验收（2026-10-02，临时 KIMI_CODE_HOME 隔离环境）

| 项 | 结果 |
|---|---|
| 徽章渲染与位置 | 伪造 running bash 任务（pid 指向存活子进程）→ 输出含 cyan `[1 task running]` 且位于额度组之前，exit 0 |
| pid 存活校验 | 终止子进程后再渲染 → 徽章消失 |
| 路径防御 | sessionId 为 `../evil` / `a/b` / `a\b` / `.` / `C:evil` 均无徽章且 exit 0（白名单 `[A-Za-z0-9_-]`，严格于 SPEC §7.7 决策 B 三条规则——code review Minor 1 加固） |
| 终态跳过 | `status:"completed"` 任务不产生徽章 |
| agent 侧 | `kind:"agent"` running 无 pid → `[1 agent running]`（running 即计入）；与 bash 并存 → `[1 task running] [1 agent running]` 单空格连接 |
| 单色组合 | `colors = false` 时徽章随单色路径输出纯文本、无任何 SGR |
| 门禁 | fmt / clippy `-D warnings` / 66 单元测试（首轮 review 时点，修复轮后 68）/ 33 golden parity / release 构建 1,800,704 字节全绿；独立 code-reviewer 证伪复核结论"可交付"（0 Critical/0 Major，2 Minor 加固已落地） |
| 热路径计时 | 真实负载（1 workspace + 1 任务）30 次均值与无扫描基线持平（81 vs 82 ms，Git Bash 管道含进程启动，增量 ≈0）；人工病态构造（64 workspace 探测 + 32 任务 json + 32 次 pid 校验打满）增量 ≈18ms，超 §4.1 的 10ms 进程内预算——真实会话不可达该形态且距宿主 300ms 硬超时余量 >5 倍；按 PLAN P7 风险节约定，收紧 64/32 上限属 SPEC §7.7 决策 D 契约数值，留待决策未单方面修改（2026-10-02 晚用户定案：维持现状） |
| 真机 TUI 验收（PLAN 标准 5，2026-10-02 晚） | 二次 review 的后台独立进程（`kimi -p`，带 pid 的 bash 任务）运行期间 footer 第 1 行出现 `[1 task running]`（用户配置 colors=false，纯文本色），进程结束后随重渲染消失——**P7 验收标准 1–5 全部通过** |
| 二次独立 code review（k3-256k/max，2026-10-02 晚） | **Request Changes**：1 Major（M-1：任务文件读取/解析失败被实现为「跳过该文件保留部分计数」，与 SPEC §7.7 决策 E/§9 的段整体省略相悖，且单测断言方向写反）——经用户仲裁**修代码对齐契约**（短路返回零计数 + 断言反转 + 空态豁免单测，修复后 67 单测全绿）；3 Minor 中文档类已同步（SPEC/PLAN v1.5.1），其余 Minor/Nit 为有意保留（明细见 docs/HANDOFF.md §7 附记） |

## 已知限制

- 仅 Windows x64（`x86_64-pc-windows-msvc` + crt-static），不做跨平台。
- `contextTokens`/`maxContextTokens` 不进渲染行（宿主 footer 第 2 行原生显示）。
- booster 钱包默认不显示；开启 `booster = true` 后以 `boost 余额` 形式（纯 ASCII，cyan）附加在额度组内（仅 Ready 状态）。
- 国际站（api.kimi.ai）纯 API key 用户的凭证兜底依赖 config.toml provider 匹配，OAuth 凭证不受影响。

## 变更历史

见 [CHANGELOG.zh-CN.md](CHANGELOG.zh-CN.md)（英文权威版：[CHANGELOG.md](CHANGELOG.md)）。

## 许可证

[MIT](LICENSE) © 2026 Shawn Qi (shawn-0106t)；quota 端点与凭证加载逻辑派生自 [kimi-planbar](https://github.com/baigong-ai/kimi-planbar)（© baigong-ai，MIT），归属说明见 [NOTICE](NOTICE)。
