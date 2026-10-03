# Changelog

本仓库的显著变更记录于此。格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

> 英文译本：[CHANGELOG.en.md](CHANGELOG.en.md)（随版本同步；两文不一致时以本文为准）。

## [Unreleased]

### 依赖

- windows-sys 0.60 → 0.61、toml 0.9 → 1.1、actions/checkout v4 → v7（Dependabot 首批自动升级；semver-major 项代码零改动通过全量门禁，Cargo.lock 净减 95 行）

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
