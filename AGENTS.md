# AGENTS.md — quota-status 项目工作区

## 项目定位

目标产物 `quota-status.exe`：Rust 静态单二进制，作为 Kimi Code CLI 的 statusline command（`~/.kimi-code/tui.toml` `[status_line]`），在 footer 第 1 行显示 Kimi For Coding 套餐额度（5h/week/month + reset 时间）。渲染路径只读本地缓存，缓存过期（TTL 60s）时派生 detached 子进程拉取 `/usages` 回填。

**当前阶段：P0–P5 实现 + 三轮 review + 实装验收全部完成**（2026-10-01）。第三轮独立 code review 发现 2 Major（console CONOUT$ 句柄 access=0 致宽度查询恒失效、缓存新鲜判定把「可解析」相与进来架空 §4.4 防风暴）+ 3 Minor，已全部修复并补回归单测（SPEC bump v1.3）。cargo 工程即仓库根，产物 `target/release/quota-status.exe` ≈1.7MB 静态单 exe。2026-10-01 晚实装完成：exe 部署至 `~/.kimi-code/bin/quota-status.exe`（脱离 target/；重新 build 后统一用 `scripts/deploy.ps1` 部署），tui.toml `[status_line] command` 已配置并生效；真机验收 §10.4 条 3 通过（footer 第 1 行正常显示、活跃会话内缓存 fetchedAt 连续推进、CONOUT$ 宽度查询真机复核正常），条 6 本轮未触发（验收期间 host 未重写 tui.toml）。排查步骤见 README「故障排查」，验收明细见 README「验收记录」。2026-10-01 发布 v1.0.0（tag `v*` 触发 release workflow 自动挂 exe/sha256，含 tag↔version 一致性校验；CI 门禁含 fmt/clippy；master 已开 branch protection 要求 CI `test` job 绿）。**P6（单色开关，SPEC v1.4）与 P7（tasks/agents 徽章，SPEC v1.5/v1.5.1）已于 2026-10-02 交付**：P7 经两轮独立 review——第二轮（k3-256k/max 证伪复核）发现 1 Major（任务文件失败语义与 SPEC §7.7 决策 E/§9 相悖、单测断言反向），经用户仲裁修代码对齐契约；PLAN P7 验收标准 1–5 全部通过（含真机 TUI 后台任务徽章出现/消失观察）。

## 实现落定要点（与参考实现/SPEC 旧文本的差异，均已过 review）

- lib/bin 双目标：逻辑全在 `src/lib.rs`（供 `tests/golden.rs` 集成测试导入解析函数），`src/main.rs` 只做 argv 分发。
- credentials 的 config.toml 逐行扫描为**手写解析**而非 regex（regex 不在 SPEC §11 依赖白名单），语义由移植单测钉死；匹配串已扩展为同时含 `api.kimi.ai/coding`（SPEC §5.2 勘误增强）。
- 原子写 tmp 带 PID 后缀（`quota-status.json.<pid>.tmp`），消除并发 refresh 写同一 tmp 的竞态窗口。
- 缓存新鲜判定为**纯 mtime**（SPEC §4.1 步骤 3 v1.3 钉死，Python `maybe_refresh` 同款）：与缓存是否可解析无关，空锚定/损坏/refresh 持续失败时靠回拨 mtime 压住派生风暴；锚定用 `create_new` 原子语义（绝不截断已有缓存），refresh 启动时清理 >60s 的孤儿 tmp。
- month 段：limit 缺失/0/NaN 均不产生段；booster 开启时显示 `boost 余额`（纯 ASCII，避免 ¥ 在非 UTF-8 终端的兼容性问题）。
- 单色开关（v1.4，P6）：`quota-bar.toml [render] colors = false` 时渲染输出纯文本（无任何 SGR），整行由宿主包装为主题 text 色（与 context 行同色，随 /theme 联动）；默认 true 多彩。
- tasks/agents 徽章（v1.5，P7）：`src/tasks.rs` 扫描 `<kimi_home>/sessions/` 探测 payload sessionId 定位会话目录（workspace 探测 ≤64、任务 json 读取 ≤32、单文件 ≤64KB，超限截断计数非失败），按 kind 分流计数——`kind=agent` running 即计入 agent 侧（无 pid 字段，接受崩溃遗留的陈旧误报，宿主重启加载标 lost 自愈）；其余一切 kind 须经 `OpenProcess` + `GetExitCodeProcess` pid 存活校验（pid 缺失/非正整数视同缺失不计入，question 恒不计入）。计数在 `render_line` 外层算一次传入 4 个降级变体；sessionId 校验为字符白名单 `[A-Za-z0-9_-]`（宿主真值 `session_<uuid>` 恒在其内；SPEC v1.5.1 §7.7 决策 B 已收录——一步覆盖三条拒绝规则并封堵盘符相对路径/裸 `.` 等残余穿越面，P7 首轮 review Minor 加固）；失败语义（v1.5.1 钉死，二次 review Major 修复）：任务文件读取/解析失败短路返回零计数、段整体省略（不得保留部分计数），目录不存在属空态（`agents/<id>/tasks/` 缺失视同无任务跳过，其余 IO 错误仍短路省略）；默认 order 补 `"tasks"`，删去即关闭整段并跳过扫描。
- 错误分类：超时（含 body 阶段）→ `TaskCanceledException`；非 2xx 与其余传输错误 → `HttpRequestException`；body 阶段非超时错误（连接 reset、非法 UTF-8）→ `JsonException`（SPEC §5.1 v1.3 钉死）。

## 权威文档（动手前必读）

- **docs/SPEC.md**（v1.5.1）——唯一契约事实来源。改行为前必读：§3 宿主契约、§5–6 数据层与解析防御规则、§7 渲染与降级、§10 测试契约、§11 工程约束、§12 风险。
- **docs/PLAN.md**（v1.5.1）——P0–P5 分阶段计划 + P6/P7 追加迭代（任务清单、可执行验收标准、依赖顺序），与 SPEC 冲突时以 SPEC 为准；文头有完成状态标注。
- **docs/REVIEW.md**——第二轮独立复核报告（历史记录；其中 8 项发现已逐条仲裁并修订进 SPEC/PLAN，勿据旧表述回改）；**docs/REVIEW3.md**——第三轮实现后 code review 报告（2 Major/3 Minor 已全部修复，复核结论"可交付"）；**docs/HANDOFF.md**——P7 迭代交接记录（实现状态、决策点、接手须知）。

## 硬约束（违反即返工）

- 渲染路径禁网络、禁重 IO（<10ms 预算；宿主 300ms 硬超时，每次渲染 spawn 一个进程）；取数只走 detached 子进程 + 原子写缓存。
- 不引入 tokio/async；HTTP 用阻塞式 `ureq`（+rustls）；release profile：`opt-level="s"`、`lto`、`codegen-units=1`、`strip`、`panic="abort"`，目标 Windows x64 静态 CRT 单 exe ≤5MB（SPEC §11；实测 1.7MB）。
- 绝不自行刷新 OAuth token（依赖运行中的 CLI 续期，凭证链见 SPEC §5.2）。
- 仅 managed OAuth（Kimi For Coding）账号有 `/usages` 端点；纯 API key 账号 404，额度组整体省略。
- 解析逐条遵守 SPEC §6 防御规则：数字按字符串建模、仅严格布尔 `false` 触发 isEnabled 防御、amountLeft 单位 1e-8 元、i64::MIN 必须可解析、resetTime 宽松阶梯、%.0f 为 round-half-to-even。
- 宿主是**事件驱动**的：空闲会话完全不调用 command——"1 分钟刷新"只在活跃会话成立（SPEC §3.3、§10.4 条 3），验收判据据此设计。
- stdout 强制 UTF-8（lossy），detached 子进程 `CREATE_NO_WINDOW | DETACHED_PROCESS`；仅 Windows x64，不做跨平台。

## repos/ —— 只读参考仓库

- `repos/kimi-code`：官方 CLI 源码（宿主契约出处：`apps/kimi-code/src/tui/utils/status-line-command.ts`、`components/chrome/footer.ts`、`tui/config.ts`；额度 API 出处：`packages/oauth/src/managed-usage.ts`）。
- `repos/kimi-planbar-tui`：可复用资产——`rust/src/`（credentials/quota/http 纯逻辑约 720 行）、`docs/SPEC.md` §16（API 契约）、`go/testdata/golden/`（30 个 golden case）。
- `repos/kimi-planbar`：statusline Python 原型（渲染段/颜色/降级语义来源）。
- **规则：全部只读**。不修改、不提交、不 `unshallow`；已 gitignore，不入本仓库。

## 验证与测试

- 常用命令：`cargo build --release`（静态单 exe）；`cargo test`（68 个单元测试）；`cargo test --test golden`（33 个 golden 逐字节 parity——golden 的 datetime 偏移固定 +08:00，测试内有时区 fail-fast，须在 UTC+08:00 机器上跑）；自检：`target/release/quota-status.exe --test-fetch`（headless 不写缓存，`error == null` 即链路正常，无凭证输出 `"error": "no-token"`）。发版：打 tag `v*` 推送即触发 `.github/workflows/release.yml`（全量测试 + 构建 + 自动建 GitHub Release 挂 exe/sha256）。CI 门禁含 `cargo fmt --check` 与 `cargo clippy --all-targets -- -D warnings`（代码须先过这两关）。
- golden parity：33 个 case 已全部内化 `testdata/golden/`（2026-10-01 从参考仓库逐字节复制 30 个既有 case，CI 自足；month 3 个原生），逐字节比对 + CRLF 归一化；参考仓库仍为上游事实来源，新增 case 绝不回写。
- 真机验收六条见 SPEC §10.4；验收记录（含 2026-10-01 晚实装验收：footer 显示与活跃会话 1 分钟自动更新已通过、`/theme` 演练未触发）与偏差记录（体积 1.7MB 低于预估下限 3–5MB）见 README「验收记录」。
- **自动化（2026-10-03 起）**：CI 首个 job 为 gitleaks 全历史敏感扫描（本地 pre-commit 的兜底防线；注意 gitleaks **未纳入** branch protection 的 required check——required 仅有 `test`，gitleaks 红需人工处理）；Dependabot 管理依赖与 CI action 升级（weekly，patch/minor 合并单 PR、major 单独开 PR——合并走 PR 的 required check，**不要直推绕过**）；Dependabot security updates 已在 repo 设置开启（漏洞披露驱动、独立于 weekly version updates；注意 dependabot.yml 的 groups 默认 `applies-to: version-updates`，**不**作用于 security updates——安全 PR 合并靠 grouped security updates 设置），配套项 Dependabot alerts / malware alerts / grouped security updates 亦已全部开启（后两者仅 UI 可开，无 REST API）；CodeQL default setup 已开（2026-10-03 经 API，识别 rust+actions，UI 子项 check runs failure threshold 保持默认；首扫报 1 条 medium 告警 `actions/missing-workflow-permissions`——ci.yml `test` job 缺显式 permissions——已于当日修复：全 workflow 各 job 显式最小权限，修复 push 后 CodeQL 重扫即自动关闭告警）；部署统一用 `powershell -File scripts/deploy.ps1`（build --locked → 占用时 .old 改名兜底 → 复制 → sha256 校验，`-DstDir` 可覆盖目标目录），**重新构建后不要再手工复制 exe**；安全漏洞报告走 SECURITY.md 的私密漏洞报告渠道；README/CHANGELOG 自 2026-10-06 i18n pass 起改为**英文为权威**（中文译本迁至 `README.zh-CN.md` / `CHANGELOG.zh-CN.md`，随版本同步）；SPEC 仍中文为权威（`SPEC.en.md` 随版本同步）；代码注释与 doc comment 已全量英文化。

## 约定

- 语言约定（2026-10-06 i18n pass 起）：面向全球用户的门面（README/CHANGELOG/CONTRIBUTING/issue·PR 模板）与代码注释、doc comment 一律英文；内部过程文档（SPEC/PLAN/REVIEW 系/HANDOFF）与用户沟通用中文；技术术语保留 English。
- 仓库根即 cargo 工程根（P0 落地：`Cargo.toml`、`.cargo/config.toml`、`src/` lib+bin 双目标、`tests/golden.rs`、`testdata/golden/`；`.gitignore` 已含 `target/`）；`rust-toolchain.toml` 钉贡献者工具链（CI 仍用 stable，MSRV 以 Cargo.toml `rust-version` 为准）。
- 设计文档集中在 docs/（SPEC/PLAN/REVIEW 系/HANDOFF；SPEC.en.md 为 SPEC 英文译本，PLAN/REVIEW 系/HANDOFF 文头带 English abstract）；根目录保留 AGENTS.md、README.md（英文为权威）+ README.zh-CN.md、CHANGELOG.md（英文为权威）+ CHANGELOG.zh-CN.md、CONTRIBUTING.md（英文）、SECURITY.md 与 LICENSE/NOTICE；.github/ 下有英文 issue/PR 模板。
