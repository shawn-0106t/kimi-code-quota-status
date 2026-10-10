// quota-status -- Kimi Code CLI statusline quota display (SPEC §1).
//
// Four run modes (SPEC §4):
//   Render mode (no args): stdin snapshot + local cache -> assemble one line of ANSI text, exit in milliseconds;
//   Fetch mode --refresh: credential chain -> GET /usages -> defensive parsing -> atomic cache write;
//   Self-check mode --test-fetch: same as fetch but no cache write, result printed as pretty JSON to stdout;
//   Version query mode --version: print "quota-status <CARGO_PKG_VERSION>" and exit (no stdin, no file IO).
//
// All implementation lives in the lib crate (src/lib.rs); this file only does argv dispatch.

use quota_status::{cache, config, console, credentials, quota, render};
use std::io::Write;

fn load_config() -> config::Config {
    credentials::kimi_home()
        .map(|k| config::load_from(&k))
        .unwrap_or_default()
}

/// Reads the config.toml text for the thinking-related segment (failure -> None, segment omitted).
/// Independent of credentials' line-by-line provider scan (separate parsing function).
fn config_toml_text() -> Option<String> {
    std::fs::read_to_string(credentials::kimi_home()?.join("config.toml")).ok()
}

fn main() {
    // args_os: non-Unicode argv does not panic (args() would; under panic=abort the host falls back to the built-in footer)
    let mode = std::env::args_os().nth(1);
    let mode = mode.as_deref().and_then(std::ffi::OsStr::to_str);
    match mode {
        // Fetch mode (SPEC §4.2): credential chain -> GET /usages (8s timeout, configurable) ->
        // defensive parsing -> atomic cache write; on any failure no cache write (LKG + fast-retry), always exit 0
        Some("--refresh") => {
            let cfg = load_config();
            // Clean up orphan tmps (leftovers from earlier refreshes killed between writing tmp and rename);
            // threshold 60s: an in-flight refresh's tmp lives <1s, so it is never mistakenly deleted
            if let Some(kimi) = credentials::kimi_home() {
                cache::cleanup_stale_tmps_at(
                    &cache::cache_path(&kimi),
                    std::time::Duration::from_secs(60),
                );
            }
            let result = quota::fetch(cfg.base_url.as_deref(), cfg.http_timeout_seconds);
            if result.error.is_none()
                && let Some(kimi) = credentials::kimi_home()
            {
                let _ = cache::write_cache_atomic_at(&cache::cache_path(&kimi), &result);
            }
        }
        // Self-check mode (SPEC §4.3): one full fetch, printed to stdout as pretty JSON (2-space
        // indent camelCase), no stdin read, no cache write
        Some("--test-fetch") => {
            let cfg = load_config();
            let r = quota::fetch(cfg.base_url.as_deref(), cfg.http_timeout_seconds);
            println!("{}", r.to_pretty_json());
        }
        // Version query mode: print "<name> <version>" and exit 0; no stdin read, no other IO.
        // Same source as the embedded VERSIONINFO resource (CARGO_PKG_VERSION at compile time)
        Some("--version") => {
            println!("quota-status {}", env!("CARGO_PKG_VERSION"));
        }
        // Render mode (SPEC §4.1)
        _ => {
            let cfg = load_config();
            // Step 1: read stdin to EOF then parse; invalid input treated as empty payload
            let mut buf = Vec::new();
            let _ = std::io::Read::read_to_end(&mut std::io::stdin(), &mut buf);
            let payload = render::payload_from_bytes(&buf);

            // Steps 2-3: cache freshness check (age >= TTL -> rewind mtime + spawn detached --refresh)
            let cached = credentials::kimi_home().and_then(|kimi| {
                cache::refresh_if_stale(&kimi, cfg.ttl_seconds, cfg.retry_seconds, || {
                    let _ = console::spawn_detached_refresh();
                })
            });

            // Step 4: thinking segment (skips the config.toml read when the switch is off, saves IO)
            let thinking = if cfg.order.contains(&config::Field::Thinking) {
                render::thinking_from_config(
                    config_toml_text().as_deref(),
                    payload.get("model").and_then(|v| v.as_str()),
                )
            } else {
                None
            };

            // Width-aware degradation (§7.5) + line assembly (§7.4); kimi_home is for tasks badge
            // counting (§7.7, one scan when order contains "tasks", v1.5)
            let line = render::render_line(
                &payload,
                cached.as_ref(),
                &cfg,
                thinking.as_ref(),
                console::console_width(),
                chrono::Local::now(),
                credentials::kimi_home().as_deref(),
            );

            // Output exactly one line + \n, stdout forced to UTF-8 bytes (§7.6), exit 0 in every case
            let stdout = std::io::stdout();
            let mut lock = stdout.lock();
            let _ = lock.write_all(line.as_bytes());
            let _ = lock.write_all(b"\n");
            let _ = lock.flush();
        }
    }
    // Render mode exits 0 in every case (SPEC §3.2); other modes likewise end with exit code 0
    // (failure is expressed via "cache not updated"/"error field", SPEC §4.2/§4.3).
    std::process::exit(0);
}
