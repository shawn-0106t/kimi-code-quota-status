// Cache layer (SPEC §4.2/§4.4/§5.3): the render path only reads the local cache;
// when stale, rewind mtime and spawn a detached --refresh child process to
// backfill. Any failure keeps LKG (SPEC §9).
//
// Path: <kimi_home>/cache/quota-status.json (follows KIMI_CODE_HOME).
// Atomic write: same-directory .tmp + fs::rename (Windows std rename sets
// MOVEFILE_REPLACE_EXISTING, same semantics as os.replace).

use crate::quota::QuotaResult;
use filetime::{FileTime, set_file_mtime};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Built-in default TTL (SPEC §5.3; overridable via quota-bar.toml [cache] since P3)
pub const DEFAULT_TTL_SECS: u64 = 60;
/// fast-retry interval (SPEC §4.4; overridable via [cache] retry_seconds since P3)
pub const DEFAULT_RETRY_SECS: u64 = 30;

pub fn cache_path(kimi: &Path) -> PathBuf {
    kimi.join("cache").join("quota-status.json")
}

/// Read the cache: invalid/missing JSON -> None (treated as missing, SPEC §9).
pub fn read_cache_at(path: &Path) -> Option<QuotaResult> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Atomic write (SPEC §4.2 step 4): mkdir -p -> write .tmp (with a PID suffix to
/// remove the race of concurrent refreshes writing the same tmp) -> rename. Retry
/// once on rename failure (e.g. AV-scan holds); if it still fails, keep the old cache.
pub fn write_cache_atomic_at(path: &Path, result: &QuotaResult) -> io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::other("cache path has no parent directory"))?;
    fs::create_dir_all(dir)?;
    let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
    let text = serde_json::to_string_pretty(result).map_err(|e| io::Error::other(e.to_string()))?;
    fs::write(&tmp, text)?;
    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(_) => fs::rename(&tmp, path), // retry once; if it still fails, keep the old cache
    }
}

/// Clean up orphan tmps (a byproduct of the SPEC §4.2 atomic write): when a refresh
/// process is killed between writing tmp and rename, `quota-status.json.<pid>.tmp`
/// lingers ownerless. Only mtime-older-than-older_than files are removed (in-flight tmps live <1s).
pub fn cleanup_stale_tmps_at(path: &Path, older_than: Duration) {
    let Some(dir) = path.parent() else { return };
    let Some(base) = path.file_name().and_then(|n| n.to_str()) else {
        return;
    };
    let prefix = format!("{base}.");
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
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

/// Cache file age; missing/unreadable -> None (treated as expired).
pub fn cache_age(path: &Path) -> Option<Duration> {
    let mtime = fs::metadata(path).ok()?.modified().ok()?;
    // when mtime is in the future (e.g. clock rewind), duration_since returns Err -> None,
    // the caller treats it as expired: rewinding the anchor self-heals (no panic)
    SystemTime::now().duration_since(mtime).ok()
}

/// mtime rewind target (SPEC §4.4): now - TTL + retry (= now - (TTL - retry)),
/// so later renders see "not stale" for retry seconds. retry >= TTL degrades to a 1s rewind (no panic).
fn rollback_target(now: SystemTime, ttl_secs: u64, retry_secs: u64) -> SystemTime {
    match now.checked_sub(Duration::from_secs(ttl_secs.saturating_sub(retry_secs))) {
        Some(t) if retry_secs < ttl_secs => t,
        _ => now.checked_sub(Duration::from_secs(1)).unwrap_or(now),
    }
}

/// Rewind mtime to now - TTL + retry (Windows SetFileTime, via the filetime crate).
fn rollback_mtime_at(path: &Path, ttl_secs: u64, retry_secs: u64) -> io::Result<()> {
    set_file_mtime(
        path,
        FileTime::from_system_time(rollback_target(SystemTime::now(), ttl_secs, retry_secs)),
    )
}

/// When missing, create an empty cache file to anchor mtime (SPEC §5.3/§9; same
/// as the Python prototype's 'a' open). create_new atomic semantics: an existing
/// file (incl. a concurrent refresh's landed rename) returns Ok; never truncates (no TOCTOU window).
fn ensure_anchor_at(path: &Path) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(e),
    }
}

/// Data-layer steps in render mode (SPEC §4.1 steps 2-3, §4.4, §9):
/// 1. Read the cache (corrupt treated as missing: quota groups omitted, no wipe, no blocking);
/// 2. age >= TTL (incl. missing file) -> create an empty anchor if missing -> rewind mtime -> spawn refresh;
/// 3. A rewind failure skips the spawn this round (a late refresh beats a panic).
///
/// `spawn_refresh` is the spawn action (production passes a detached spawn, tests a mock);
/// the return value feeds rendering (None possible when stale/missing/corrupt; other fields render as usual).
pub fn refresh_if_stale<F: FnOnce()>(
    kimi: &Path,
    ttl_secs: u64,
    retry_secs: u64,
    spawn_refresh: F,
) -> Option<QuotaResult> {
    let path = cache_path(kimi);
    let cached = read_cache_at(&path);
    let age = cache_age(&path);
    // Freshness is judged on mtime alone (SPEC §4.1 step 3/§4.4 pure-age semantics,
    // like the Python prototype's maybe_refresh): age < TTL -> no spawn. With an
    // empty anchor, corrupt cache, or persistently failing refresh (no token / 404),
    // the rewound mtime still suppresses the herd (retry after retry secs); adding "parseable" to the check would spawn on every render.
    if let Some(a) = age
        && a < Duration::from_secs(ttl_secs)
    {
        return cached;
    }
    // stale or missing: anchor first (create only when missing), then rewind; spawn only if rewind succeeds
    if ensure_anchor_at(&path).is_err() {
        return cached; // anchor failed (e.g. unwritable directory): give up this round's refresh
    }
    if rollback_mtime_at(&path, ttl_secs, retry_secs).is_err() {
        return cached; // rewind failed: skip spawning, avoiding a refresh thundering herd
    }
    spawn_refresh();
    cached
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quota::{ExtraInfo, ExtraState, QuotaResult, QuotaSegment};
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

    /// Each test gets its own temp directory (env is global mutable state, not isolated via KIMI_CODE_HOME).
    fn temp_kimi(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("quota-status-test-{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Unit test for the rewind computation: target = now - TTL + retry (SPEC §4.4 / PLAN P2 test list).
    #[test]
    fn rollback_target_is_now_minus_ttl_plus_retry() {
        let now = SystemTime::now();
        let want = now.checked_sub(Duration::from_secs(30)).unwrap();
        let got = rollback_target(now, 60, 30);
        assert!(got.duration_since(want).unwrap_or_default() < Duration::from_millis(5));
        assert!(want.duration_since(got).unwrap_or_default() < Duration::from_millis(5));

        // retry >= TTL: degrades to a 1s rewind, no panic
        let got = rollback_target(now, 30, 60);
        assert!(got < now);
    }

    /// Content written atomically round-trips through parsing (PLAN P2 test list).
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
        // the tmp file (with PID suffix) was consumed by the rename, nothing left behind
        let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
        assert!(!tmp.exists());
        fs::remove_dir_all(&kimi).unwrap();
    }

    /// Corrupt cache treated as missing (SPEC §9): quota groups omitted, garbage content kept;
    /// spawn depends on mtime alone (§4.4 pure-age) — fresh corrupt does not spawn, stale does (rewind + spawn).
    #[test]
    fn corrupt_cache_treated_as_missing() {
        let kimi = temp_kimi("corrupt");
        let path = cache_path(&kimi);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{not valid json").unwrap();
        let before = fs::read(&path).unwrap();

        // corrupt but fresh (mtime ~= now < TTL): no spawn, returns None, content intact
        let mut spawns = 0;
        let got = refresh_if_stale(&kimi, 60, 30, || spawns += 1);
        assert!(got.is_none());
        assert_eq!(
            spawns, 0,
            "fresh corrupt cache does not spawn (pure mtime check)"
        );
        assert_eq!(
            fs::read(&path).unwrap(),
            before,
            "LKG garbage content is not wiped"
        );

        // corrupt and stale: rewind + spawn once; the very next render is suppressed by the rewound mtime
        let stale = SystemTime::now() - Duration::from_secs(60);
        set_file_mtime(&path, FileTime::from_system_time(stale)).unwrap();
        let mut spawns = 0;
        let got = refresh_if_stale(&kimi, 60, 30, || spawns += 1);
        assert!(got.is_none());
        assert_eq!(spawns, 1);
        let age = cache_age(&path).unwrap().as_secs();
        assert!((29..=31).contains(&age), "age after rollback = {age}");
        let got = refresh_if_stale(&kimi, 60, 30, || spawns += 1);
        assert!(got.is_none());
        assert_eq!(
            spawns, 1,
            "consecutive renders within 30s after rewind spawn only once (§4.4 thundering-herd protection)"
        );
        fs::remove_dir_all(&kimi).unwrap();
    }

    /// Missing cache: create an empty anchor file + spawn once (SPEC §5.3); the
    /// rewound empty anchor suppresses later renders (§4.4) — even a persistently
    /// failing refresh (no token / 404) does not spawn a process on every render.
    #[test]
    fn missing_cache_creates_anchor_and_spawns() {
        let kimi = temp_kimi("missing");
        let path = cache_path(&kimi);
        let mut spawns = 0;
        let got = refresh_if_stale(&kimi, 60, 30, || spawns += 1);
        assert!(got.is_none());
        assert_eq!(spawns, 1);
        assert!(path.exists(), "anchor file created");
        // second consecutive render: age ~= TTL-retry < TTL, no duplicate spawn
        let got = refresh_if_stale(&kimi, 60, 30, || spawns += 1);
        assert!(got.is_none());
        assert_eq!(
            spawns, 1,
            "consecutive renders spawn only once (§4.4 thundering-herd protection)"
        );
        fs::remove_dir_all(&kimi).unwrap();
    }

    /// TTL expiry boundary (PLAN P2 test list): age == TTL triggers a spawn;
    /// age < TTL (incl. the post-rewind TTL-retry) does not spawn and returns data.
    #[test]
    fn ttl_expiry_boundary() {
        let kimi = temp_kimi("ttl");
        let path = cache_path(&kimi);
        let data = test_result();
        write_cache_atomic_at(&path, &data).unwrap();

        // age == TTL -> stale, spawn
        let stale = SystemTime::now() - Duration::from_secs(60);
        set_file_mtime(&path, FileTime::from_system_time(stale)).unwrap();
        let mut spawns = 0;
        let got = refresh_if_stale(&kimi, 60, 30, || spawns += 1);
        assert!(
            got.is_some(),
            "stale still returns old data (LKG keeps rendering)"
        );
        assert_eq!(spawns, 1);
        // after rewind, mtime age ~= 30, within the retry window
        let age = cache_age(&path).unwrap().as_secs();
        assert!((29..=31).contains(&age), "age after rollback = {age}");

        // age < TTL -> fresh, no spawn
        set_file_mtime(
            &path,
            FileTime::from_system_time(SystemTime::now() - Duration::from_secs(59)),
        )
        .unwrap();
        let mut spawns = 0;
        let got = refresh_if_stale(&kimi, 60, 30, || spawns += 1);
        assert!(got.is_some());
        assert_eq!(spawns, 0, "fresh cache does not spawn");
        fs::remove_dir_all(&kimi).unwrap();
    }

    /// Anchoring never truncates an existing file (create_new atomic semantics):
    /// after a concurrent refresh's rename lands, the render-side anchor attempt
    /// must not truncate the new cache to empty. Note: the TOCTOU race window is not
    /// deterministically reproducible; this pins the no-truncate invariant (characterization test, not a race regression test).
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

    /// Orphan tmp cleanup: old tmps deleted; in-flight (fresh) tmps and non-tmp files kept.
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
        assert!(!old_tmp.exists(), "old orphan tmp cleaned up");
        assert!(fresh_tmp.exists(), "in-flight tmp not wrongly deleted");
        assert!(other.exists(), "non-tmp file untouched");
        fs::remove_dir_all(&kimi).unwrap();
    }
}
