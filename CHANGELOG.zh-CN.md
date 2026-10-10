# Changelog

本仓库的显著变更记录于此。格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

> 英文版（权威）：[CHANGELOG.md](CHANGELOG.md)（本文为随版本同步的中文译本，不一致时以英文版为准）。

## [1.2.0] - 2026-10-10

### 新增

- 面向全球用户的贡献者基建：英文 `CONTRIBUTING.md`（构建/测试门禁、golden parity 的 UTC+08:00 时区要求、golden 数据政策、文档同步义务）、GitHub issue 模板（`.github/ISSUE_TEMPLATE/`）与 PR 模板、`rust-toolchain.toml` 钉贡献者工具链（CI 仍用最新 stable；Cargo.toml 的 MSRV 不变）
- Cargo 包元数据：`repository`、`keywords`、`categories`（为 crates.io 可发现性铺路）
- `CODE_OF_CONDUCT.md`：Contributor Covenant v2.1，上英下中单文件；行为准则违规举报走 GitHub 私密联系维护者
- 经 `build.rs` + `winresource`（仅构建期依赖，SPEC §11 白名单收录）嵌入 Windows VERSIONINFO 版本资源：exe 文件「详细信息」现在显示 FileVersion/ProductVersion（取自 CARGO_PKG_VERSION）、ProductName、FileDescription 与版权——`tag = Cargo.toml = exe 属性`保持一致；并新增 `--version` flag（SPEC §4.5）输出 `quota-status <版本>`，不读 stdin、无其他 IO（体积增加约 12KB，远在 5MB 预算内）

### 变更

- i18n pass：`README.md` / `CHANGELOG.md` 改为英文为权威，中文译本迁至 `README.zh-CN.md` / `CHANGELOG.zh-CN.md`（随版本同步）；全部源码注释、doc comment 与开发者向测试/断言消息译为英文；内部开发文档（PLAN/REVIEW/REVIEW3/HANDOFF）文头补 English abstract（正文仍中文）；docs/SPEC 仍中文为权威、`SPEC.en.md` 随版本同步。纯文档/注释层变更，无行为变化。
- `SECURITY.md` 由逐段中英混排改为「英文全文在上 + 中文全文在下」单文件结构（内容不变，纯排版调整）
- `CONTRIBUTING.md` 在英文全文之下补全量中文译本，采用与 `SECURITY.md` 相同的「英文在上」单文件结构（英文段不变、仍为权威）
- issue 模板补一行指导注释：报告可用中文撰写（GitHub 在编辑时显示模板注释、提交时自动剥离）

### 依赖

- windows-sys 0.60 → 0.61、toml 0.9 → 1.1、actions/checkout v4 → v7（Dependabot 首批自动升级；semver-major 项代码零改动通过全量门禁，Cargo.lock 净减 71 行）

### 安全

- CI `ci.yml` 的 `test` job 补显式最小权限 `permissions: contents: read`——修复 CodeQL default setup 首扫 medium 告警 `actions/missing-workflow-permissions`（2026-10-03）；至此 `ci.yml` 两 job 与 `release.yml`（workflow 级 `contents: write`）均显式声明最小权限，GITHUB_TOKEN 不再回落 repo 默认权限

## [1.1.0] - 2026-10-02

### 新增

- tasks/agents 徽章（SPEC v1.5 §7.7）：footer 第 1 行在额度组之前显示当前会话的后台任务计数——bash 后台任务 `[N task(s) running]`（cyan，经 `OpenProcess` + `GetExitCodeProcess` pid 存活校验）、后台 subagent `[M agent(s) running]`（running 即计入），两者皆零整段省略；数据源为 `<kimi_home>/sessions/` 扫描探测（workspace 探测 ≤64、任务 json 读取 ≤32，超限截断计数），sessionId 路径穿越防御 + 失败即省略，不阻塞渲染；默认 order 补入 `"tasks"`，从 order 删去即关闭整段并跳过扫描，不新增配置键
- 单色渲染开关：`quota-bar.toml [render] colors = false` 时输出纯文本（无任何 SGR），整行由宿主包装为主题 text 色——与 footer 第 2 行 context 读数同色并随 `/theme` 联动（SPEC v1.4 §7.2/§7.4/§8）

### 修复

- tasks 徽章失败语义对齐契约（SPEC §7.7 决策 E / §9）：任务 json 读取/解析失败由「跳过该文件、保留部分计数」改为短路返回零计数（段整体省略），对应单测断言方向反转并补空态回归（二次独立 code review Major，经用户仲裁修代码对齐契约）；`agents/<id>/tasks/` 目录不存在按空态跳过（非失败），其余 IO 错误同样段整体省略
- 补 M-1 确定性回归单测（三次 review Minor-1）：好任务按名称序先于坏文件被计数 + 跨 agent 省略用例，确定性钉死「短路时丢弃已积累计数」

### 文档

- SPEC v1.5.1：§7.7 决策 B 补记 sessionId 字符白名单语义（实现自 P7 review 加固起即如此，文本同步无行为变更）、决策 E 补记 NotFound 空态豁免、§10.1 补失败语义与空态两单测项；PLAN v1.5.1：P7「涉及文件」补录 src/lib.rs、src/main.rs；HANDOFF §2 单测拆分勘误（13/5）+ §7 附记（二次 review 经过与验收标准 5 通过）；三次 review Nit 收尾：SPEC 决策 E 补 flatten 条目级错误豁免、PLAN 契约依据 bump v1.5.1、README 门禁行标注首轮时点

## [1.0.0] - 2026-10-01

首个正式版。

### 新增

- Kimi Code CLI statusline command：footer 第 1 行显示 Kimi For Coding 套餐额度（5h / week / month 百分比 + reset 时间）
- 三模式进程模型：渲染模式（只读本地缓存，毫秒级退出，禁网络）/ `--refresh` 取数模式（detached 子进程后台回填）/ `--test-fetch` 自检模式（headless 不写缓存）
- `~/.kimi-code/quota-bar.toml`：字段开关与行内顺序、额度子段开关、颜色阈值、缓存 TTL、base_url 覆盖
- 渲染行字段：permissionMode（yolo/auto/manual 配色）、model、thinking 级别、额度组（绿/黄/红阈值）、gitBranch
- 缓存机制：TTL 60s + mtime 回拨防刷新风暴 + 30s fast-retry + LKG 保留；PID 后缀 tmp + rename 原子写
- 防御式解析（SPEC §6）：数字按字符串建模兼容真数字、limit 钳位防除零、NaN/敌意整数归零、resetTime 宽松阶梯、boosterWallet `isEnabled` 防御
- 宽度感知降级阶梯：丢 reset 后缀 → 丢 gitBranch → 只留额度组 → 宿主截断兜底
- golden parity 测试：33 case 逐字节对齐（30 个内化自 kimi-planbar-tui + 3 个月度原生 case）
- GitHub Actions CI（fmt/clippy 门禁 + 单元 + golden + release 构建 + 5MB 体积门禁）与 tag `v*` 触发的 Release workflow（自动挂 exe + sha256）
- MIT LICENSE + NOTICE（kimi-planbar / kimi-planbar-tui 派生归属）

### 修复

- 第三轮独立 code review 的 2 Major + 3 Minor：console `CONOUT$` 句柄 access=0 致宽度查询恒失效；缓存新鲜判定把「可解析」相与进来架空防风暴回拨（明细见 docs/REVIEW3.md）

[1.0.0]: https://github.com/shawn-0106t/kimi-code-quota-status/releases/tag/v1.0.0
