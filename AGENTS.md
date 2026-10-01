# AGENTS.md — quota-status 项目工作区

## 项目定位

目标产物 `quota-status.exe`：Rust 静态单二进制，作为 Kimi Code CLI 的 statusline command（`~/.kimi-code/tui.toml` `[status_line]`），在 footer 第 1 行显示 Kimi For Coding 套餐额度（5h/week/month + reset 时间）。渲染路径只读本地缓存，缓存过期（TTL 60s）时派生 detached 子进程拉取 `/usages` 回填。

**当前阶段：设计/评审完成，尚未写任何代码**（无 Cargo.toml）。实现从 PLAN.md 的 P0 开始，严格按 P0→P5 顺序推进。

## 权威文档（动手前必读）

- **docs/SPEC.md**（v1.1）——唯一契约事实来源。改行为前必读：§3 宿主契约、§5–6 数据层与解析防御规则、§7 渲染与降级、§10 测试契约、§11 工程约束、§12 风险。
- **docs/PLAN.md**（v1.1）——P0–P5 分阶段计划（任务清单、可执行验收标准、依赖顺序），与 SPEC 冲突时以 SPEC 为准。
- **docs/REVIEW.md**——第二轮独立复核报告（历史记录；其中 8 项发现已逐条仲裁并修订进 SPEC/PLAN v1.1，勿据旧表述回改）。

## 硬约束（违反即返工）

- 渲染路径禁网络、禁重 IO（<10ms 预算；宿主 300ms 硬超时，每次渲染 spawn 一个进程）；取数只走 detached 子进程 + 原子写缓存。
- 不引入 tokio/async；HTTP 用阻塞式 `ureq`（+rustls）；release profile：`opt-level="s"`、`lto`、`codegen-units=1`、`strip`、`panic="abort"`，目标 Windows x64 静态 CRT 单 exe 3–5MB（SPEC §11）。
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

- golden parity：对齐 `repos/kimi-planbar-tui/go/testdata/golden/quota-*.txt` 逐字节（`--test-fetch` JSON 输出、CRLF 归一化；month 为本项目新增字段，新 case 放本仓库 testdata，绝不回写参考仓库）。
- 真机验收六条见 SPEC §10.4（活跃会话 1 分钟更新、断网 LKG、非 OAuth 账号降级、tui.toml 重写演练）。
- P0 建成后可用命令：`cargo build --release`、`cargo test`（当前仓库无构建系统）。

## 约定

- 文档与代码注释：中文，技术术语保留 English。
- 仓库根即未来 cargo 工程根（PLAN P0）；创建工程时核对 `.gitignore` 补 `target/`。
- 设计文档集中在 docs/（SPEC/PLAN/REVIEW）；根目录仅保留 AGENTS.md 与 P5 交付的 README.md。
