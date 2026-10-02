# HANDOFF — P7 tasks/agents 徽章迭代交接（2026-10-02）

- 目的：记录 P7 迭代的实现状态、验证结论与未尽事项，供后续会话/协作者接手。契约事实来源仍是 `docs/SPEC.md` v1.5 与 `docs/PLAN.md` P7 节，本文不重复契约内容，只记录"做到哪了、还差什么、坑在哪"。
- 性质：历史工作记录（与 `docs/REVIEW.md`/`docs/REVIEW3.md` 同类）。是否随下次 commit 入库由提交者决定；若不入库，接手前请先读完本文。

---

## 1. 状态总览

**P7 实现 + 测试 + 独立 code review + 部署 + 真机验收（PLAN P7 验收标准 2–4）已全部完成；变更已入库（commit `087c266`，2026-10-02 晚，含本文档）。** 验收标准 5 的最终结论、两个决策点的用户定案、二次独立 review（k3-256k/max）的 Major 发现与修复经过见文末 §7 附记。

## 2. 变更清单（当前工作区，`git status` 一致，未提交）

| 文件 | 变更 |
|---|---|
| `src/tasks.rs` | **新建**。扫描计数入口 `count_running(kimi_home, session_id)` + pid 存活校验（`OpenProcess`/`GetExitCodeProcess`）+ 13 个单测（v1.5.1 勘误：本文初版误记 12） |
| `src/config.rs` | `Field::Tasks` 变体、`parse_field` 认识 `"tasks"`、默认 order 插入 `"tasks"`（thinking 与 quota 之间）+ 1 个单测 |
| `src/render.rs` | `render_variant` 增加 `TaskCounts` 参数与 `Field::Tasks` 分支；`render_line` 新增 `kimi_home: Option<&Path>` 参数，计数在外层执行**一次**传入 4 个降级变体；order 不含 tasks 跳过扫描 + 5 个新单测（v1.5.1 勘误：本文初版误记 6） |
| `src/main.rs` | `render_line` 调用点传入 `credentials::kimi_home().as_deref()` |
| `src/lib.rs` | 挂 `pub mod tasks;` |
| `README.md` | 简介补徽章、配置示例 order 补 `"tasks"`、徽章说明段、单元测试计数 66、新增「P7 验收记录」表 |
| `AGENTS.md` | 「实现落定要点」补 P7 一条（含白名单加固）；测试计数 47→66 |
| `CHANGELOG.md` | `[Unreleased]` 新增 P7 条目 |

建议 commit message（沿用仓库风格）：`feat: P7 tasks/agents 徽章（sessions 扫描计数 + pid 校验，SPEC/PLAN v1.5）`

## 3. 已完成的验证与结论

### 3.1 门禁（全绿）

- `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings` 零告警
- `cargo test`：66 单元测试通过（原 47 + 新增 19）
- `cargo test --test golden`：33 case 逐字节 parity 通过
- `cargo build --release`：1,800,704 字节（≈1.8MB，≤5MB 预算）；部署版 sha256 `c3282243...702c2bfc7` 与构建产物一致

### 3.2 独立 code-reviewer 证伪复核

- 结论「可交付」：**0 Critical / 0 Major / 2 Minor / 4 Nit**
- 已落地的加固（各补单测）：
  - Minor 1 → sessionId 校验从三条拒绝规则（`/`、`\`、`..`）升级为**字符白名单 `[A-Za-z0-9_-]`**（宿主真值 `session_<uuid>` 恒在其内；封堵盘符相对路径 `"C:evil"`——`Path::join` 遇带 prefix 的参数会替换整个 base、裸 `"."`、非 ASCII 等残余穿越面）
  - Nit 4 → 单个任务 json **64KB 大小护栏**（超大文件视为格式漂移跳过，防病态文件全量载入拖垮热路径）
- 未处理的 Nit（均为理论项，有意保留）：Nit 3（sessions 下非目录条目不消耗探测配额，目录污染场景无界 stat）、Nit 5（测试依赖 NTFS 枚举名称序）、Nit 6（PLAN 涉及文件清单漏列，见 §4 决策点 2）

### 3.3 真机验收（临时 KIMI_CODE_HOME 隔离环境，部署版 exe）

| 项 | 结果 |
|---|---|
| 验收 2 | 伪造 running bash 任务（pid 指向存活子进程）→ cyan `[1 task running]` 位于额度组之前；杀子进程后徽章消失 |
| 验收 3 | sessionId `../evil` / `a/b` / `a\b` / `.` / `C:evil` 均无徽章且 exit 0；`status:"completed"` 不计；无 sessions 目录正常渲染 |
| 验收 4 | agent running 无 pid → `[1 agent running]`；与 bash 并存 → `[1 task running] [1 agent running]` 单空格连接；`colors=false` 纯文本无 SGR |
| 计时复核 | 真实负载（1 ws + 1 task）与无扫描基线持平（81 vs 82 ms，30 次均值，含进程启动，增量 ≈0）；人工病态构造（64 probes + 32 json + 32 pid 校验打满、预热后）增量 ≈18ms |

### 3.4 用户真实环境的实测佐证（决策点 1 的依据）

本机 `~/.kimi-code/sessions/`：37 个 workspace 目录、305 个历史任务 json、当前 0 个 running；用户 `quota-bar.toml` 只配了 `colors = false` 未写 order → 默认 order 生效 → 徽章已启用但平时不显示。真实 home 渲染 81–89 ms vs 空 home 基线 82 ms——增量在测量噪声内。结论与建议：**维持现状，不收紧上限**。

## 4. 待办与决策点（均需用户输入，接手者勿擅自执行）

1. **提交**：等用户明确说"提交"后执行（纪律：未经明确指令不 commit）。注意 `docs/HANDOFF.md` 本身是否入库需用户确认。
2. **PLAN P7 验收标准 5（用户操作）**：在 Kimi Code TUI 中发起一个后台 bash 任务 → footer 第 1 行应出现 cyan `[1 task running]`（活跃会话 ≤1s + 节流）；任务结束后随下次重渲染消失。部署版已生效（command 每次渲染重新 spawn exe，无需 `/reload-tui`）。
3. **决策点 A——扫描上限**：病态构造（同会话 32 个 running bash 任务 + 64 workspace 探测打满）下扫描增量 ≈18ms，超 SPEC §4.1 的 10ms 进程内预算；真实负载增量 ≈0 且距宿主 300ms 硬超时余量 >5 倍。选项：(a) 维持现状（**建议**，真实会话不可达该形态）；(b) 收紧 64/32 上限（属 SPEC §7.7 决策 D 契约数值变更，须先修 SPEC 再改 `src/tasks.rs` 两常量 + 单测）；(c) 用户 order 删 `"tasks"` 零成本停用。
4. **决策点 B——PLAN 勘误**（reviewer Nit 6）：`docs/PLAN.md` P7「涉及文件」清单漏列实际改动的 `src/lib.rs` 与 `src/main.rs`。是否顺手勘误由用户定（本轮纪律是不动契约文档）。

## 5. 接手须知（实现落定要点速查）

- **计数只算一次**：`render_line` 外层调用 `count_running` 一次，`TaskCounts`（Copy）传入 full/no_reset/no_git/quota_only 四个变体——勿把计数挪进 `render_variant`（会执行 4 次）。
- **开关省 IO**：`cfg.order.contains(&Field::Tasks)` 为 false 时跳过扫描（kimi_home 照传但不读目录），与 thinking 段惯例一致。
- **徽章格式**：`[N task running]`/`[N tasks running]`、`[M agent running]`/`[M agents running]`，单复数按 `== 1`；两徽章各自 span 包装（对齐宿主 per-badge chalk）后以单空格 join 成一段；cyan 36；皆零整段省略。勿改成单一大 span（曾有测试断言写错方向）。
- **计数口径**：仅 `status == "running"`（严格相等）参与；`kind == "agent"`（严格相等）running 即计入、无 pid 字段；其余一切 kind 走 pid 校验（`pid_of` 仅接受 JSON 正整数 ≤ u32::MAX，字符串/负数/小数/0/超界视同缺失）；question 无 pid 恒不计入（已仲裁）。
- **windows-sys 语义坑**：`STILL_ACTIVE` 在 `Win32::Foundation`（NTSTATUS = i32），而 `GetExitCodeProcess` 出参是 u32——比较须 `as u32`。
- **测试的已知假设**：`scan_limits_truncate_not_fail` 的 workspace 截断用例依赖 NTFS 目录按名称序枚举（B+ 树，CI 与开发机 TEMP 均为 NTFS，实践中稳定）；`exited_pid` 用 spawn-exit-wait 的子进程并 assert `!process_alive`（PID 复用理论窗口自担，注释已写明）；temp 目录按 `tag + 进程 id` 隔离、先清残留，测试可并行。
- **部署流程**：`cargo build --release` 后**必须重新复制**到 `~/.kimi-code/bin/quota-status.exe`（运行中会话占用时先 `mv` 为 `quota-status.exe.old` 再复制；本轮直接复制成功说明当时无锁）。
- **真机验收复现要点**：临时 `KIMI_CODE_HOME` 下伪造 `sessions/wd_*/session_*/agents/main/tasks/*.json`；存活 pid 用 PowerShell `(Start-Process cmd -ArgumentList '/c','ping -n N 127.0.0.1 >nul' -WindowStyle Hidden -PassThru).Id` 取（Git Bash 的 `$!` 是 MSYS pid 不可用）；额度组显示需伪造新鲜 mtime 的 `cache/quota-status.json`（schema 见 SPEC §5.3）。
- **计时方法学坑**：Git Bash 下用 `date +%s%N` 做 bench 时，累加变量勿在循环内被 `S=$(date ...)` 重新赋值（epoch 纳秒 ≈1.8e18，int64 累加会溢出出天文数字）；应每轮 `TOT=$((TOT + E - S))`。MSYS 管道测得的绝对值（基线 ≈82ms）远高于宿主真实 spawn 形态（≈33–44ms，见 README 既有记录），只用于相对对比。

## 6. 快速验证命令

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings
cargo test                    # 66 单元测试
cargo test --test golden      # 33 golden（须 UTC+08:00 机器）
cargo build --release         # ≤5MB 单 exe
cp target/release/quota-status.exe ~/.kimi-code/bin/   # 重新部署（勿忘）
```


---

## 7. 附记（2026-10-02 晚，二次独立 review 后）

- **入库**：全部变更（含本文档）已随 commit `087c266` 入库（gitleaks 通过）。
- **验收标准 5 通过**：借二次 review 的后台独立进程（`kimi -p`，带 pid 的 bash 任务）观察真实 TUI——footer 第 1 行出现 `[1 task running]`（用户配置 colors=false，纯文本色），进程结束后随重渲染消失。P7 五条验收标准至此全部通过。
- **二次独立 code review（k3-256k/max，证伪导向）**：结论 Request Changes——1 Major + 3 Minor + 5 Nit；工程门禁（fmt/clippy/单测/golden/release 体积/零新增依赖）实跑全绿。
- **M-1（Major）**：任务文件读取/解析失败被实现为「跳过该文件、保留部分计数」，与 SPEC §7.7 决策 E / §9 的「段整体省略」相悖，且 `malformed_json_skipped` 断言方向写反。用户仲裁：**修代码对齐契约**——`count_task_file` 改返回 `Option<()>`，失败时 `count_in_session` 短路返回零计数；`read_dir(tasks)` 的 NotFound 按空态跳过（其余 IO 错误短路省略）；`malformed_json_omits_segment` 断言反转 + 新增 `agent_without_tasks_dir_is_empty_state`（单测 66→67）。
- **决策点 A**：用户定案维持现状（64/32 上限不变；真实负载增量 ≈0，18ms 病态形态现实不可达）。
- **决策点 B 已修**：PLAN P7 涉及文件补录 lib.rs/main.rs；本文 §2 拆分计数勘误（13/5）；SPEC v1.5.1 补记决策 B 白名单语义（Minor-2）与决策 E NotFound 空态豁免、§10.1 补两单测项；exit code 259 盲区注释（Nit 4）。
- **有意保留不变**：m-1（探测预算不含非目录枚举维度）、m-3（agent 目录枚举无预算，契约未要求）、n-3（NTFS 枚举序依赖）、n-5（§4.1 预算张力，随决策点 A 定案关闭）。
