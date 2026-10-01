# AGENTS.md — quota-status 项目工作区

## 项目定位

目标产物 `quota-status.exe`：Rust 静态单二进制，作为 Kimi Code CLI 的 statusline command（`~/.kimi-code/tui.toml` `[status_line]`），在 footer 第 1 行显示 Kimi For Coding 套餐额度（5h/week/month + reset 时间）。渲染路径只读本地缓存，缓存过期（TTL 60s）时派生 detached 子进程拉取 `/usages` 回填。

**当前阶段：P0–P5 实现 + 三轮 review + 实装验收全部完成**（2026-10-01）。第三轮独立 code review 发现 2 Major（console CONOUT$ 句柄 access=0 致宽度查询恒失效、缓存新鲜判定把「可解析」相与进来架空 §4.4 防风暴）+ 3 Minor，已全部修复并补回归单测（SPEC bump v1.3）。cargo 工程即仓库根，产物 `target/release/quota-status.exe` ≈1.7MB 静态单 exe。2026-10-01 晚实装完成：exe 部署至 `~/.kimi-code/bin/quota-status.exe`（脱离 target/，重新 build 后需重新复制），tui.toml `[status_line] command` 已配置并生效；真机验收 §10.4 条 3 通过（footer 第 1 行正常显示、活跃会话内缓存 fetchedAt 连续推进、CONOUT$ 宽度查询真机复核正常），条 6 本轮未触发（验收期间 host 未重写 tui.toml）。排查步骤见 README「故障排查」，验收明细见 README「验收记录」。

## 实现落定要点（与参考实现/SPEC 旧文本的差异，均已过 review）

- lib/bin 双目标：逻辑全在 `src/lib.rs`（供 `tests/golden.rs` 集成测试导入解析函数），`src/main.rs` 只做 argv 分发。
- credentials 的 config.toml 逐行扫描为**手写解析**而非 regex（regex 不在 SPEC §11 依赖白名单），语义由移植单测钉死；匹配串已扩展为同时含 `api.kimi.ai/coding`（SPEC §5.2 勘误增强）。
- 原子写 tmp 带 PID 后缀（`quota-status.json.<pid>.tmp`），消除并发 refresh 写同一 tmp 的竞态窗口。
- 缓存新鲜判定为**纯 mtime**（SPEC §4.1 步骤 3 v1.3 钉死，Python `maybe_refresh` 同款）：与缓存是否可解析无关，空锚定/损坏/refresh 持续失败时靠回拨 mtime 压住派生风暴；锚定用 `create_new` 原子语义（绝不截断已有缓存），refresh 启动时清理 >60s 的孤儿 tmp。
- month 段：limit 缺失/0/NaN 均不产生段；booster 开启时显示 `boost 余额`（纯 ASCII，避免 ¥ 在非 UTF-8 终端的兼容性问题）。
- 错误分类：超时（含 body 阶段）→ `TaskCanceledException`；非 2xx 与其余传输错误 → `HttpRequestException`；body 阶段非超时错误（连接 reset、非法 UTF-8）→ `JsonException`（SPEC §5.1 v1.3 钉死）。

## 权威文档（动手前必读）

- **docs/SPEC.md**（v1.3）——唯一契约事实来源。改行为前必读：§3 宿主契约、§5–6 数据层与解析防御规则、§7 渲染与降级、§10 测试契约、§11 工程约束、§12 风险。
- **docs/PLAN.md**（v1.3）——P0–P5 分阶段计划（任务清单、可执行验收标准、依赖顺序），与 SPEC 冲突时以 SPEC 为准；文头有完成状态标注。
- **docs/REVIEW.md**——第二轮独立复核报告（历史记录；其中 8 项发现已逐条仲裁并修订进 SPEC/PLAN，勿据旧表述回改）；**docs/REVIEW3.md**——第三轮实现后 code review 报告（2 Major/3 Minor 已全部修复，复核结论"可交付"）。

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

- 常用命令：`cargo build --release`（静态单 exe）；`cargo test`（45 个单元测试）；`cargo test --test golden`（33 个 golden 逐字节 parity——golden 的 datetime 偏移固定 +08:00，测试内有时区 fail-fast，须在 UTC+08:00 机器上跑）；自检：`target/release/quota-status.exe --test-fetch`（headless 不写缓存，`error == null` 即链路正常，无凭证输出 `"error": "no-token"`）。发版：打 tag `v*` 推送即触发 `.github/workflows/release.yml`（全量测试 + 构建 + 自动建 GitHub Release 挂 exe/sha256）。
- golden parity：33 个 case 已全部内化 `testdata/golden/`（2026-10-01 从参考仓库逐字节复制 30 个既有 case，CI 自足；month 3 个原生），逐字节比对 + CRLF 归一化；参考仓库仍为上游事实来源，新增 case 绝不回写。
- 真机验收六条见 SPEC §10.4；验收记录（含 2026-10-01 晚实装验收：footer 显示与活跃会话 1 分钟自动更新已通过、`/theme` 演练未触发）与偏差记录（体积 1.7MB 低于预估下限 3–5MB）见 README「验收记录」。

## 约定

- 文档与代码注释：中文，技术术语保留 English。
- 仓库根即 cargo 工程根（P0 落地：`Cargo.toml`、`.cargo/config.toml`、`src/` lib+bin 双目标、`tests/golden.rs`、`testdata/golden/`；`.gitignore` 已含 `target/`）。
- 设计文档集中在 docs/（SPEC/PLAN/REVIEW）；根目录仅保留 AGENTS.md、README.md（P5 交付）与 LICENSE/NOTICE。
