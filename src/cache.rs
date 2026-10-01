// 缓存层（SPEC §4.2/§4.4/§5.3）：渲染路径只读本地缓存，缓存过期时回拨
// mtime 后派生 detached --refresh 回填。任何失败保留 LKG（SPEC §9）。
//
// 路径：<kimi_home>/cache/quota-status.json（随 KIMI_CODE_HOME 联动）。
// 原子写：同目录 .tmp + fs::rename（Windows std rename 带
// MOVEFILE_REPLACE_EXISTING，语义同 os.replace）。

use crate::quota::QuotaResult;
use filetime::{set_file_mtime, FileTime};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// 内置默认 TTL（SPEC §5.3；P3 接入 quota-bar.toml [cache] 覆盖）
pub const DEFAULT_TTL_SECS: u64 = 60;
/// fast-retry 间隔（SPEC §4.4；P3 接入 [cache] retry_seconds 覆盖）
pub const DEFAULT_RETRY_SECS: u64 = 30;

pub fn cache_path(kimi: &Path) -> PathBuf {
    kimi.join("cache").join("quota-status.json")
}

/// 读缓存：JSON 非法/缺失 -> None（当缺失处理，SPEC §9）。
pub fn read_cache_at(path: &Path) -> Option<QuotaResult> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// 原子写（SPEC §4.2 步骤 4）：mkdir -p -> 写 .tmp（带 PID 后缀，消除并发
/// refresh 交错写同一 tmp 的竞态窗口）-> rename 替换。
/// rename 失败重试一次（防病毒扫描等瞬时占用）；仍失败保留旧缓存。
pub fn write_cache_atomic_at(path: &Path, result: &QuotaResult) -> io::Result<()> {
    let dir = path.parent().ok_or_else(|| {
        io::Error::other("cache path has no parent directory")
    })?;
    fs::create_dir_all(dir)?;
    let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
    let text = serde_json::to_string_pretty(result)
        .map_err(|e| io::Error::other(e.to_string()))?;
    fs::write(&tmp, text)?;
    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(_) => fs::rename(&tmp, path), // 重试一次；仍失败保留旧缓存
    }
}

/// 清理孤儿 tmp（SPEC §4.2 原子写的副产物）：refresh 进程在写 tmp 与 rename
/// 之间被杀时，`quota-status.json.<pid>.tmp` 无属主残留。仅清理 mtime 早于
/// older_than 的文件——在途 refresh 的 tmp 存活期 <1s，不会被误删。
pub fn cleanup_stale_tmps_at(path: &Path, older_than: Duration) {
    let Some(dir) = path.parent() else { return };
    let Some(base) = path.file_name().and_then(|n| n.to_str()) else { return };
    let prefix = format!("{base}.");
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let Ok(name) = entry.file_name().into_string() else { continue };
        if !name.starts_with(&prefix) || !name.ends_with(".tmp") {
            continue;
        }
        let age = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| SystemTime::now().duration_since(t).ok());
        if age.is_some_and(|a| a >= older_than) {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// 缓存文件年龄；缺失/不可读 -> None（视为过期）。
pub fn cache_age(path: &Path) -> Option<Duration> {
    let mtime = fs::metadata(path).ok()?.modified().ok()?;
    // mtime 在未来（时钟回拨等）时 duration_since 返回 Err -> None，
    // 调用方按过期处理：回拨锚点即自愈（不 panic）
    SystemTime::now().duration_since(mtime).ok()
}

/// mtime 回拨目标（SPEC §4.4）：now - TTL + retry（= now - (TTL - retry)），
/// 使后续渲染 retry 秒内看到"未过期"。retry >= TTL 时退化回拨 1s（不 panic）。
fn rollback_target(now: SystemTime, ttl_secs: u64, retry_secs: u64) -> SystemTime {
    match now.checked_sub(Duration::from_secs(ttl_secs.saturating_sub(retry_secs))) {
        Some(t) if retry_secs < ttl_secs => t,
        _ => now
            .checked_sub(Duration::from_secs(1))
            .unwrap_or(now),
    }
}

/// 把 mtime 回拨为 now - TTL + retry（Windows SetFileTime，filetime crate）。
fn rollback_mtime_at(path: &Path, ttl_secs: u64, retry_secs: u64) -> io::Result<()> {
    set_file_mtime(
        path,
        FileTime::from_system_time(rollback_target(SystemTime::now(), ttl_secs, retry_secs)),
    )
}

/// 缺失时创建空缓存文件锚定 mtime（SPEC §5.3/§9；Python 原型 'a' 打开同义）。
/// create_new 原子语义：文件已存在（含并发 refresh 恰好 rename 落盘）直接
/// Ok 返回，绝不截断已有内容（消除 exists() 后 create 的 TOCTOU 窗口）。
fn ensure_anchor_at(path: &Path) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    match fs::OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(e),
    }
}

/// 渲染模式数据层步骤（SPEC §4.1 步骤 2–3、§4.4、§9）：
/// 1. 读缓存（损坏当缺失：额度组省略，不清空不阻塞）；
/// 2. age >= TTL（含文件缺失）-> 缺失先建空锚定文件 -> 回拨 mtime -> 派生刷新；
/// 3. 回拨失败跳过派生本轮（宁可晚一轮刷新，不 panic）。
///
/// `spawn_refresh` 为派生动作（生产传 detached spawn，测试传 mock）；
/// 返回值供渲染使用（过期/缺失/损坏时可能为 None，其余字段照常渲染）。
pub fn refresh_if_stale<F: FnOnce()>(
    kimi: &Path,
    ttl_secs: u64,
    retry_secs: u64,
    spawn_refresh: F,
) -> Option<QuotaResult> {
    let path = cache_path(kimi);
    let cached = read_cache_at(&path);
    let age = cache_age(&path);
    // 新鲜判定只看 mtime（SPEC §4.1 步骤 3/§4.4 纯 age 语义，Python 原型
    // maybe_refresh 同款）：age < TTL 即不派生——空锚定、损坏缓存、refresh
    // 持续失败（无 token / 404）时回拨后的 mtime 同样压住风暴，retry 秒后
    // 自然重试；若把"可解析"相与进来，上述场景每次渲染都会派生进程
    if let Some(a) = age {
        if a < Duration::from_secs(ttl_secs) {
            return cached;
        }
    }
    // 过期或缺失：先锚定（仅缺失时创建），再回拨，回拨成功才派生
    if ensure_anchor_at(&path).is_err() {
        return cached; // 锚定失败（目录不可写等）：放弃本轮刷新
    }
    if rollback_mtime_at(&path, ttl_secs, retry_secs).is_err() {
        return cached; // 回拨失败：跳过派生，避免刷新风暴
    }
    spawn_refresh();
    cached
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quota::{ExtraInfo, ExtraState, QuotaSegment, QuotaResult};
    use chrono::{Local, TimeZone, Timelike};

    fn test_result() -> QuotaResult {
        let at = Local.timestamp_millis_opt(1_893_456_000_000).unwrap();
        QuotaResult {
            five_hour: Some(QuotaSegment {
                percent: 21.0,
                reset_at: Some(at),
            }),
            week: Some(QuotaSegment {
                percent: 18.5,
                reset_at: None,
            }),
            month: Some(QuotaSegment {
                percent: 43.0,
                reset_at: Some(at),
            }),
            extra: Some(ExtraInfo {
                state: ExtraState::Ready,
                balance_cents: Some(1235),
                monthly_enabled: true,
                monthly_used_cents: Some(4567),
                monthly_limit_cents: Some(10000),
            }),
            fetched_at: at.with_nanosecond(123_456_789).unwrap(),
            error: None,
        }
    }

    /// 每个测试独享临时目录（env 全局可变，不通过 KIMI_CODE_HOME 隔离）。
    fn temp_kimi(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("quota-status-test-{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 回拨计算单测：target = now - TTL + retry（SPEC §4.4 / PLAN P2 单测清单）。
    #[test]
    fn rollback_target_is_now_minus_ttl_plus_retry() {
        let now = SystemTime::now();
        let want = now
            .checked_sub(Duration::from_secs(30))
            .unwrap();
        let got = rollback_target(now, 60, 30);
        assert!(got.duration_since(want).unwrap_or_default() < Duration::from_millis(5));
        assert!(want.duration_since(got).unwrap_or_default() < Duration::from_millis(5));

        // retry >= TTL：退化回拨 1s，不 panic
        let got = rollback_target(now, 30, 60);
        assert!(got < now);
    }

    /// 原子写后内容可 round-trip 解析（PLAN P2 单测清单）。
    #[test]
    fn atomic_write_round_trip() {
        let kimi = temp_kimi("round-trip");
        let path = cache_path(&kimi);
        let data = test_result();
        write_cache_atomic_at(&path, &data).unwrap();
        let back = read_cache_at(&path).unwrap();
        assert_eq!(back.five_hour.as_ref().unwrap().percent, 21.0);
        assert_eq!(back.month.as_ref().unwrap().percent, 43.0);
        assert_eq!(back.extra.as_ref().unwrap().balance_cents, Some(1235));
        assert_eq!(back.fetched_at, data.fetched_at);
        assert!(back.error.is_none());
        // tmp 文件（带 PID 后缀）已被 rename 消费，不残留
        let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
        assert!(!tmp.exists());
        fs::remove_dir_all(&kimi).unwrap();
    }

    /// 缓存损坏当缺失（SPEC §9）：额度组省略、垃圾内容不清空；派生与否只看
    /// mtime（§4.4 纯 age 语义）——新鲜损坏文件不派生，过期才回拨 + 派生。
    #[test]
    fn corrupt_cache_treated_as_missing() {
        let kimi = temp_kimi("corrupt");
        let path = cache_path(&kimi);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{not valid json").unwrap();
        let before = fs::read(&path).unwrap();

        // 损坏但新鲜（mtime≈now < TTL）：不派生，返回 None，内容原样
        let mut spawns = 0;
        let got = refresh_if_stale(&kimi, 60, 30, || spawns += 1);
        assert!(got.is_none());
        assert_eq!(spawns, 0, "新鲜损坏缓存不派生（纯 mtime 判定）");
        assert_eq!(fs::read(&path).unwrap(), before, "LKG 垃圾内容不被清空");

        // 损坏且过期：回拨 + 派生一次；紧接着的渲染被回拨后的 mtime 压住
        let stale = SystemTime::now() - Duration::from_secs(60);
        set_file_mtime(&path, FileTime::from_system_time(stale)).unwrap();
        let mut spawns = 0;
        let got = refresh_if_stale(&kimi, 60, 30, || spawns += 1);
        assert!(got.is_none());
        assert_eq!(spawns, 1);
        let age = cache_age(&path).unwrap().as_secs();
        assert!(age >= 29 && age <= 31, "age after rollback = {age}");
        let got = refresh_if_stale(&kimi, 60, 30, || spawns += 1);
        assert!(got.is_none());
        assert_eq!(spawns, 1, "回拨后 30s 内连续渲染只派生一次（§4.4 防风暴）");
        fs::remove_dir_all(&kimi).unwrap();
    }

    /// 缓存缺失：创建空锚定文件 + 派生一次（SPEC §5.3）；回拨后的空锚定
    /// 压住后续渲染（§4.4）——refresh 持续失败（无 token / 404）也不会
    /// 每次渲染都派生进程。
    #[test]
    fn missing_cache_creates_anchor_and_spawns() {
        let kimi = temp_kimi("missing");
        let path = cache_path(&kimi);
        let mut spawns = 0;
        let got = refresh_if_stale(&kimi, 60, 30, || spawns += 1);
        assert!(got.is_none());
        assert_eq!(spawns, 1);
        assert!(path.exists(), "锚定文件已创建");
        // 连续第二次渲染：age≈TTL-retry < TTL，不重复派生
        let got = refresh_if_stale(&kimi, 60, 30, || spawns += 1);
        assert!(got.is_none());
        assert_eq!(spawns, 1, "连续渲染只派生一次（§4.4 防风暴）");
        fs::remove_dir_all(&kimi).unwrap();
    }

    /// TTL 过期边界（PLAN P2 单测清单）：age == TTL 触发派生；
    /// age < TTL（含回拨后的 TTL-retry）不派生且返回数据。
    #[test]
    fn ttl_expiry_boundary() {
        let kimi = temp_kimi("ttl");
        let path = cache_path(&kimi);
        let data = test_result();
        write_cache_atomic_at(&path, &data).unwrap();

        // age == TTL -> 过期，派生
        let stale = SystemTime::now() - Duration::from_secs(60);
        set_file_mtime(&path, FileTime::from_system_time(stale)).unwrap();
        let mut spawns = 0;
        let got = refresh_if_stale(&kimi, 60, 30, || spawns += 1);
        assert!(got.is_some(), "过期仍返回旧数据（LKG 继续渲染）");
        assert_eq!(spawns, 1);
        // 回拨后 mtime age ≈ 30，落在 retry 窗口内
        let age = cache_age(&path).unwrap().as_secs();
        assert!(age >= 29 && age <= 31, "age after rollback = {age}");

        // age < TTL -> 新鲜，不派生
        set_file_mtime(&path, FileTime::from_system_time(SystemTime::now() - Duration::from_secs(59)))
            .unwrap();
        let mut spawns = 0;
        let got = refresh_if_stale(&kimi, 60, 30, || spawns += 1);
        assert!(got.is_some());
        assert_eq!(spawns, 0, "新鲜缓存不派生");
        fs::remove_dir_all(&kimi).unwrap();
    }

    /// 锚定绝不截断已有文件（create_new 原子语义）：并发 refresh 恰好 rename
    /// 落盘后，渲染侧的锚定尝试不会把新缓存截断为空。
    /// 注：TOCTOU 竞态窗口无法确定性复现，本测试钉死的是 create_new 的
    /// 不截断不变量（characterization test），非竞态回归测试。
    #[test]
    fn anchor_never_truncates_existing() {
        let kimi = temp_kimi("anchor");
        let path = cache_path(&kimi);
        write_cache_atomic_at(&path, &test_result()).unwrap();
        let before = fs::read(&path).unwrap();
        ensure_anchor_at(&path).unwrap();
        assert_eq!(fs::read(&path).unwrap(), before);
        fs::remove_dir_all(&kimi).unwrap();
    }

    /// 孤儿 tmp 清理：老 tmp 删除；在途（新鲜）tmp 与非 tmp 文件保留。
    #[test]
    fn cleanup_stale_tmps_removes_only_old_tmps() {
        let kimi = temp_kimi("tmps");
        let path = cache_path(&kimi);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let old_tmp = path.with_extension("json.111.tmp");
        let fresh_tmp = path.with_extension("json.222.tmp");
        let other = path.with_extension("json.bak");
        fs::write(&old_tmp, "x").unwrap();
        fs::write(&fresh_tmp, "x").unwrap();
        fs::write(&other, "x").unwrap();
        let old = SystemTime::now() - Duration::from_secs(3600);
        set_file_mtime(&old_tmp, FileTime::from_system_time(old)).unwrap();

        cleanup_stale_tmps_at(&path, Duration::from_secs(60));
        assert!(!old_tmp.exists(), "老孤儿 tmp 被清理");
        assert!(fresh_tmp.exists(), "在途 tmp 不误删");
        assert!(other.exists(), "非 tmp 文件不动");
        fs::remove_dir_all(&kimi).unwrap();
    }
}
