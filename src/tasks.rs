// tasks/agents 徽章数据源（SPEC §7.7 v1.5）：扫描 <kimi_home>/sessions/
// 定位 payload sessionId 对应会话目录，读取 agents/*/tasks/*.json，
// 按 kind 分流计数 running 任务。全程防御式：任何失败零计数（段省略），
// 绝不 panic、绝不阻塞渲染（决策 E）；扫描上限截断计数而非失败（决策 D）。

use serde_json::Value;
use std::path::Path;
use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};

/// workspace 探测上限（SPEC §7.7 决策 D）
const MAX_WORKSPACE_PROBES: u32 = 64;
/// 任务 json 读取上限（SPEC §7.7 决策 D）
const MAX_TASK_READS: u32 = 32;
/// 单个任务 json 大小护栏：宿主写的任务 json 恒为字节级小文件，超大文件
/// 视为格式漂移跳过（决策 D 精神的单文件维度护栏，防病态文件拖垮热路径）
const MAX_TASK_JSON_BYTES: u64 = 64 * 1024;

/// 计数结果：bash 侧（kind=process/question/未知/缺失，经 pid 存活校验）与
/// agent 侧（kind=agent，running 即计入）的 running 任务数（SPEC §7.7 决策 C）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TaskCounts {
    pub bash: u32,
    pub agent: u32,
}

/// payload sessionId + `<kimi_home>` -> running 任务计数（渲染行 tasks 徽章段，
/// SPEC §7.1/§7.7）。防御规则：
/// - 决策 A：sessionId 缺失/为空 -> 零计数（段省略）；sessions/ 下逐 workspace
///   目录探测 `<ws>/<sessionId>` 是否存在（sessionId 全局唯一，命中即定位）；
/// - 决策 B：sessionId 字符白名单校验（`[A-Za-z0-9_-]`，覆盖 `/`、`\`、`..`
///   与盘符相对路径等全部穿越面）不通过 -> 零计数；
/// - 决策 D：workspace 探测 <=64、任务 json 读取 <=32，超限截断按已读计数；
/// - 决策 E：sessions 目录不存在 / json 解析失败 / 任何 IO 错误 -> 零计数。
pub fn count_running(kimi_home: &Path, session_id: Option<&str>) -> TaskCounts {
    // 决策 A：sessionId 缺失/为空 -> 段省略
    let Some(sid) = session_id.filter(|s| !s.is_empty()) else {
        return TaskCounts::default();
    };
    // 决策 B：路径防御（来自宿主 payload，拼接前必须校验）
    if !is_safe_session_id(sid) {
        return TaskCounts::default();
    }
    let Ok(entries) = std::fs::read_dir(kimi_home.join("sessions")) else {
        return TaskCounts::default(); // 决策 E：目录不存在 -> 零计数
    };
    let mut probes = 0u32;
    for entry in entries.flatten() {
        let ws = entry.path();
        if !ws.is_dir() {
            continue; // 非 workspace 目录不消耗探测配额
        }
        if probes >= MAX_WORKSPACE_PROBES {
            break; // 决策 D：截断非失败
        }
        probes += 1;
        let session_dir = ws.join(sid);
        if session_dir.is_dir() {
            return count_in_session(&session_dir);
        }
    }
    TaskCounts::default() // 未命中 -> 零计数（段省略）
}

/// sessionId 形态校验（决策 B）：宿主真值为 `session_<uuid>`（RFC 4122 hex +
/// 连字符），字符白名单 `[A-Za-z0-9_-]` 一步覆盖 SPEC 三条拒绝规则（`/`、`\`、
/// `..`），并封堵残余穿越面——盘符相对路径（如 `"C:evil"` 经 `Path::join` 会
/// 替换整个 base 指向 C 盘当前目录）、裸 `"."`、UNC、Windows 保留名、尾随
/// 点/空格、非 ASCII 等。
fn is_safe_session_id(sid: &str) -> bool {
    sid.bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// 遍历 `<sessionDir>/agents/*/tasks/*.json` 计数（决策 B：仅扫 agents/*/tasks/
/// 新路径，文件名不校验、以 json 内容为准）。
fn count_in_session(session_dir: &Path) -> TaskCounts {
    let mut counts = TaskCounts::default();
    let Ok(agents) = std::fs::read_dir(session_dir.join("agents")) else {
        return counts; // 决策 E：agents 目录不存在/不可读 -> 零计数
    };
    let mut reads = 0u32;
    for agent in agents.flatten() {
        if reads >= MAX_TASK_READS {
            break; // 决策 D：预算耗尽即停止
        }
        let agent_dir = agent.path();
        if !agent_dir.is_dir() {
            continue;
        }
        let tasks = match std::fs::read_dir(agent_dir.join("tasks")) {
            Ok(t) => t,
            // 该 agent 无 tasks 目录属空态（与顶层 sessions 缺失同义）-> 跳过；
            // 其余 IO 错误属决策 E 失败 -> 短路零计数（段整体省略）
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return TaskCounts::default(),
        };
        for task in tasks.flatten() {
            if reads >= MAX_TASK_READS {
                break; // 决策 D：截断，按已读结果计数
            }
            let path = task.path();
            if !is_task_json(&path) {
                continue;
            }
            reads += 1;
            if count_task_file(&path, &mut counts).is_none() {
                // 决策 E：任务文件读取/解析失败 -> 短路返回零计数（段整体省略），
                // 不得保留已积累的部分计数（SPEC §7.7 决策 E / §9）
                return TaskCounts::default();
            }
        }
    }
    counts
}

/// tasks 目录条目是否为待读的任务 json（宿主持久化恒为 `<taskId>.json`，
/// 大小写宽容以适配 Windows 文件系统语义）
fn is_task_json(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("json"))
}

/// 单个任务 json 计数（SPEC §7.7 计数口径）：仅 `status == "running"` 参与
/// 计数（五类终态与未知 status 跳过）；`kind == "agent"`（严格相等）running
/// 即计入 agent 计数；其余一切 kind 走 pid 存活校验计入 bash 侧。
/// 返回 None 表示文件读取/解析失败（决策 E：调用方短路整段省略）。
fn count_task_file(path: &Path, counts: &mut TaskCounts) -> Option<()> {
    // 大小护栏：超大文件视为格式漂移的刻意跳过（非失败），其余文件照常计数；
    // metadata 失败按未超限处理（后续读取失败走决策 E 短路）
    if std::fs::metadata(path).is_ok_and(|m| m.len() > MAX_TASK_JSON_BYTES) {
        return Some(());
    }
    let text = std::fs::read_to_string(path).ok()?; // 决策 E：读取失败
    let v = serde_json::from_str::<Value>(&text).ok()?; // 决策 E：解析失败
    if v.get("status").and_then(|s| s.as_str()) != Some("running") {
        return Some(());
    }
    if v.get("kind").and_then(|k| k.as_str()) == Some("agent") {
        // 决策 C：agent 任务无 pid 字段（CLI 进程内异步任务），running 即计入，
        // 接受 CLI 崩溃场景的陈旧误报（宿主重启加载会自愈标 lost）
        counts.agent += 1;
        return Some(());
    }
    // 其余 kind（process/question/未知/缺失）-> bash 侧经 pid 校验；
    // question 无 pid 恒不计入（2026-10-02 仲裁：保守不计入，宁少报勿误报）
    let Some(pid) = pid_of(&v) else {
        return Some(());
    };
    if process_alive(pid) {
        counts.bash += 1;
    }
    Some(())
}

/// pid 防御（决策 C）：仅接受 JSON 正整数且 <= u32::MAX；缺失、字符串形态、
/// 负数、小数、0、超 u32 视同缺失，保守不计入。
fn pid_of(task: &Value) -> Option<u32> {
    let n = task.get("pid")?.as_u64()?;
    if n == 0 || n > u32::MAX as u64 {
        return None;
    }
    Some(n as u32)
}

/// pid 存活校验（决策 C bash 侧）：OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)
/// 与 GetExitCodeProcess；句柄 NULL（进程不存在/无权限）或 exit code !=
/// STILL_ACTIVE(259) 视为已结束。复用既有 windows-sys 依赖，零新增 crate。
fn process_alive(pid: u32) -> bool {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return false;
        }
        let mut exit_code: u32 = 0;
        let ok = GetExitCodeProcess(handle, &mut exit_code);
        CloseHandle(handle);
        // exit code == STILL_ACTIVE(259) 才视为存活（决策 C；windows-sys 的
        // STILL_ACTIVE 为 NTSTATUS = i32，GetExitCodeProcess 出参为 u32）。
        // 已知盲区：退出码恰为 259 的已退出进程会误判存活（Win32 API 固有
        // 限制，与 PID 复用窗口同类，概率可忽略）
        ok != 0 && exit_code == STILL_ACTIVE as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::process::{Command, Stdio};

    /// 隔离的临时 <kimi_home>（按 tag + 进程 id 唯一，先清残留）
    fn temp_home(tag: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!("qs-tasks-{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        base
    }

    /// 落盘一个任务 json：<home>/sessions/<ws>/<sid>/agents/<agent>/tasks/<task>.json
    fn write_task(
        home: &Path,
        ws: &str,
        sid: &str,
        agent: &str,
        task: &str,
        json: &str,
    ) -> PathBuf {
        let dir = home
            .join("sessions")
            .join(ws)
            .join(sid)
            .join("agents")
            .join(agent)
            .join("tasks");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{task}.json"));
        fs::write(&path, json).unwrap();
        path
    }

    fn task_json(status: &str, kind: &str, pid: Option<u32>) -> String {
        let pid_part = pid.map(|p| format!(r#""pid":{p},"#)).unwrap_or_default();
        let kind_part = if kind.is_empty() {
            String::new()
        } else {
            format!(r#""kind":"{kind}","#)
        };
        format!(r#"{{"taskId":"t","status":"{status}",{kind_part}{pid_part}"startedAt":1}}"#)
    }

    /// 存活进程正控：本测试进程自身必然存活（PROCESS_QUERY_LIMITED_INFORMATION
    /// 可查自身），无需派生子进程。
    fn alive_pid() -> u32 {
        std::process::id()
    }

    /// 已退出进程：spawn 后立即 exit 的 cmd，wait 归收后其 pid 不应判活。
    fn exited_pid() -> u32 {
        let mut child = Command::new("cmd")
            .args(["/c", "exit 7"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id();
        child.wait().unwrap();
        // 复用窗口极小（Windows pid 轮转分配），命中即测试自身缺陷，宁可失败
        assert!(!process_alive(pid), "刚退出的 pid 被立即复用？pid={pid}");
        pid
    }

    /// workspace 探测：sessionId 命中（SPEC §10.1 v1.5 探测命中例）
    #[test]
    fn workspace_probe_hit() {
        let home = temp_home("hit");
        write_task(
            &home,
            "wd_a",
            "session_x",
            "main",
            "t1",
            &task_json("running", "process", Some(alive_pid())),
        );
        let c = count_running(&home, Some("session_x"));
        assert_eq!(c, TaskCounts { bash: 1, agent: 0 });
        fs::remove_dir_all(&home).ok();
    }

    /// workspace 探测：sessionId 未命中 -> 零计数（段省略）
    #[test]
    fn workspace_probe_miss() {
        let home = temp_home("miss");
        write_task(
            &home,
            "wd_a",
            "session_other",
            "main",
            "t1",
            &task_json("running", "agent", None),
        );
        assert_eq!(
            count_running(&home, Some("session_x")),
            TaskCounts::default()
        );
        // sessions 目录本身缺失 -> 零计数（决策 E）
        let empty = temp_home("miss-empty");
        assert_eq!(
            count_running(&empty, Some("session_x")),
            TaskCounts::default()
        );
        fs::remove_dir_all(&home).ok();
        fs::remove_dir_all(&empty).ok();
    }

    /// 路径防御（决策 B，SPEC §10.1 v1.5 三类穿越串拒绝）：含 `/`、`\`、`..`
    /// 的 sessionId 视为无效零计数——伪造出穿越后目标目录仍必须拒绝。
    #[test]
    fn session_id_traversal_rejected() {
        let home = temp_home("traversal");
        // "../evil" 穿越目标：sessions/evil（ws 内 join(..) 可达）
        write_task(
            &home,
            "wd_a",
            "../evil",
            "main",
            "t1",
            &task_json("running", "agent", None),
        );
        // "a/b" 与 "a\\b" 穿越目标：ws 内子路径 a/b（Windows 下反斜杠 join 等价）
        write_task(
            &home,
            "wd_a",
            "a/b",
            "main",
            "t1",
            &task_json("running", "agent", None),
        );
        for sid in ["../evil", "a/b", "a\\b", "..", "a/../b"] {
            assert_eq!(
                count_running(&home, Some(sid)),
                TaskCounts::default(),
                "穿越串 {sid:?} 必须拒绝"
            );
        }
        fs::remove_dir_all(&home).ok();
    }

    /// sessionId 白名单（review Minor 1 加固）：封堵三条规则之外的残余穿越
    /// 面——盘符相对路径（join 会替换整个 base）、裸 "."、非 ASCII、空白等
    /// 一律拒绝；合法形态（宿主真值 `session_<uuid>` 的字符域）通过。
    #[test]
    fn session_id_whitelist() {
        let home = temp_home("whitelist");
        write_task(
            &home,
            "wd_a",
            "session_ok-1_X",
            "main",
            "t",
            &task_json("running", "agent", None),
        );
        assert_eq!(
            count_running(&home, Some("session_ok-1_X")),
            TaskCounts { bash: 0, agent: 1 },
            "合法字符域（字母/数字/_/-）须通过"
        );
        for sid in [
            "C:evil",
            "C:\\abs",
            ".",
            "...",
            "session ok",
            "session_x.y",
            "会话",
            "session\ttab",
        ] {
            assert_eq!(
                count_running(&home, Some(sid)),
                TaskCounts::default(),
                "白名单外 sid {sid:?} 必须拒绝"
            );
        }
        fs::remove_dir_all(&home).ok();
    }

    /// 单文件大小护栏（review Nit 4 加固）：>64KB 的任务 json 视为格式漂移
    /// 跳过（不计数、不阻断同目录其余任务）。
    #[test]
    fn oversized_task_json_skipped() {
        let home = temp_home("bigjson");
        let big = format!(
            r#"{{"taskId":"t","status":"running","kind":"agent","pad":"{}"}}"#,
            "x".repeat(70_000)
        );
        write_task(&home, "wd_a", "s", "main", "big", &big);
        write_task(
            &home,
            "wd_a",
            "s",
            "main",
            "ok",
            &task_json("running", "agent", None),
        );
        assert_eq!(
            count_running(&home, Some("s")),
            TaskCounts { bash: 0, agent: 1 }
        );
        fs::remove_dir_all(&home).ok();
    }

    /// 计数分流（决策 C）：agent running 即计入 / question 无 pid 不计入 /
    /// 未知 kind 与 kind 缺失归 bash 侧经 pid 校验。
    #[test]
    fn kind_routing() {
        let home = temp_home("routing");
        // agent：无 pid，running 即计入 agent 侧（不做 pid 校验）
        write_task(
            &home,
            "wd_a",
            "s",
            "main",
            "t_agent",
            &task_json("running", "agent", None),
        );
        // question：无 pid，归 bash 侧但 pid 缺失 -> 不计入
        write_task(
            &home,
            "wd_a",
            "s",
            "main",
            "t_question",
            &task_json("running", "question", None),
        );
        // 未知 kind / kind 缺失：归 bash 侧，pid 存活 -> 计入
        write_task(
            &home,
            "wd_a",
            "s",
            "main",
            "t_unknown",
            &task_json("running", "wat", Some(alive_pid())),
        );
        write_task(
            &home,
            "wd_a",
            "s",
            "main",
            "t_nokind",
            &task_json("running", "", Some(alive_pid())),
        );
        let c = count_running(&home, Some("s"));
        assert_eq!(c, TaskCounts { bash: 2, agent: 1 });
        fs::remove_dir_all(&home).ok();
    }

    /// 终态与未知 status 跳过（SPEC §10.1 v1.5：五类终态与未知 status 不计入）
    #[test]
    fn terminal_and_unknown_status_skipped() {
        let home = temp_home("terminal");
        for (i, status) in [
            "completed",
            "failed",
            "timed_out",
            "killed",
            "lost",
            "starting",
        ]
        .iter()
        .enumerate()
        {
            let kind = if i % 2 == 0 { "process" } else { "agent" };
            write_task(
                &home,
                "wd_a",
                "s",
                "main",
                &format!("t{i}"),
                &task_json(status, kind, Some(alive_pid())),
            );
        }
        // status 缺失同样跳过
        let dir = home.join("sessions/wd_a/s/agents/main/tasks");
        fs::write(
            dir.join("t_nostatus.json"),
            r#"{"taskId":"t","kind":"process"}"#,
        )
        .unwrap();
        assert_eq!(count_running(&home, Some("s")), TaskCounts::default());
        fs::remove_dir_all(&home).ok();
    }

    /// pid 防御（SPEC §10.1 v1.5 bash 三例）：存活计入 / 已退出不计入 /
    /// pid 缺失或非正整数（字符串、负数、小数、0、超 u32）不计入。
    #[test]
    fn pid_defense() {
        // 存活进程计入
        let home = temp_home("pid-alive");
        write_task(
            &home,
            "wd_a",
            "s",
            "main",
            "t",
            &task_json("running", "process", Some(alive_pid())),
        );
        assert_eq!(
            count_running(&home, Some("s")),
            TaskCounts { bash: 1, agent: 0 }
        );
        fs::remove_dir_all(&home).ok();

        // 已退出进程不计入
        let home = temp_home("pid-exited");
        write_task(
            &home,
            "wd_a",
            "s",
            "main",
            "t",
            &task_json("running", "process", Some(exited_pid())),
        );
        assert_eq!(count_running(&home, Some("s")), TaskCounts::default());
        fs::remove_dir_all(&home).ok();

        // 敌意 pid 形态视同缺失：字符串、负数、小数、0、超 u32、字段缺失
        let home = temp_home("pid-hostile");
        let hostile = [
            r#"{"taskId":"t","status":"running","kind":"process","pid":"123"}"#,
            r#"{"taskId":"t","status":"running","kind":"process","pid":-1}"#,
            r#"{"taskId":"t","status":"running","kind":"process","pid":1.5}"#,
            r#"{"taskId":"t","status":"running","kind":"process","pid":0}"#,
            r#"{"taskId":"t","status":"running","kind":"process","pid":4294967296}"#,
            r#"{"taskId":"t","status":"running","kind":"process"}"#,
        ];
        for (i, json) in hostile.iter().enumerate() {
            write_task(&home, "wd_a", "s", "main", &format!("h{i}"), json);
        }
        assert_eq!(count_running(&home, Some("s")), TaskCounts::default());
        fs::remove_dir_all(&home).ok();
    }

    /// agent 一例（SPEC §10.1 v1.5）：running 即计入，不做 pid 校验——
    /// 即便伪造一个已退出 pid 也照样计入（该形态本无 pid 字段）。
    #[test]
    fn agent_running_counted_without_pid_check() {
        let home = temp_home("agent-nopidcheck");
        write_task(
            &home,
            "wd_a",
            "s",
            "main",
            "t",
            &task_json("running", "agent", Some(exited_pid())),
        );
        assert_eq!(
            count_running(&home, Some("s")),
            TaskCounts { bash: 0, agent: 1 }
        );
        fs::remove_dir_all(&home).ok();
    }

    /// json 解析失败即段整体省略（决策 E 严格语义，SPEC §7.7/§9）：坏文件
    /// 存在时不得保留其余任务的部分计数——返回零计数（徽章整段省略）。
    #[test]
    fn malformed_json_omits_segment() {
        let home = temp_home("badjson");
        write_task(&home, "wd_a", "s", "main", "bad", "not-json{{{");
        write_task(
            &home,
            "wd_a",
            "s",
            "main",
            "good",
            &task_json("running", "agent", None),
        );
        assert_eq!(count_running(&home, Some("s")), TaskCounts::default());
        fs::remove_dir_all(&home).ok();
    }

    /// agent 目录缺 tasks/ 子目录属空态（非失败，决策 E 的 NotFound 豁免）：
    /// 不触发段省略，其余 agent 的计数照常。
    #[test]
    fn agent_without_tasks_dir_is_empty_state() {
        let home = temp_home("notasks");
        write_task(
            &home,
            "wd_a",
            "s",
            "main",
            "t",
            &task_json("running", "agent", None),
        );
        // 并列一个无 tasks/ 子目录的 agent（空态，枚举先后均不影响结果）
        fs::create_dir_all(home.join("sessions/wd_a/s/agents/sub_x")).unwrap();
        assert_eq!(
            count_running(&home, Some("s")),
            TaskCounts { bash: 0, agent: 1 }
        );
        fs::remove_dir_all(&home).ok();
    }

    /// 扫描上限截断（决策 D，SPEC §10.1 v1.5）：
    /// - 任务 json 读取 >32：截断后按已读计数（40 个可计任务 -> 恰 32），
    ///   且不触发段省略（计数非零仍渲染徽章，render 侧覆盖）；
    /// - workspace 探测 >64：第 65 个及之后的 workspace 不再探测。
    #[test]
    fn scan_limits_truncate_not_fail() {
        // 任务 json 上限：40 个全部可计 -> 恰计 32（与枚举顺序无关）
        let home = temp_home("cap-tasks");
        for i in 0..40 {
            write_task(
                &home,
                "wd_a",
                "s",
                "main",
                &format!("t{i:02}"),
                &task_json("running", "agent", None),
            );
        }
        assert_eq!(
            count_running(&home, Some("s")),
            TaskCounts { bash: 0, agent: 32 }
        );
        fs::remove_dir_all(&home).ok();

        // workspace 探测上限：目标只放在排序最后的 workspace——NTFS 目录枚举
        // 按名称序（B+ 树），前 64 次探测耗尽配额后截断，第 65 个不可达 -> 零计数
        let home = temp_home("cap-ws");
        for i in 0..65 {
            let ws = format!("wd_{i:04}");
            fs::create_dir_all(home.join("sessions").join(&ws)).unwrap();
        }
        write_task(
            &home,
            "wd_zzzz",
            "s",
            "main",
            "t",
            &task_json("running", "agent", None),
        );
        assert_eq!(count_running(&home, Some("s")), TaskCounts::default());
        // 对照组：目标在前 64 个之一（首个 ws）-> 命中计数，不受上限影响
        write_task(
            &home,
            "wd_0000",
            "s",
            "main",
            "t",
            &task_json("running", "agent", None),
        );
        assert_eq!(
            count_running(&home, Some("s")),
            TaskCounts { bash: 0, agent: 1 }
        );
        fs::remove_dir_all(&home).ok();
    }

    /// sessionId 缺失/为空 -> 零计数（决策 A）。
    #[test]
    fn session_id_missing_or_empty() {
        let home = temp_home("sid-missing");
        write_task(
            &home,
            "wd_a",
            "s",
            "main",
            "t",
            &task_json("running", "agent", None),
        );
        assert_eq!(count_running(&home, None), TaskCounts::default());
        assert_eq!(count_running(&home, Some("")), TaskCounts::default());
        fs::remove_dir_all(&home).ok();
    }

    /// 进程存活校验烟测：自身存活、刚退出的子进程不存活。
    #[test]
    fn process_alive_sanity() {
        assert!(process_alive(alive_pid()));
        assert!(!process_alive(exited_pid()));
    }
}
