// Render module (SPEC §7): stdin snapshot + local cache -> assemble one line of ANSI-colored text.
// Order: permissionMode -> model -> thinking -> tasks/agents badges ->
// quota group (5h/week/month) -> gitBranch; each segment is shown only when non-empty; gray | between
// segments, gray · within the quota group. Width-aware degradation (§7.5): drop reset suffix -> drop
// gitBranch -> keep only the quota group -> output as-is.

use crate::config::{Config, Field};
use crate::quota::{ExtraState, QuotaResult};
use crate::tasks::TaskCounts;
use chrono::{DateTime, Local};
use serde_json::Value;
use std::path::Path;

const RESET: &str = "\x1b[0m";
const SEP_SEG: &str = " \x1b[90m|\x1b[0m "; // segment separator (gray |)
const SEP_PART: &str = " \x1b[90m·\x1b[0m "; // separator within the quota group (gray ·)
const GRAY: &str = "90";
const CYAN: &str = "36";
const GREEN: &str = "32";
const YELLOW: &str = "33";
const RED: &str = "31";
const MAGENTA: &str = "35";
const WHITE: &str = "37";

/// thinking segment value result (config.toml ladder output, SPEC §7.1)
#[derive(Debug, PartialEq)]
pub enum Thinking {
    Off,
    Effort(String),
}

/// stdin bytes -> payload Value: first lossy UTF-8 (U+FFFD, SPEC §7.6), then parse;
/// on parse failure, treat as an empty payload (Value::Null, same semantics as quota-status.py:177-181).
pub fn payload_from_bytes(bytes: &[u8]) -> Value {
    let text = String::from_utf8_lossy(bytes);
    serde_json::from_str(&text).unwrap_or(Value::Null)
}

/// thinking ladder (SPEC §7.1, same semantics as quota-status.py:101-119):
/// 1. [thinking] enabled == false (strict boolean) -> gray off;
/// 2. otherwise [thinking] effort (non-empty string);
/// 3. missing -> the entry in [models.*] whose display_name or model matches the stdin model,
///    take overrides.default_effort, then fall back to default_effort;
/// 4. none obtainable -> None (segment omitted).
pub fn thinking_from_config(config_text: Option<&str>, model: Option<&str>) -> Option<Thinking> {
    let text = config_text?;
    let Ok(val) = toml::from_str::<toml::Value>(text) else {
        return None;
    };
    let th = val.get("thinking");
    if th.and_then(|t| t.get("enabled")).and_then(|v| v.as_bool()) == Some(false) {
        return Some(Thinking::Off);
    }
    if let Some(eff) = th.and_then(|t| t.get("effort")).and_then(|v| v.as_str())
        && !eff.is_empty()
    {
        return Some(Thinking::Effort(eff.to_string()));
    }
    let model = model?;
    let models = val.get("models")?.as_table()?;
    for m in models.values() {
        let display = m.get("display_name").and_then(|v| v.as_str());
        let model_key = m.get("model").and_then(|v| v.as_str());
        if display != Some(model) && model_key != Some(model) {
            continue;
        }
        let over_eff = m
            .get("overrides")
            .and_then(|o| o.get("default_effort"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty());
        let direct_eff = m
            .get("default_effort")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty());
        if let Some(eff) = over_eff.or(direct_eff) {
            return Some(Thinking::Effort(eff.to_string()));
        }
    }
    None
}

/// Quota segment color (SPEC §7.2): percent < green_below green / < yellow_below yellow / otherwise red.
/// Boundaries: `>= 85` red, `>= 60` yellow (thresholds default to 60/85).
pub fn quota_color(percent: f64, green_below: f64, yellow_below: f64) -> &'static str {
    if percent < green_below {
        GREEN
    } else if percent < yellow_below {
        YELLOW
    } else {
        RED
    }
}

/// reset suffix (SPEC §7.3): same day ` (rst HH:MM)`; cross-day ` (rst MM/DD HH:MM)`.
/// Local time zone; resetAt parse failure/missing -> None (no suffix).
pub fn reset_suffix(reset_at: Option<&DateTime<Local>>, now: DateTime<Local>) -> Option<String> {
    let dt = *reset_at?;
    let same_day = dt.date_naive() == now.date_naive();
    Some(if same_day {
        format!(" (rst {})", dt.format("%H:%M"))
    } else {
        format!(" (rst {})", dt.format("%m/%d %H:%M"))
    })
}

fn permission_mode_color(mode: &str) -> &'static str {
    match mode {
        "yolo" => RED,
        "auto" => YELLOW,
        "manual" => GREEN,
        _ => WHITE,
    }
}

/// percent formatted to a whole number (%.0f = round-half-to-even, SPEC §7.3)
fn fmt_percent(p: f64) -> String {
    format!("{p:.0}")
}

struct QuotaPart {
    label: &'static str,
    percent: f64,
    reset_at: Option<DateTime<Local>>,
}

/// Quota group cache-side data (SPEC §7.1): a segment joins the group only if present and enabled;
/// a cache containing error (theoretically impossible, defensive) is treated as no data.
fn quota_parts(cached: Option<&QuotaResult>, cfg: &Config) -> Vec<QuotaPart> {
    let Some(r) = cached.filter(|r| r.error.is_none()) else {
        return Vec::new();
    };
    let mut parts = Vec::new();
    if cfg.quota.five_hour
        && let Some(seg) = &r.five_hour
    {
        parts.push(QuotaPart {
            label: "5h",
            percent: seg.percent,
            reset_at: seg.reset_at,
        });
    }
    if cfg.quota.week
        && let Some(seg) = &r.week
    {
        parts.push(QuotaPart {
            label: "week",
            percent: seg.percent,
            reset_at: seg.reset_at,
        });
    }
    if cfg.quota.month
        && let Some(seg) = &r.month
    {
        parts.push(QuotaPart {
            label: "month",
            percent: seg.percent,
            reset_at: seg.reset_at,
        });
    }
    parts
}

/// Line assembly (SPEC §7.4). `opts` controls the degradation variants:
/// reset=false drops all reset suffixes; git=false drops the gitBranch segment;
/// quota_only=true keeps only the quota group. Removal only, no reordering.
struct VariantOpts {
    reset: bool,
    git: bool,
    quota_only: bool,
}

fn render_variant(
    payload: &Value,
    cached: Option<&QuotaResult>,
    cfg: &Config,
    thinking: Option<&Thinking>,
    tasks: TaskCounts,
    now: DateTime<Local>,
    opts: &VariantOpts,
) -> String {
    let mut segs: Vec<String> = Vec::new();
    // Monochrome switch (SPEC §7.2 v1.4): when colors=false, output plain text (no SGR at all);
    // the host's footer.ts:318 chalk.hex(colors.text) wrapper tints the whole line with the theme text
    // color (same as the context on line 2, follows /theme); separators are plain characters
    let colors_on = cfg.colors;
    let span = |color: &str, text: &str| {
        if colors_on {
            format!("\x1b[{color}m{text}{RESET}")
        } else {
            text.to_string()
        }
    };
    let sep_seg = if colors_on { SEP_SEG } else { " | " };
    let sep_part = if colors_on { SEP_PART } else { " · " };
    // Iterate over the configured order (reordering takes effect); quota_only processes only the
    // quota group; degradation dropping gitBranch removes only, no reordering
    for field in &cfg.order {
        if opts.quota_only && *field != Field::Quota {
            continue;
        }
        match field {
            Field::PermissionMode => {
                if let Some(mode) = payload.get("permissionMode").and_then(|v| v.as_str())
                    && !mode.is_empty()
                {
                    segs.push(span(permission_mode_color(mode), mode));
                }
            }
            Field::Model => {
                if let Some(model) = payload.get("model").and_then(|v| v.as_str())
                    && !model.is_empty()
                {
                    segs.push(span(CYAN, model));
                }
            }
            // thinking (off gray / effort cyan, SPEC §7.2)
            Field::Thinking => match thinking {
                Some(Thinking::Off) => segs.push(span(GRAY, "off")),
                Some(Thinking::Effort(eff)) => segs.push(span(CYAN, eff)),
                None => {}
            },
            // tasks/agents badges (SPEC §7.1/§7.7 v1.5): the segment text replicates the host's
            // native footer badges (footer.ts:483-494), singular/plural by count; the two badges
            // are joined by a single space into this segment; when both are zero it is omitted; cyan 36 (§7.2)
            Field::Tasks => {
                let mut badges: Vec<String> = Vec::new();
                if tasks.bash > 0 {
                    let noun = if tasks.bash == 1 { "task" } else { "tasks" };
                    badges.push(span(CYAN, &format!("[{} {noun} running]", tasks.bash)));
                }
                if tasks.agent > 0 {
                    let noun = if tasks.agent == 1 { "agent" } else { "agents" };
                    badges.push(span(CYAN, &format!("[{} {noun} running]", tasks.agent)));
                }
                if !badges.is_empty() {
                    segs.push(badges.join(" "));
                }
            }
            // quota group (parts joined by gray · within the group, SPEC §7.3)
            Field::Quota => {
                let mut parts: Vec<String> = Vec::new();
                for p in quota_parts(cached, cfg) {
                    let color = quota_color(p.percent, cfg.green_below, cfg.yellow_below);
                    let reset = if opts.reset && cfg.quota.reset_time {
                        reset_suffix(p.reset_at.as_ref(), now).unwrap_or_default()
                    } else {
                        String::new()
                    };
                    parts.push(span(
                        color,
                        &format!("{} {}%{}", p.label, fmt_percent(p.percent), reset),
                    ));
                }
                // booster is not rendered by default (SPEC §1.2/§7.1); when enabled and Ready, the
                // balance (yuan) is shown in cyan. The format is pure ASCII (avoiding compatibility
                // issues with symbols like ¥ on non-UTF-8 terminals)
                if cfg.quota.booster
                    && let Some(r) = cached.filter(|r| r.error.is_none())
                    && let Some(extra) = &r.extra
                    && extra.state == ExtraState::Ready
                    && let Some(cents) = extra.balance_cents
                {
                    let yuan = cents as f64 / 100.0;
                    parts.push(span(CYAN, &format!("boost {yuan:.2}")));
                }
                if !parts.is_empty() {
                    segs.push(parts.join(sep_part));
                }
            }
            Field::GitBranch if opts.git => {
                if let Some(branch) = payload.get("gitBranch").and_then(|v| v.as_str())
                    && !branch.is_empty()
                {
                    segs.push(span(MAGENTA, branch));
                }
            }
            Field::GitBranch => {}
        }
    }
    segs.join(sep_seg)
}

/// Visible width: approximated by character count after stripping ANSI escape sequences (SPEC §7.4;
/// fields are mostly ASCII, with the host's truncateToWidth as the final fallback).
pub fn visible_width(line: &str) -> usize {
    let mut width = 0usize;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            // consume up to 'm' (we only output SGR sequences)
            while let Some(&c2) = chars.peek() {
                chars.next();
                if c2 == 'm' {
                    break;
                }
            }
        } else {
            width += 1;
        }
    }
    width
}

/// Width-aware degradation ladder (SPEC §7.5): 0 full -> 1 drop reset -> 2 drop gitBranch ->
/// 3 keep only the quota group -> 4 still too wide, output as-is (leave truncation to the host).
/// `kimi_home` feeds the tasks badge count (SPEC §7.7): when order contains "tasks", run the
/// scan once and pass the result to all 4 degradation variants (avoiding 4 directory scans);
/// when order lacks "tasks", skip the scan (saving IO, per the SPEC §8 switch-saves-IO convention); missing home yields zero counts.
pub fn render_line(
    payload: &Value,
    cached: Option<&QuotaResult>,
    cfg: &Config,
    thinking: Option<&Thinking>,
    width: u32,
    now: DateTime<Local>,
    kimi_home: Option<&Path>,
) -> String {
    // Counting is computed once in the outer layer (a single scan within decision D's cap; sessionId
    // comes from payload §3.5; missing/empty/invalid yields zero counts inside tasks.rs)
    let tasks = if cfg.order.contains(&Field::Tasks) {
        let session_id = payload.get("sessionId").and_then(|v| v.as_str());
        kimi_home
            .map(|home| crate::tasks::count_running(home, session_id))
            .unwrap_or_default()
    } else {
        TaskCounts::default()
    };

    // Full line (no suffix by nature when the reset switch is off); degradation removes only, no reordering
    let full = render_variant(
        payload,
        cached,
        cfg,
        thinking,
        tasks,
        now,
        &VariantOpts {
            reset: cfg.quota.reset_time,
            git: true,
            quota_only: false,
        },
    );
    let no_reset = render_variant(
        payload,
        cached,
        cfg,
        thinking,
        tasks,
        now,
        &VariantOpts {
            reset: false,
            git: true,
            quota_only: false,
        },
    );
    let no_git = render_variant(
        payload,
        cached,
        cfg,
        thinking,
        tasks,
        now,
        &VariantOpts {
            reset: false,
            git: false,
            quota_only: false,
        },
    );
    let quota_only = render_variant(
        payload,
        cached,
        cfg,
        thinking,
        tasks,
        now,
        &VariantOpts {
            reset: false,
            git: false,
            quota_only: true,
        },
    );

    // Try each level until one fits; if all overflow -> output as-is (full)
    for cand in [&full, &no_reset, &no_git, &quota_only] {
        if (visible_width(cand) as u32) <= width {
            return cand.clone();
        }
    }
    full
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quota::{ExtraInfo, ExtraState, QuotaSegment};
    use chrono::TimeZone;

    fn seg(percent: f64, reset: Option<&str>) -> QuotaSegment {
        QuotaSegment {
            percent,
            reset_at: reset.map(|s| {
                DateTime::parse_from_rfc3339(s)
                    .unwrap()
                    .with_timezone(&Local)
            }),
        }
    }

    fn cached_full() -> QuotaResult {
        QuotaResult {
            five_hour: Some(seg(21.0, Some("2030-01-01T00:00:00+08:00"))),
            week: Some(seg(68.0, Some("2030-01-08T00:00:00+08:00"))),
            month: Some(seg(43.0, Some("2030-11-05T00:00:00+08:00"))),
            extra: Some(ExtraInfo {
                state: ExtraState::Ready,
                balance_cents: Some(1235),
                monthly_enabled: false,
                monthly_used_cents: None,
                monthly_limit_cents: None,
            }),
            fetched_at: Local.timestamp_millis_opt(1_893_456_000_000).unwrap(),
            error: None,
        }
    }

    fn payload_full() -> Value {
        serde_json::json!({
            "model": "Kimi",
            "permissionMode": "yolo",
            "gitBranch": "main"
        })
    }

    fn now() -> DateTime<Local> {
        Local.timestamp_millis_opt(1_893_456_000_000).unwrap() // 2030-01-01 08:00 +08:00
    }

    /// Color threshold boundaries (PLAN P3 unit test list): 59.x green / 60 yellow / 84.x yellow / 85 red.
    #[test]
    fn color_threshold_boundaries() {
        assert_eq!(quota_color(59.9, 60.0, 85.0), "32");
        assert_eq!(quota_color(60.0, 60.0, 85.0), "33");
        assert_eq!(quota_color(84.9, 60.0, 85.0), "33");
        assert_eq!(quota_color(85.0, 60.0, 85.0), "31");
    }

    /// percent rounding boundaries (%.0f = round-half-to-even): 60.5->60, 61.5->62.
    #[test]
    fn percent_rounds_half_to_even() {
        assert_eq!(fmt_percent(60.5), "60");
        assert_eq!(fmt_percent(61.5), "62");
        assert_eq!(fmt_percent(0.5), "0");
        assert_eq!(fmt_percent(1.5), "2");
        assert_eq!(fmt_percent(21.0), "21");
    }

    /// reset cross-day format (PLAN P3 unit test list): same day HH:MM, cross day MM/DD HH:MM.
    #[test]
    fn reset_suffix_same_and_cross_day() {
        let n = now(); // 2030-01-01 08:00 local
        let same = DateTime::parse_from_rfc3339("2030-01-01T23:30:00+08:00")
            .unwrap()
            .with_timezone(&Local);
        let cross = DateTime::parse_from_rfc3339("2030-01-02T00:00:00+08:00")
            .unwrap()
            .with_timezone(&Local);
        assert_eq!(reset_suffix(Some(&same), n).unwrap(), " (rst 23:30)");
        assert_eq!(reset_suffix(Some(&cross), n).unwrap(), " (rst 01/02 00:00)");
        assert_eq!(reset_suffix(None, n), None);
    }

    /// thinking ladder (PLAN P3 unit test list): enabled=false / effort / models
    /// two-level fallback / all missing -> omitted.
    #[test]
    fn thinking_ladder_branches() {
        let text = r#"
[thinking]
enabled = false
[models.kimi]
display_name = "Kimi"
overrides.default_effort = "high"
"#;
        assert_eq!(
            thinking_from_config(Some(text), Some("Kimi")),
            Some(Thinking::Off)
        );

        let text = "[thinking]\neffort = \"medium\"\n";
        assert_eq!(
            thinking_from_config(Some(text), Some("Kimi")),
            Some(Thinking::Effort("medium".into()))
        );

        // models fallback: overrides.default_effort takes priority
        let text = r#"
[models.kimi]
display_name = "Kimi"
[models.kimi.overrides]
default_effort = "high"
"#;
        assert_eq!(
            thinking_from_config(Some(text), Some("Kimi")),
            Some(Thinking::Effort("high".into()))
        );

        // models fallback: fall back further to default_effort
        let text = r#"
[models.other]
model = "kimi-model"
default_effort = "low"
"#;
        assert_eq!(
            thinking_from_config(Some(text), Some("kimi-model")),
            Some(Thinking::Effort("low".into()))
        );

        // all missing -> omitted
        assert_eq!(
            thinking_from_config(Some("[thinking]\n"), Some("Kimi")),
            None
        );
        assert_eq!(thinking_from_config(None, Some("Kimi")), None);
        // config.toml entirely invalid -> omitted
        assert_eq!(thinking_from_config(Some("not toml"), Some("Kimi")), None);
    }

    /// Line assembly (SPEC §7.4): segment order, color codes, gray separators, per-segment reset.
    #[test]
    fn full_line_layout() {
        let cfg = Config::default();
        let c = cached_full();
        let line = render_line(
            &payload_full(),
            Some(&c),
            &cfg,
            Some(&Thinking::Effort("high".into())),
            200,
            now(),
            None,
        );
        assert!(
            line.starts_with("\x1b[31myolo\x1b[0m"),
            "yolo red: {line:?}"
        );
        assert!(line.contains("\x1b[36mKimi\x1b[0m"), "model cyan");
        assert!(line.contains("\x1b[36mhigh\x1b[0m"), "thinking cyan");
        assert!(
            line.contains("\x1b[32m5h 21%"),
            "5h green + no-space concatenation"
        );
        assert!(line.contains("(rst 00:00)"), "same-day reset");
        assert!(line.contains("\x1b[33mweek 68%"), "68 yellow");
        assert!(
            line.contains("\x1b[32mmonth 43% (rst 11/05 00:00)"),
            "cross-day reset: {line:?}"
        );
        assert!(line.ends_with("\x1b[35mmain\x1b[0m"), "git magenta");
        assert_eq!(
            line.matches(" \x1b[90m|\x1b[0m ").count(),
            4,
            "4 gray | between segments"
        );
        assert_eq!(
            line.matches(" \x1b[90m·\x1b[0m ").count(),
            2,
            "2 gray · within the group"
        );
        // line contains no contextTokens / maxContextTokens (SPEC §3.4)
        assert!(!line.contains("context"));
    }

    /// Four-level width degradation (PLAN P3 unit test list): drop reset -> drop gitBranch ->
    /// keep only the quota group -> output as-is.
    #[test]
    fn width_degradation_ladder() {
        let cfg = Config::default();
        let c = cached_full();
        let full = render_line(
            &payload_full(),
            Some(&c),
            &cfg,
            Some(&Thinking::Effort("high".into())),
            400,
            now(),
            None,
        );
        assert!(full.contains("(rst"), "full contains reset");
        assert!(full.contains("\x1b[35mmain"), "full contains git");

        // Level 1: width < full -> drop reset
        let w1 = visible_width(&full) as u32 - 1;
        let l1 = render_line(
            &payload_full(),
            Some(&c),
            &cfg,
            Some(&Thinking::Effort("high".into())),
            w1,
            now(),
            None,
        );
        assert!(!l1.contains("(rst"), "level 1 drops reset");
        assert!(l1.contains("\x1b[35mmain"), "level 1 still contains git");

        // Level 2: shrink again -> drop gitBranch
        let w2 = visible_width(&l1) as u32 - 1;
        let l2 = render_line(
            &payload_full(),
            Some(&c),
            &cfg,
            Some(&Thinking::Effort("high".into())),
            w2,
            now(),
            None,
        );
        assert!(!l2.contains("\x1b[35m"), "level 2 drops git");
        assert!(l2.contains("5h"), "level 2 still contains the quota group");
        assert!(l2.contains("\x1b[36mKimi"), "level 2 still contains model");

        // Level 3: keep only the quota group
        let w3 = visible_width(&l2) as u32 - 1;
        let l3 = render_line(
            &payload_full(),
            Some(&c),
            &cfg,
            Some(&Thinking::Effort("high".into())),
            w3,
            now(),
            None,
        );
        assert!(l3.contains("5h"), "level 3 contains the quota group");
        assert!(!l3.contains("\x1b[36mKimi"), "level 3 drops model");
        assert!(
            l3.starts_with("\x1b[32m5h"),
            "level 3 starts with the quota group"
        );

        // Level 4: extremely narrow width -> output as-is (equals full)
        let l4 = render_line(
            &payload_full(),
            Some(&c),
            &cfg,
            Some(&Thinking::Effort("high".into())),
            1,
            now(),
            None,
        );
        assert_eq!(
            l4, full,
            "level 4 outputs as-is, leaving truncation to the host"
        );
    }

    /// reset_time=false is equivalent to degradation level 1 (SPEC §8).
    #[test]
    fn reset_time_config_off() {
        let cfg = parse_minimal("[render.quota]\nreset_time = false\n");
        let c = cached_full();
        let line = render_line(&payload_full(), Some(&c), &cfg, None, 400, now(), None);
        assert!(
            !line.contains("(rst"),
            "reset_time=false permanently drops reset"
        );
        assert!(
            line.contains("\x1b[35mmain"),
            "other fields render normally"
        );
    }

    fn parse_minimal(text: &str) -> Config {
        crate::config::parse(text)
    }

    /// Empty/invalid payload: renders an empty line when no field is renderable (host falls back, SPEC §4.1 step 5).
    #[test]
    fn empty_payload_renders_empty_line() {
        let cfg = Config::default();
        let empty = render_line(&serde_json::json!({}), None, &cfg, None, 120, now(), None);
        assert_eq!(empty, "");
        // a missing quota-group cache is omitted the same way
        let null_payload = render_line(&Value::Null, None, &cfg, None, 120, now(), None);
        assert_eq!(null_payload, "");
    }

    /// UTF-8 lossy (SPEC §7.6): invalid bytes -> U+FFFD, no crash; invalid JSON -> empty payload.
    #[test]
    fn payload_lossy_and_invalid() {
        let v = payload_from_bytes(b"\xff\xfe not json");
        assert_eq!(v, Value::Null);

        // invalid UTF-8 bytes inside a valid JSON carrier: parses fine after lossy replacement
        let mut bytes = b"{\"model\":\"".to_vec();
        bytes.extend_from_slice(&[0xff]);
        bytes.extend_from_slice(b"\"}");
        let v = payload_from_bytes(&bytes);
        assert_eq!(v.get("model").and_then(|m| m.as_str()), Some("\u{FFFD}"));
    }

    /// order reordering takes effect; a cache error on the quota group omits the whole group (defensive).
    #[test]
    fn order_and_error_cache() {
        let cfg = parse_minimal("[render]\norder = [\"quota\", \"permission_mode\"]\n");
        let c = cached_full();
        let line = render_line(&payload_full(), Some(&c), &cfg, None, 400, now(), None);
        assert!(
            line.starts_with("\x1b[32m5h"),
            "quota group comes first: {line:?}"
        );

        let mut err = cached_full();
        err.error = Some("HttpRequestException".into());
        let line = render_line(
            &payload_full(),
            Some(&err),
            &Config::default(),
            None,
            400,
            now(),
            None,
        );
        assert!(
            !line.contains("5h"),
            "error cache does not render the quota group"
        );
        assert!(line.contains("\x1b[31myolo"), "other fields as usual");
    }

    /// colors=false monochrome rendering (SPEC §7.2 v1.4): output contains no SGR; visible content
    /// (segment text, order, reset suffix, plain-character separators) matches the colored version; the degradation ladder still applies.
    #[test]
    fn monochrome_strips_all_sgr() {
        let cfg = Config {
            colors: false,
            ..Config::default()
        };
        let c = cached_full();
        let line = render_line(
            &payload_full(),
            Some(&c),
            &cfg,
            Some(&Thinking::Effort("high".into())),
            400,
            now(),
            None,
        );
        assert!(
            !line.contains('\x1b'),
            "monochrome output must not contain SGR: {line:?}"
        );
        for piece in [
            "yolo",
            "Kimi",
            "high",
            "5h 21%",
            "(rst",
            "week 68%",
            "month 43%",
            "main",
        ] {
            assert!(line.contains(piece), "missing {piece}: {line:?}");
        }
        assert!(
            line.contains(" | "),
            "plain-character segment separator: {line:?}"
        );
        assert!(
            line.contains(" · "),
            "plain-character intra-group separator: {line:?}"
        );
        // Monochrome x degradation-ladder combination (PLAN P6): at width=30 it degrades to "keep only
        // the quota group" (no reset suffix, visible width exactly 29); output must still have zero SGR
        let degraded = render_line(
            &payload_full(),
            Some(&c),
            &cfg,
            Some(&Thinking::Effort("high".into())),
            30,
            now(),
            None,
        );
        assert!(
            !degraded.contains('\x1b'),
            "must remain monochrome after degradation: {degraded:?}"
        );
        assert!(
            !degraded.contains("main") && !degraded.contains("(rst"),
            "degraded: git/reset dropped"
        );
        assert!(
            degraded.starts_with("5h 21%"),
            "quota group only: {degraded:?}"
        );
        // character-for-character comparable with the colored version (colored = monochrome + ANSI wrapping)
        let color_cfg = Config {
            colors: true,
            ..Config::default()
        };
        let colored = render_line(
            &payload_full(),
            Some(&c),
            &color_cfg,
            Some(&Thinking::Effort("high".into())),
            400,
            now(),
            None,
        );
        let mut plain = String::new();
        let mut chars = colored.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\x1b' && chars.peek() == Some(&'[') {
                chars.next();
                while let Some(&c2) = chars.peek() {
                    chars.next();
                    if c2 == 'm' {
                        break;
                    }
                }
            } else {
                plain.push(ch);
            }
        }
        assert_eq!(
            line, plain,
            "monochrome visible content must match the colored version"
        );
    }

    /// visible_width counts characters after stripping ANSI.
    #[test]
    fn visible_width_strips_ansi() {
        assert_eq!(visible_width("\x1b[31myolo\x1b[0m"), 4);
        assert_eq!(visible_width("a\x1b[90m|\x1b[0mb"), 3);
        assert_eq!(visible_width(""), 0);
    }

    /// booster rendering: off by default and not shown; when enabled and Ready, the balance (yuan) is shown in ASCII.
    #[test]
    fn booster_render_switch_and_ascii_format() {
        let c = cached_full(); // balanceCents 1235 -> 12.35 yuan
        let off = render_line(
            &payload_full(),
            Some(&c),
            &Config::default(),
            None,
            400,
            now(),
            None,
        );
        assert!(!off.contains("boost"), "booster not rendered by default");

        let cfg = parse_minimal("[render.quota]\nbooster = true\n");
        let on = render_line(&payload_full(), Some(&c), &cfg, None, 400, now(), None);
        assert!(
            on.contains("\x1b[36mboost 12.35\x1b[0m"),
            "booster ASCII balance: {on:?}"
        );

        // not Ready (NoData/NotActivated): not shown
        let mut nodata = cached_full();
        nodata.extra.as_mut().unwrap().state = ExtraState::NoData;
        let line = render_line(&payload_full(), Some(&nodata), &cfg, None, 400, now(), None);
        assert!(!line.contains("boost"), "NoData shows no balance");
    }

    // ---- tasks/agents badges (SPEC §7.1/§7.7 v1.5, PLAN P7 render-side unit tests) ----

    /// Isolated temporary <kimi_home> (same approach as the tasks.rs tests, distinguished by prefix)
    fn temp_home(tag: &str) -> std::path::PathBuf {
        let base = std::env::temp_dir().join(format!("qs-render-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        base
    }

    /// Write a task json to disk: <home>/sessions/wd_a/<sid>/agents/main/tasks/<task>.json
    fn write_task(home: &std::path::Path, sid: &str, task: &str, json: &str) {
        let dir = home
            .join("sessions")
            .join("wd_a")
            .join(sid)
            .join("agents")
            .join("main")
            .join("tasks");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{task}.json")), json).unwrap();
    }

    fn agent_running_json() -> &'static str {
        r#"{"taskId":"t","status":"running","kind":"agent","startedAt":1}"#
    }

    /// Render position (SPEC §10.1 v1.5): the tasks segment comes before the quota group; the two
    /// badges join with a single space into one segment (each span-wrapped, same per-badge chalk
    /// semantics as the host); singular/plural by count (1 task / 2 agents).
    #[test]
    fn tasks_badge_before_quota_and_plural() {
        let home = temp_home("position");
        // 1 bash (pid = this test process, guaranteed alive) + 2 agents
        write_task(
            &home,
            "s",
            "t1",
            &format!(
                r#"{{"status":"running","kind":"process","pid":{}}}"#,
                std::process::id()
            ),
        );
        write_task(&home, "s", "a1", agent_running_json());
        write_task(&home, "s", "a2", agent_running_json());
        let line = render_line(
            &serde_json::json!({"sessionId": "s"}),
            Some(&cached_full()),
            &Config::default(),
            None,
            400,
            now(),
            Some(&home),
        );
        assert!(
            line.contains("\x1b[36m[1 task running]\x1b[0m \x1b[36m[2 agents running]\x1b[0m"),
            "single-space join (each span-wrapped, same as the host's per-badge chalk) + singular/plural: {line:?}"
        );
        let badge = line.find("[1 task running]").unwrap();
        let quota = line.find("5h").unwrap();
        assert!(
            badge < quota,
            "tasks must come before the quota group: {line:?}"
        );
        std::fs::remove_dir_all(&home).ok();
    }

    /// colors=false monochrome combination (SPEC §10.1 v1.5): no SGR at all, the two badges
    /// joined by a single space, plain-character separators.
    #[test]
    fn tasks_badge_monochrome() {
        let home = temp_home("mono");
        write_task(
            &home,
            "s",
            "t1",
            &format!(
                r#"{{"status":"running","kind":"process","pid":{}}}"#,
                std::process::id()
            ),
        );
        write_task(&home, "s", "a1", agent_running_json());
        let cfg = Config {
            colors: false,
            ..Config::default()
        };
        let line = render_line(
            &serde_json::json!({"sessionId": "s", "model": "Kimi"}),
            None,
            &cfg,
            None,
            400,
            now(),
            Some(&home),
        );
        assert!(
            !line.contains('\x1b'),
            "monochrome output must not contain SGR: {line:?}"
        );
        assert!(
            line.contains("[1 task running] [1 agent running]"),
            "single-space join: {line:?}"
        );
        assert!(
            line.contains(" | "),
            "plain-character segment separator: {line:?}"
        );
        std::fs::remove_dir_all(&home).ok();
    }

    /// Degradation level 3 (quota group only) drops the tasks segment via the existing skip logic (removal only, no reordering).
    #[test]
    fn tasks_badge_dropped_in_quota_only_degradation() {
        let home = temp_home("degrade");
        write_task(&home, "s", "a1", agent_running_json());
        let payload = serde_json::json!({"model": "Kimi", "sessionId": "s", "gitBranch": "main"});
        let c = cached_full();
        let cfg = Config::default();

        // Narrow the width step by step to the level-3 trigger point (same technique as width_degradation_ladder)
        let full = render_line(&payload, Some(&c), &cfg, None, 400, now(), Some(&home));
        let l1 = render_line(
            &payload,
            Some(&c),
            &cfg,
            None,
            visible_width(&full) as u32 - 1,
            now(),
            Some(&home),
        );
        assert!(
            l1.contains("[1 agent running]"),
            "level 1 (drop reset) still contains the badge"
        );
        let l2 = render_line(
            &payload,
            Some(&c),
            &cfg,
            None,
            visible_width(&l1) as u32 - 1,
            now(),
            Some(&home),
        );
        assert!(
            l2.contains("[1 agent running]"),
            "level 2 (drop git) still contains the badge: {l2:?}"
        );
        let l3 = render_line(
            &payload,
            Some(&c),
            &cfg,
            None,
            visible_width(&l2) as u32 - 1,
            now(),
            Some(&home),
        );
        assert!(
            l3.starts_with("\x1b[32m5h"),
            "level 3 starts with the quota group: {l3:?}"
        );
        assert!(!l3.contains("running"), "level 3 drops the tasks badge");
        assert!(!l3.contains("Kimi"), "level 3 drops model");
        std::fs::remove_dir_all(&home).ok();
    }

    /// Scan-cap truncation does not trigger segment omission (SPEC §10.1 v1.5): with >32 task jsons,
    /// the badge renders the count actually read after truncation (40 -> "[32 agents running]").
    #[test]
    fn tasks_badge_renders_truncated_counts() {
        let home = temp_home("cap-render");
        for i in 0..40 {
            write_task(&home, "s", &format!("a{i:02}"), agent_running_json());
        }
        let line = render_line(
            &serde_json::json!({"sessionId": "s"}),
            None,
            &Config::default(),
            None,
            400,
            now(),
            Some(&home),
        );
        assert!(
            line.contains("[32 agents running]"),
            "truncated count still renders: {line:?}"
        );
        std::fs::remove_dir_all(&home).ok();
    }

    /// Render-side defense combinations (SPEC §9 v1.5 row): a traversal sessionId / missing sessions
    /// directory -> no badge, other fields normal, no panic; removing "tasks" from order turns the
    /// segment off (and skips the scan along with it, SPEC §8).
    #[test]
    fn tasks_badge_absent_on_defense_and_switch_off() {
        let home = temp_home("defense");
        write_task(&home, "s", "a1", agent_running_json());
        let cfg = Config::default();

        // traversal string -> no badge, model normal
        let line = render_line(
            &serde_json::json!({"sessionId": "../evil", "model": "Kimi"}),
            None,
            &cfg,
            None,
            400,
            now(),
            Some(&home),
        );
        assert!(!line.contains("running"));
        assert!(line.contains("\x1b[36mKimi\x1b[0m"));

        // no hit (no such sessionId under sessions) -> no badge
        let line = render_line(
            &serde_json::json!({"sessionId": "other"}),
            None,
            &cfg,
            None,
            400,
            now(),
            Some(&home),
        );
        assert!(!line.contains("running"));

        // order lacks "tasks" -> whole segment off (scan skipped, kimi_home still passed)
        let cfg_no_tasks = parse_minimal("[render]\norder = [\"model\", \"quota\"]\n");
        let line = render_line(
            &serde_json::json!({"sessionId": "s"}),
            Some(&cached_full()),
            &cfg_no_tasks,
            None,
            400,
            now(),
            Some(&home),
        );
        assert!(!line.contains("running"), "switch off, no badge: {line:?}");
        assert!(line.contains("5h"), "other fields render normally");
        std::fs::remove_dir_all(&home).ok();
    }
}
