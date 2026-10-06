// tasks/agents badge data source (SPEC §7.7 v1.5): scan <kimi_home>/sessions/ to locate the
// session directory for the payload sessionId, read agents/*/tasks/*.json, and count running
// tasks routed by kind. Defensive throughout: any failure yields zero counts (segment omitted),
// never panics, never blocks rendering (Decision E); scan caps truncate counts instead of failing (Decision D).

use serde_json::Value;
use std::path::Path;
use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};

/// Workspace probe cap (SPEC §7.7 Decision D)
const MAX_WORKSPACE_PROBES: u32 = 64;
/// Task json read cap (SPEC §7.7 Decision D)
const MAX_TASK_READS: u32 = 32;
/// Per-task json size guard: task jsons the host writes are always byte-scale small files;
/// oversized ones are treated as format drift and skipped (a per-file guard in the spirit of Decision D, keeping pathological files off the hot path)
const MAX_TASK_JSON_BYTES: u64 = 64 * 1024;

/// Count result: running task counts on the bash side (kind=process/question/unknown/missing,
/// via pid-liveness check) and the agent side (kind=agent, counted while running) (SPEC §7.7 Decision C)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TaskCounts {
    pub bash: u32,
    pub agent: u32,
}

/// payload sessionId + `<kimi_home>` -> running task counts (the tasks badge segment of
/// the rendered line, SPEC §7.1/§7.7). Defense rules:
/// - Decision A: missing/empty sessionId -> zero counts (segment omitted); probe each workspace
///   directory under sessions/ for `<ws>/<sessionId>` (sessionIds are globally unique; the first hit locates);
/// - Decision B: sessionId character allowlist check (`[A-Za-z0-9_-]`, covering `/`, `\`,
///   `..` and every other traversal surface such as drive-relative paths) failing -> zero counts;
/// - Decision D: workspace probes <=64, task json reads <=32; beyond the cap, truncate and count what was read;
/// - Decision E: sessions directory missing / json parse failure / any IO error -> zero counts.
pub fn count_running(kimi_home: &Path, session_id: Option<&str>) -> TaskCounts {
    // Decision A: missing/empty sessionId -> segment omitted
    let Some(sid) = session_id.filter(|s| !s.is_empty()) else {
        return TaskCounts::default();
    };
    // Decision B: path defense (comes from the host payload; must be validated before joining)
    if !is_safe_session_id(sid) {
        return TaskCounts::default();
    }
    let Ok(entries) = std::fs::read_dir(kimi_home.join("sessions")) else {
        return TaskCounts::default(); // Decision E: directory missing -> zero counts
    };
    let mut probes = 0u32;
    for entry in entries.flatten() {
        let ws = entry.path();
        if !ws.is_dir() {
            continue; // non-workspace directories do not consume probe quota
        }
        if probes >= MAX_WORKSPACE_PROBES {
            break; // Decision D: truncation, not failure
        }
        probes += 1;
        let session_dir = ws.join(sid);
        if session_dir.is_dir() {
            return count_in_session(&session_dir);
        }
    }
    TaskCounts::default() // no hit -> zero counts (segment omitted)
}

/// sessionId shape validation (Decision B): the host's real value is `session_<uuid>`
/// (RFC 4122 hex + hyphens); the character allowlist `[A-Za-z0-9_-]` covers the three
/// SPEC rejection rules (`/`, `\`, `..`) in one step and seals the remaining traversal
/// surfaces — drive-relative paths (e.g. `"C:evil"` makes `Path::join` replace the whole
/// base with the C drive's current directory), bare `"."`, UNC, Windows reserved names, trailing dots/spaces, non-ASCII, etc.
fn is_safe_session_id(sid: &str) -> bool {
    sid.bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Walk `<sessionDir>/agents/*/tasks/*.json` and count (Decision B: only the new
/// agents/*/tasks/ layout is scanned; file names are not validated, json content is authoritative).
fn count_in_session(session_dir: &Path) -> TaskCounts {
    let mut counts = TaskCounts::default();
    let Ok(agents) = std::fs::read_dir(session_dir.join("agents")) else {
        return counts; // Decision E: agents directory missing/unreadable -> zero counts
    };
    let mut reads = 0u32;
    for agent in agents.flatten() {
        if reads >= MAX_TASK_READS {
            break; // Decision D: stop once the budget is exhausted
        }
        let agent_dir = agent.path();
        if !agent_dir.is_dir() {
            continue;
        }
        let tasks = match std::fs::read_dir(agent_dir.join("tasks")) {
            Ok(t) => t,
            // A missing tasks directory for this agent is an empty state (same as top-level
            // sessions missing) -> skip; other IO errors are Decision E failures -> short-circuit to zero counts (entire segment omitted)
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return TaskCounts::default(),
        };
        for task in tasks.flatten() {
            if reads >= MAX_TASK_READS {
                break; // Decision D: truncate and count from what has been read
            }
            let path = task.path();
            if !is_task_json(&path) {
                continue;
            }
            reads += 1;
            if count_task_file(&path, &mut counts).is_none() {
                // Decision E: task file read/parse failure -> short-circuit to zero counts
                // (entire segment omitted); accumulated partial counts must not be kept (SPEC §7.7 Decision E / §9)
                return TaskCounts::default();
            }
        }
    }
    counts
}

/// Whether a tasks directory entry is a task json to read (the host always persists
/// `<taskId>.json`; case-insensitive to match Windows filesystem semantics)
fn is_task_json(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("json"))
}

/// Count a single task json (SPEC §7.7 counting semantics): only `status == "running"` is counted
/// (the five terminal statuses and unknown statuses are skipped); `kind == "agent"` (strict
/// equality) is counted toward the agent side while running; every other kind goes through the
/// pid-liveness check toward the bash side. Returns None on read/parse failure (Decision E: caller short-circuits, entire segment omitted).
fn count_task_file(path: &Path, counts: &mut TaskCounts) -> Option<()> {
    // Size guard: oversized files are deliberately skipped as format drift (not a failure), the rest counted as usual;
    // a metadata failure is treated as within the cap (a later read failure takes the Decision E short-circuit)
    if std::fs::metadata(path).is_ok_and(|m| m.len() > MAX_TASK_JSON_BYTES) {
        return Some(());
    }
    let text = std::fs::read_to_string(path).ok()?; // Decision E: read failure
    let v = serde_json::from_str::<Value>(&text).ok()?; // Decision E: parse failure
    if v.get("status").and_then(|s| s.as_str()) != Some("running") {
        return Some(());
    }
    if v.get("kind").and_then(|k| k.as_str()) == Some("agent") {
        // Decision C: agent tasks have no pid field (async tasks inside the CLI process);
        // counted while running, accepting the stale over-count after a CLI crash (host reload marks them lost and self-heals)
        counts.agent += 1;
        return Some(());
    }
    // Every other kind (process/question/unknown/missing) -> bash side via pid check;
    // question has no pid and is never counted (2026-10-02 arbitration: conservative, prefer under-counting over false positives)
    let Some(pid) = pid_of(&v) else {
        return Some(());
    };
    if process_alive(pid) {
        counts.bash += 1;
    }
    Some(())
}

/// pid defense (Decision C): only a JSON positive integer <= u32::MAX is accepted;
/// missing, string form, negative, fractional, 0, or > u32 are all treated as missing, conservatively not counted.
fn pid_of(task: &Value) -> Option<u32> {
    let n = task.get("pid")?.as_u64()?;
    if n == 0 || n > u32::MAX as u64 {
        return None;
    }
    Some(n as u32)
}

/// pid-liveness check (Decision C bash side): OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)
/// plus GetExitCodeProcess; a NULL handle (process missing / no permission) or exit code !=
/// STILL_ACTIVE(259) is treated as exited. Reuses the existing windows-sys dependency, zero new crates.
fn process_alive(pid: u32) -> bool {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return false;
        }
        let mut exit_code: u32 = 0;
        let ok = GetExitCodeProcess(handle, &mut exit_code);
        CloseHandle(handle);
        // Only exit code == STILL_ACTIVE(259) counts as alive (Decision C; windows-sys's
        // STILL_ACTIVE is an NTSTATUS = i32, while GetExitCodeProcess's out-param is u32).
        // Known blind spot: an exited process whose exit code happens to be 259 is misjudged
        // as alive (an inherent Win32 API limitation, same family as the PID-reuse window; probability negligible)
        ok != 0 && exit_code == STILL_ACTIVE as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::process::{Command, Stdio};

    /// Isolated temporary <kimi_home> (unique per tag + process id; leftovers cleaned first)
    fn temp_home(tag: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!("qs-tasks-{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        base
    }

    /// Write one task json to disk: <home>/sessions/<ws>/<sid>/agents/<agent>/tasks/<task>.json
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

    /// Positive control for a live process: this test process itself is necessarily alive
    /// (PROCESS_QUERY_LIMITED_INFORMATION can query itself); no child process needs to be spawned.
    fn alive_pid() -> u32 {
        std::process::id()
    }

    /// Exited process: a cmd that exits immediately after spawn; after wait reaps it, its pid should not be judged alive.
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
        // The reuse window is tiny (Windows pids are allocated round-robin); a hit means a defect in the test itself, so prefer failing
        assert!(
            !process_alive(pid),
            "just-exited pid reused immediately? pid={pid}"
        );
        pid
    }

    /// Workspace probe: sessionId hit (SPEC §10.1 v1.5 probe-hit case)
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

    /// Workspace probe: sessionId miss -> zero counts (segment omitted)
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
        // the sessions directory itself missing -> zero counts (Decision E)
        let empty = temp_home("miss-empty");
        assert_eq!(
            count_running(&empty, Some("session_x")),
            TaskCounts::default()
        );
        fs::remove_dir_all(&home).ok();
        fs::remove_dir_all(&empty).ok();
    }

    /// Path defense (Decision B, SPEC §10.1 v1.5 three traversal-string rejections): sessionIds
    /// containing `/`, `\`, `..` are invalid -> zero counts — even with the traversal target directory forged into existence, it must still be rejected.
    #[test]
    fn session_id_traversal_rejected() {
        let home = temp_home("traversal");
        // "../evil" traversal target: sessions/evil (reachable via join(..) within ws)
        write_task(
            &home,
            "wd_a",
            "../evil",
            "main",
            "t1",
            &task_json("running", "agent", None),
        );
        // "a/b" and "a\\b" traversal targets: subpath a/b within ws (backslash join is equivalent on Windows)
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
                "traversal string {sid:?} must be rejected"
            );
        }
        fs::remove_dir_all(&home).ok();
    }

    /// sessionId allowlist (review Minor 1 hardening): seals the remaining traversal surfaces
    /// beyond the three rules — drive-relative paths (join replaces the whole base), bare ".",
    /// non-ASCII, whitespace, etc. are all rejected; the legitimate shape (the host's real `session_<uuid>` character domain) passes.
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
            "legitimate character domain (letters/digits/_/-) must pass"
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
                "sid {sid:?} outside the allowlist must be rejected"
            );
        }
        fs::remove_dir_all(&home).ok();
    }

    /// Per-file size guard (review Nit 4 hardening): a task json >64KB is treated as format
    /// drift and skipped (not counted, without blocking other tasks in the same directory).
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

    /// Count routing (Decision C): agent counted while running / question not counted without
    /// pid / unknown kind and missing kind routed to the bash side via pid check.
    #[test]
    fn kind_routing() {
        let home = temp_home("routing");
        // agent: no pid, counted toward the agent side while running (no pid check)
        write_task(
            &home,
            "wd_a",
            "s",
            "main",
            "t_agent",
            &task_json("running", "agent", None),
        );
        // question: no pid, routed to the bash side but pid missing -> not counted
        write_task(
            &home,
            "wd_a",
            "s",
            "main",
            "t_question",
            &task_json("running", "question", None),
        );
        // unknown kind / missing kind: routed to the bash side, pid alive -> counted
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

    /// Terminal and unknown statuses skipped (SPEC §10.1 v1.5: five terminal statuses and unknown statuses not counted)
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
        // missing status is skipped likewise
        let dir = home.join("sessions/wd_a/s/agents/main/tasks");
        fs::write(
            dir.join("t_nostatus.json"),
            r#"{"taskId":"t","kind":"process"}"#,
        )
        .unwrap();
        assert_eq!(count_running(&home, Some("s")), TaskCounts::default());
        fs::remove_dir_all(&home).ok();
    }

    /// pid defense (SPEC §10.1 v1.5 three bash cases): alive counted / exited not counted /
    /// missing or non-positive-integer pid (string, negative, fractional, 0, > u32) not counted.
    #[test]
    fn pid_defense() {
        // live process counted
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

        // exited process not counted
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

        // hostile pid shapes treated as missing: string, negative, fractional, 0, > u32, field missing
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

    /// Agent case (SPEC §10.1 v1.5): counted while running, no pid check —
    /// counted even if forged with an exited pid (this shape has no pid field to begin with).
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

    /// json parse failure omits the entire segment (Decision E strict semantics, SPEC §7.7/§9):
    /// when a bad file exists, partial counts from other tasks must not be kept — return zero counts
    /// (whole badge segment omitted). File names a_good/z_bad make the countable task precede the
    /// bad file in NTFS name order, deterministically pinning "accumulated counts are discarded on short-circuit" (not relying on the bad file being enumerated first).
    #[test]
    fn malformed_json_omits_segment() {
        let home = temp_home("badjson");
        write_task(
            &home,
            "wd_a",
            "s",
            "main",
            "a_good",
            &task_json("running", "agent", None),
        );
        write_task(&home, "wd_a", "s", "main", "z_bad", "not-json{{{");
        assert_eq!(count_running(&home, Some("s")), TaskCounts::default());
        fs::remove_dir_all(&home).ok();
    }

    /// Cross-agent propagation (Decision E, added per third-review Minor-1): agent A's tasks all
    /// countable, agent B has a bad file -> still the entire segment omitted (accumulated counts
    /// must not be kept). Directory names aaa_main/zzz_sub ensure A is counted first in NTFS name order.
    #[test]
    fn malformed_json_omits_segment_across_agents() {
        let home = temp_home("badjson-xagent");
        write_task(
            &home,
            "wd_a",
            "s",
            "aaa_main",
            "t",
            &task_json("running", "agent", None),
        );
        write_task(&home, "wd_a", "s", "zzz_sub", "t", "not-json{{{");
        assert_eq!(count_running(&home, Some("s")), TaskCounts::default());
        fs::remove_dir_all(&home).ok();
    }

    /// An agent directory missing its tasks/ subdirectory is an empty state (not a failure,
    /// the Decision E NotFound exemption): no segment omission, other agents counted as usual.
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
        // add alongside it an agent without a tasks/ subdirectory (empty state; enumeration order does not affect the result)
        fs::create_dir_all(home.join("sessions/wd_a/s/agents/sub_x")).unwrap();
        assert_eq!(
            count_running(&home, Some("s")),
            TaskCounts { bash: 0, agent: 1 }
        );
        fs::remove_dir_all(&home).ok();
    }

    /// Scan-cap truncation (Decision D, SPEC §10.1 v1.5):
    /// - task json reads >32: truncated and counted from what was read (40 countable tasks ->
    ///   exactly 32), without triggering segment omission (non-zero counts still render the badge; render side covers it);
    /// - workspace probes >64: the 65th and later workspaces are no longer probed.
    #[test]
    fn scan_limits_truncate_not_fail() {
        // task json cap: all 40 countable -> exactly 32 counted (independent of enumeration order)
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

        // workspace probe cap: the target sits only in the last-sorted workspace — NTFS enumerates
        // directories in name order (B+ tree); the first 64 probes exhaust the quota, then truncation leaves the 65th unreachable -> zero counts
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
        // control group: target among the first 64 (the first ws) -> hit and counted, unaffected by the cap
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

    /// Missing/empty sessionId -> zero counts (Decision A).
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

    /// Process-liveness smoke test: self alive, just-exited child not alive.
    #[test]
    fn process_alive_sanity() {
        assert!(process_alive(alive_pid()));
        assert!(!process_alive(exited_pid()));
    }
}
