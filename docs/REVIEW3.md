# quota-status 第三轮独立 code review 报告（实现后审查）

- 审查对象：P0–P5 实现（`src/` 全部 9 文件、`tests/golden.rs`、`testdata/golden/`、`Cargo.toml`、`.cargo/config.toml`）
- 审查日期：2026-10-01
- 审查性质：交付级独立 code review，证伪导向（只读；含 `cargo build --release` / `cargo test` / `--test-fetch` / PowerShell P/Invoke / mtime 观测实证）
- 契约基线：`docs/SPEC.md` v1.2（初审时）；发现的修订已落入 SPEC v1.3 与代码
- 执行方式：初审与修复后复核各一轮，均由独立 code-reviewer agent 执行

---

## 初审发现与处置（0 Critical / 2 Major / 3 Minor / 5 Info）

| # | 级别 | 位置 | 问题 | 处置 |
|---|------|------|------|------|
| 1 | Major | `src/console.rs` | `CreateFileW("CONOUT$", 0, ...)` 零权限句柄致 `GetConsoleScreenBufferInfo` 恒 ERROR_ACCESS_DENIED（已 P/Invoke 实证），§7.5 宽度降级在真机从未生效，窄终端行被宿主硬截 | 已修：`dwDesiredAccess` 改 `GENERIC_READ`（0x8000_0000）；补「附着 console 时查询必须成功」回归单测（headless 跳过） |
| 2 | Major | `src/cache.rs` | `refresh_if_stale` 新鲜判定把「数据可解析」与 `age < TTL` 相与，空锚定/损坏缓存/refresh 持续失败（无 token、纯 API key 404）时每次渲染都派生进程（已实证 mtime 每次重新回拨），架空 §4.4 防风暴；SPEC §4.1/§4.4 与 Python 原型 `maybe_refresh` 均为纯 mtime 判定 | 已修：改纯 mtime 判定；两个受影响单测按正确语义改写并补「连续渲染只派生一次」断言（在旧代码下会失败，为真回归测试） |
| 3 | Minor | `src/cache.rs` | `ensure_anchor_at` 的 `exists()` + `File::create` 有 TOCTOU 截断窗口（并发 rename 落盘会被截断为空） | 已修：`create_new` 原子语义，AlreadyExists → Ok，绝不截断 |
| 4 | Minor | `src/cache.rs` | PID 后缀 tmp 在进程被杀后永久残留 | 已修：`--refresh` 启动时清理 mtime >60s 的孤儿 tmp（在途 tmp 存活期 <1s，不误删） |
| 5 | Minor | `src/quota.rs` | body 阶段非超时错误归 `JsonException` 与 AGENTS.md「其余传输错误 → HttpRequestException」表述有张力（参考实现支持代码现状，属契约表述歧义） | 已修：SPEC §5.1 v1.3 按 send/body 阶段细分钉死；AGENTS.md 同步 |
| 6 | Info | `src/main.rs` 等 | `env::args()` 非 Unicode argv panic；OnceLock timeout 仅首次生效；`dwSize.X as u32` 负值符号扩展；order 重复字段重复渲染 | 已修：`args_os`；注释钉死；`try_from`；去重（保留首次出现）+ 单测 |

## 复核结论（修复验证轮）

- 6 项修复**全部正确落实**，0 Critical / 0 Major，总体结论**可交付**（含隔离 `KIMI_CODE_HOME` 端到端冒烟、ctypes 佐证 GENERIC_READ 契约、零编译警告）。
- 复核追加的 4 条小项已随手修订：AGENTS.md/README.md 单测计数（40/41 → 45）、SPEC §5.1 括注过度声称改为「非超时部分与参考实现一致；body 超时属有意偏差」、锚定测试如实标注为 characterization test、补 console 权限回归单测。
- 记录备查（无需修改）：mtime 在未来时钟下 Rust 按过期处理（自愈），Python 原型按新鲜处理（停摆），属既有行为差异，SPEC 未规定该边界。

## 待真机终验

- §7.5 宽度降级在真实 console 的终验（Major 1 修复点，headless 无法覆盖；本机 ctypes 已从 OS 契约层面佐证修复有效）——随 SPEC §10.4 条 3/6 一并在用户 CLI 会话确认。
