# Changelog

本仓库的显著变更记录于此。格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

## [Unreleased]

### 新增

- 单色渲染开关：`quota-bar.toml [render] colors = false` 时输出纯文本（无任何 SGR），整行由宿主包装为主题 text 色——与 footer 第 2 行 context 读数同色并随 `/theme` 联动（SPEC v1.4 §7.2/§7.4/§8）

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
