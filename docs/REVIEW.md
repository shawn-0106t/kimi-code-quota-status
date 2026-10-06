# quota-status SPEC/PLAN 第二轮独立复核报告

- 复核对象：`SPEC.md` v1.0（2026-10-01）、`PLAN.md` v1.0（2026-10-01）
- 复核日期：2026-10-01
- 复核性质：第二轮独立复核，证伪导向（只读；未编辑/创建任何被审文件与参考仓库文件）
- 方法：通读 SPEC（409 行）与 PLAN（296 行），提取全部可证伪论断 70+ 条，分三路对 `repos/kimi-code`、`repos/kimi-planbar-tui`、`repos/kimi-planbar` 逐条读源码取证，再做两文档内部一致性判定
- 路径约定：本文所有 `path:line` 引用相对本仓库根

> **English abstract** — Second-round independent review report (2026-10-01) covering SPEC v1.0 and PLAN v1.0, conducted read-only and falsification-oriented. It spot-checked 23 citations against the referenced source repositories and raised 8 findings (1 high, 2 medium, 5 low); the most significant was that the host renders event-driven rather than polling about once per second, which reshaped the refresh acceptance criteria. This document is a historical record: all 8 findings were subsequently arbitrated and their revisions folded into SPEC and PLAN, so the wording here no longer reflects the current contract and must not be used as a basis for changing code.
>
> *Note: internal development document written in Chinese; the Chinese body below is authoritative and is not fully translated.*

---

## 复核清单逐项结论

| # | 项目 | 结论 |
|---|---|---|
| 1 | 事实性（≥15 处引用抽查） | **fail（1 处语义误读 + 1 处无出处断言 + 若干引用小疵）**，详见抽查记录与 findings |
| 2 | 需求覆盖（4 条原始需求） | **conditional pass**：需求 2/3/4 完整覆盖；需求 1 有契约有验收，但建立在错误的宿主行为前提上（F1） |
| 3 | SPEC↔PLAN 一致性 | **pass（1 处数字不一致）**：SPEC 各章均有 PLAN 阶段落点；验收标准均带具体命令或可观察判据；唯 PLAN P3 "四开关" 与 SPEC §8 五键不符（F4） |
| 4 | 实现风险 | **pass**：P0→P5 依赖链成立；无 tokio 白名单可满足 detached spawn（std `CommandExt::creation_flags`）+ 8s 阻塞 HTTP（ureq timeout）；Windows 三约束（UTF-8 stdout→P3、CREATE_NO_WINDOW\|DETACHED_PROCESS→P2 console.rs、tmp+rename→P2 cache.rs）均有任务落点 |
| 5 | 内部一致性 | **基本 pass**：TTL 60/RETRY 30/8s/300ms/1s/64KB/3-5MB、颜色码、golden 计数（30/37）、路径引用两文档互洽；SPEC §5.3 示例与 golden 实证格式逐字段吻合（`fetchedAt: 2030-01-01T08:00:00.123456789+08:00` = atFracs 1893456000000ms+123456789ns @ +08:00，浮点 `21.0`、`"error": null` 均一致） |

---

## 引用抽查记录（23 处）

### 宿主契约（repos/kimi-code）

1. SPEC:14 超时 300ms → `status-line-command.ts:13` `STATUS_LINE_COMMAND_TIMEOUT_MS = 300` ✅
2. SPEC:63/99 节流 1s → `status-line-command.ts:14` `= 1_000` ✅；":139-149 pending 延迟落地不丢弃" 与实际代码逐字吻合 ✅
3. SPEC:100 64KB → `status-line-command.ts:15` `= 65_536` ✅；:84-95 首个换行截断 ✅
4. SPEC:86-89 进程契约 → `status-line-command.ts:46`（`%ComSpec%` 兜底 cmd.exe、`/d /s /c`）、`:47` stdio `['pipe','pipe','ignore']`、`:48` `KIMI_CODE_STATUS_LINE: '1'`、`:113` `child.stdin?.end(JSON.stringify(payload))` 全中 ✅
5. SPEC:90-91 → `status-line-command.ts:106` `stdout.split('\n')[0].trimEnd()`、`:107` 空首行 null、`:100-104` 非 0 失败 ✅
6. SPEC:98 → `status-line-command.ts:58-78` `taskkill /pid <pid> /T /F` ✅
7. SPEC:101 last-good → `status-line-command.ts:174-179` 失败不触碰缓存 ✅
8. SPEC:107 → `footer.ts:311-318` 首行接管 + `:318` `chalk.hex(colors.text)(customLine)` ✅
9. SPEC:108 第 2 行 → `footer.ts:366-395`/`177-183` context 在右侧属实，但左侧描述不精确 ❌→F5
10. SPEC:112 payload schema → `status-line-command.ts:17-28` 十字段逐字吻合（不多不少）✅；`footer.ts:509` model 取 displayName ✅
11. SPEC:80-82 启用方式 → `config.ts:238`/`:287-290`/`:311`、`registry.ts:303-309`、`dispatch.ts:512-513` ✅
12. SPEC:103 "调用频率约 1 次/秒" → **无源码支持且被证伪** ❌→F1

（以上文件均位于 `repos/kimi-code/apps/kimi-code/src/tui/` 下：utils/status-line-command.ts、components/chrome/footer.ts、config.ts、commands/registry.ts、commands/dispatch.ts）

### 数据层（repos/kimi-planbar-tui）

13. SPEC:50-56 资产行数 → `wc -l` 实测 credentials.rs=147、http.rs=16、quota.rs=374、polling.rs=74、quota-status.py=227、docs/SPEC.md=694 全中 ✅
14. SPEC:182-188 凭证链 → `rust/src/credentials.rs:35-42/44-66/49-60/62-113` 全中 ✅；但兜底匹配串实测仅 `"api.kimi.com/coding"`（:106-107），不含国际站 ❌→F3
15. SPEC:219-228 防御规则 → `rust/src/quota.rs:135-150/154-176/180-183/188-193/204-220/222-233` 全中 ✅（§6.3/§6.8 引用小疵 →F7）
16. SPEC:222 → `quota.rs:115-123` 确实只取 `limits[0]`（全文无 window 匹配）✅；`quota-status.py:77-82` window 匹配语义逐字吻合 ✅
17. SPEC:224 月段 → `quota-status.py:93-97` `totalQuota` 有 limit 才产生 ✅；`quota.rs` grep `totalQuota` 零命中 ✅
18. SPEC:228 错误类型名 → `quota.rs:77-105` 四种齐全（`no-token` 在 :78）✅
19. SPEC:62 勘误 1 → `repos/kimi-planbar/go/testdata/golden/` 不存在 ✅；`repos/kimi-planbar-tui/go/testdata/golden/` 37 文件、quota-\* 恰 30、ts/ts-nodejs 拷贝存在 ✅
20. SPEC:368-372 parity 机制 → `go/internal/core/quota_test.go:206-293`/`:240-266`/`:14-15`/`:272-275`、`go/internal/core/goldens_test.go:23-25` 全中 ✅
21. SPEC:176 "国际站 api.kimi.ai" 与 :180 "纯 API key 账号返回 404" → 在被引用的 `repos/kimi-planbar-tui/docs/SPEC.md` §16.1 中 grep 均无出处 ❌→F3 附注

### 渲染参考（repos/kimi-planbar/quota-status.py）

22. SPEC:272 颜色依据 → `py:21/195/202/111/119/222/224` 全中 ✅（阈值 `>= 85`/`>= 60` 逐字吻合）
23. SPEC:407 风险① 引 `config.ts:298-312` → 引用位置真实存在，但语义被反向误读 ❌→F2

---

## Findings

### F1 — severity: high

- **where**：SPEC.md:103（§3.3 推论）、SPEC.md:384（§10.4 条 3）、PLAN.md:243（P5 验收 3）
- **problem**："渲染进程被调用频率约 1 次/秒""额度数据 1 分钟内自动更新"预设宿主周期性重渲染，但宿主是纯事件驱动——空闲会话完全不调用 command，需求 1 的"1 分钟刷新"承诺和验收只在活跃会话成立。
- **evidence**：`maybeRefresh` 全仓库唯一调用点在 footer 渲染内（`footer.ts:312`）；`kimi-tui.ts` grep `setInterval` 零命中；tips 轮转无定时器（`footer.ts:82-84` 仅墙钟取模）；唯一周期器是 goal 活动时的 1s timer（`footer.ts:529-535`）；空闲（无输入/无流式/无活动 goal）时 footer 不重渲染。
- **fix**：SPEC §3.3 推论改为"至多 1 次/秒、事件驱动、空闲不刷新；活跃会话中额度 ≤60s+一次渲染落地"；§10.4 条 3 与 PLAN P5 验收 3 加前提"活跃会话（流式/输入/goal 活动）"，并补"空闲后恢复活动数秒内自愈"判据；§12 建议新增此限制条目。

### F2 — severity: medium

- **where**：SPEC.md:407（§12 风险①）、SPEC.md:353（§9）、SPEC.md:387（§10.4 条 6）、PLAN.md:229/245（P5 README 任务与验收 5）
- **problem**："0.31.1 在 /theme 或升级后整体重写 tui.toml、静默丢 [status_line]" 被其自引用代码证伪——重写属实，但 [status_line] 的 command/items 值会 round-trip 存活；被丢的只是段内注释与未知键。
- **evidence**：`config.ts:299-303` 注释逐字："An active status_line must round-trip: any preference save rewrites the whole file, so the section is emitted live when set"；`:307-312` 重写时重新 emit 两键值（`:311` 用 `escapeTomlBasicString`）；整文件重写机制在 `saveTuiConfig` `:222-228` + `renderTuiConfig` `:298-354`；schema `z.object({items?, command?})`（`:41-44`）剥离未知键。"升级后重写"无代码证据。
- **fix**：风险①改写为"重写丢失段内注释/未知键，command 值本身保真（config.ts:299-312）"；§10.4 条 6 与 P5 验收 5 改为验证 command 存活，或删除该演练。

### F3 — severity: medium

- **where**：SPEC.md:176（§5.1）↔ SPEC.md:187（§5.2 步骤 2）
- **problem**：§5.1 声明支持国际站 base_url（api.kimi.ai），但凭证链兜底只匹配字面量 `api.kimi.com/coding`——纯 API key 的国际站用户拿不到 token，SPEC 如实收录源码语义却未声明该缺口；另 §5.1 的"国际站 URL"与"纯 API key 账号返回 404"两句在上游 SPEC §16.1 无出处（行为上 404 归非 2xx 处理不变，但属无来源断言）。
- **evidence**：`credentials.rs:106-107` 逐字：`if s.starts_with("providers.") && b.contains("api.kimi.com/coding") && !k.is_empty()`；上游 `repos/kimi-planbar-tui/docs/SPEC.md` 全文 grep 无 `api.kimi.ai`、无 `404`。
- **fix**：实现时匹配串扩展为同时含 `api.kimi.ai/coding`，或在 §5.1/§5.2 注明"国际站仅 OAuth 凭证可用"；两句无来源断言补出处或弱化措辞。

### F4 — severity: low

- **where**：PLAN.md:145（P3 任务 1）"[render.quota] 四开关" vs SPEC.md:314-319（§8）[render.quota] 五键（five_hour/week/month/reset_time/booster）
- **problem**：数字不一致（PLAN:4 自述"冲突时以 SPEC 为准"，影响有限）。
- **fix**：改"五键"或逐键列出。

### F5 — severity: low

- **where**：SPEC.md:108（§3.4）
- **problem**："第 2 行内容为 ctrl+o 提示（左）+ context（右）" 不精确：ctrl+o 仅在 command 接管第 1 行时才下移到第 2 行，左侧还优先给 transient/warning 提示；且 tokens 经 1024 进制缩写格式化而非原始数字。在本工具运行形态下描述碰巧成立，关键决策（contextTokens 不进自定义行）不受影响。
- **evidence**：`footer.ts:386-388`（"A status_line.command owns line 1 outright, so the ctrl+o hint moves down here; the transient and warning hints above take precedence"）、`:389-392`（原生布局第 2 行左为空）、`:177-183` + `formatTokenCount`（1024 进制缩写）。
- **fix**：改为"command 接管第 1 行时，第 2 行左侧容纳 ctrl+o 提示（transient/warning 优先）、右侧 `context: N% (缩写tokens/max)`"。

### F6 — severity: low

- **where**：SPEC.md:276（§7.3）"percent 按 %.0f 语义取整（四舍五入到整数）"
- **problem**：%.0f 实为 round-half-to-even（银行家舍入），exact .5 时与"四舍五入"相反（如 60.5→60）；PLAN.md:155 已正确写 "%.0f 语义"。
- **evidence**：`quota-status.py:31` `f'...{p:.0f}%...'`；Python/Rust 的 `.0f` 均为 half-to-even。
- **fix**：SPEC 改"round-half-to-even，边界由单测钉死"（PLAN 无需改）。

### F7 — severity: low（引用精度打包）

- **where**：SPEC.md:221（§6.3）、:225（§6.7）、:226（§6.8）
- **problem**：① §6.3 把 i64::MIN 敌意输入与 `saturating_add` 绑在同一语境，实际 `saturating_add` 护的是 amountLeft 近 i64::MAX 正溢出（quota.rs:229-230 注释逐字），i64::MIN 用例是 monthly `priceInCents`（quota.rs:363-373）；② §6.7 "可带小数秒、T 或空格分隔" 过宽——小数秒仅"空格分隔+%:z"变体支持（quota.rs:161-163），naive 格式无 `%.f`（:168-174）；③ §6.8 所引 `quota_test.go:137-141` 只覆盖字符串 "false"，数字 0 用例实际在 `quota_test.go:255` 内联 payload。
- **fix**：三处按实际代码措辞修订。

### F8 — severity: low

- **where**：PLAN.md:125-126（P2 验收 2/3）
- **problem**：验收计时脆弱：验收 2 "等待约 10s" 未声明需 managed OAuth 已登录前提，且 8s HTTP 超时+进程启动+TLS 慢网下可超 10s；验收 3 渲染后立即查 `tasklist` 无残留——断网下 detached refresh 可存活至 8s 超时，立即检查可能误报。
- **fix**：验收 2 加注前提并放宽至 ≥15s 或轮询；验收 3 改"等待 >8s 后再查 tasklist"。

---

## 附带确认（无需修）

- PLAN P1 风险 2 担心的"window 匹配升级导致 golden byte 差异"实测不会发生——`quota_test.go:241-265` 全部内联 payload 的 limits 数组均恰 1 元素，golden 集无法区分两种实现（这也意味着 window 匹配只能靠 PLAN P1 新增单测覆盖，parity 管不到，分工正确）。
- PLAN P0 的 `.gitignore` 含 `target/` 假设实测不成立（当前根 `.gitignore` 无 target/），但任务本身写了"核对补齐"，自洽。
- 宿主 stdout 解码为 utf8（`status-line-command.ts:82` `child.stdout?.setEncoding('utf-8')`），与 SPEC §7.6 强制 UTF-8 输出的决策吻合。
- golden JSON 实证格式：camelCase 键、2 空格缩进、fetchedAt 带时区偏移、纳秒非零时 9 位、为零时省略、`"error": null` 显式存在、浮点输出 `21.0`/`18.5`；与 SPEC §5.3 示例逐字段一致。
- ts 侧 golden 与 go 侧内容相同但行尾为 LF（go 为 CRLF）——印证 PLAN P4 CRLF 归一化的必要性。

---

## 总评

两份文档引用准确率约 95%、SPEC↔PLAN 映射完整、工程拆解可执行，P0 工程骨架本身不依赖任何错误条目、可以先行启动；但 F1（宿主事件驱动被误判为约 1 次/秒轮询）动摇了需求 1 的验收前提，F2/F3 分别为与所引代码相反的风险断言和国际站凭证缺口——建议先修订 SPEC §3.3/§10.4/§12/§5.1-5.2 并同步 PLAN 对应验收条目，再按 PLAN 全流程推进，不宜带病直接进入实现。
