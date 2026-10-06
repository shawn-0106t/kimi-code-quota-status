// Quota fetching + defensive parsing, ported from repos/kimi-planbar-tui/rust/src/quota.rs (SPEC §2.2),
// with three changes per SPEC v1.1:
//  - 5h segment upgraded from "take limits[0] only" to window-conditional matching (§6.4)
//  - new month segment (root.totalQuota, §6.6)
//  - QuotaResult extended with a month field, skipped on serialization when None (§5.3, existing goldens unchanged)
// Defensive rules aligned one by one with the ten SPEC §6 rules (numbers modeled as strings, limit<=0 clamped to 1,
// NaN/inf zeroed, i64::MIN parseable, resetTime lenient ladder, boosterWallet defense, 1e-8 yuan unit conversion).

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::credentials;
use crate::http;

const DEFAULT_BASE_URL: &str = "https://api.kimi.com/coding/v1";

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaSegment {
    pub percent: f64,
    pub reset_at: Option<DateTime<Local>>,
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub enum ExtraState {
    NotActivated,
    NoData,
    Ready,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtraInfo {
    pub state: ExtraState,
    pub balance_cents: Option<i64>,
    pub monthly_enabled: bool,
    pub monthly_used_cents: Option<i64>,
    pub monthly_limit_cents: Option<i64>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaResult {
    pub five_hour: Option<QuotaSegment>,
    pub week: Option<QuotaSegment>,
    /// Monthly quota segment (added by this tool, SPEC §6.6). When None, the whole
    /// key is not serialized; the existing 30 goldens (no month key) stay byte-identical (SPEC §10.2).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub month: Option<QuotaSegment>,
    pub extra: Option<ExtraInfo>,
    pub fetched_at: DateTime<Local>,
    pub error: Option<String>,
}

impl QuotaResult {
    fn failed(kind: &str, now: DateTime<Local>) -> Self {
        QuotaResult {
            five_hour: None,
            week: None,
            month: None,
            extra: None,
            fetched_at: now,
            error: Some(kind.to_string()),
        }
    }

    /// On failure keep last-known-good data: fill null fields from `last`
    /// (SPEC 16.5 step 2 / §5.3).
    pub fn fill_missing_from(&mut self, last: &QuotaResult) {
        if self.five_hour.is_none() {
            self.five_hour = last.five_hour.clone();
        }
        if self.week.is_none() {
            self.week = last.week.clone();
        }
        if self.month.is_none() {
            self.month = last.month.clone();
        }
        if self.extra.is_none() {
            self.extra = last.extra.clone();
        }
    }

    /// Serialization shared by the --test-fetch output and golden comparison (SPEC §10.2):
    /// serde pretty, 2-space indent, camelCase. The parse path guarantees a finite
    /// percent, so serialization cannot fail; the fallback is pure defense (no unwinding under panic=abort).
    pub fn to_pretty_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// base_url override chain (SPEC §5.1): env KIMI_CODE_BASE_URL > quota-bar.toml
/// [network] base_url > default. `configured` is the base_url parsed from config.
pub fn resolve_base_url(configured: Option<&str>) -> String {
    if let Ok(v) = std::env::var("KIMI_CODE_BASE_URL")
        && !v.is_empty()
    {
        return v;
    }
    configured
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
}

/// Fetch entry point (base_url/timeout provided by config, SPEC §5.1/§8).
pub fn fetch(configured_base_url: Option<&str>, timeout_secs: u64) -> QuotaResult {
    fetch_from(&resolve_base_url(configured_base_url), timeout_secs)
}

/// Core flow of fetch mode (SPEC §4.2 steps 1-3; cache writing is the caller's job).
/// Error type names keep the .NET style (SPEC §6.10):
/// timeout -> TaskCanceledException; non-2xx and other transport errors -> HttpRequestException;
/// invalid response body -> JsonException; no credential -> no-token.
pub fn fetch_from(base_url: &str, timeout_secs: u64) -> QuotaResult {
    let Some(token) = credentials::load_token() else {
        return QuotaResult::failed("no-token", Local::now());
    };
    let agent = http::shared_client(std::time::Duration::from_secs(timeout_secs));
    let url = format!("{}/usages", base_url.trim_end_matches('/'));
    let resp = match agent
        .get(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/json")
        .call()
    {
        Ok(r) => r,
        Err(e) => return QuotaResult::failed(classify_send_error(&e), Local::now()),
    };
    let text = match resp.into_body().read_to_string() {
        Ok(t) => t,
        // a body-stage timeout (headers already received, body stalled) is also
        // TaskCanceledException; the rest (incl. invalid UTF-8) counts as JsonException
        Err(ureq::Error::Timeout(_)) => {
            return QuotaResult::failed("TaskCanceledException", Local::now());
        }
        Err(ureq::Error::Io(e)) if e.kind() == std::io::ErrorKind::TimedOut => {
            return QuotaResult::failed("TaskCanceledException", Local::now());
        }
        Err(_) => return QuotaResult::failed("JsonException", Local::now()),
    };
    let root: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return QuotaResult::failed("JsonException", Local::now()),
    };
    parse_payload(&root, Local::now())
}

/// Send-stage error classification (SPEC §5.1/§6.10, .NET-style type names): timeout ->
/// TaskCanceledException; non-2xx (ureq defaults to http_status_as_error, including
/// the 404 of pure API key accounts) and other transport errors -> HttpRequestException.
fn classify_send_error(e: &ureq::Error) -> &'static str {
    match e {
        ureq::Error::Timeout(_) => "TaskCanceledException",
        _ => "HttpRequestException",
    }
}

/// Defensive parsing (SPEC §6). `now` is injected as a parameter: the production path
/// passes Local::now(), golden tests pass a fixed clock (a P4 parity prerequisite, avoiding rework).
pub fn parse_payload(root: &Value, now: DateTime<Local>) -> QuotaResult {
    let mut r = QuotaResult {
        five_hour: None,
        week: None,
        month: None,
        extra: None,
        fetched_at: now,
        error: None,
    };
    // 5h segment: window-conditional match within limits[] (SPEC §6.4); when detail is
    // not an object, parse_segment falls back to all zeros (aligned with the reference impl, golden quota-detail_string)
    if let Some(detail) = root
        .get("limits")
        .and_then(|l| l.as_array())
        .and_then(|a| pick_five_hour(a))
        .and_then(|first| first.get("detail"))
    {
        r.five_hour = Some(parse_segment(detail));
    }
    // week segment: top-level root.usage (parsed only when an object, SPEC §6.5)
    if let Some(u) = root.get("usage")
        && u.is_object()
    {
        r.week = Some(parse_segment(u));
    }
    // month segment: root.totalQuota; no segment when limit is missing/0/non-finite (NaN etc.)
    // (SPEC §6.6/§6.3; NaN != 0.0 is always true, so it must be excluded explicitly)
    if let Some(t) = root.get("totalQuota") {
        let limit = get_f64(t, "limit");
        if t.is_object() && limit.is_finite() && limit != 0.0 {
            r.month = Some(parse_segment(t));
        }
    }
    r.extra = Some(parse_extra(root.get("boosterWallet")));
    r
}

/// 5h segment window selection (SPEC §6.4): prefer the element whose window.duration
/// == 300 and window.timeUnit == "TIME_UNIT_MINUTE" (duration string-modeled too); fall back to limits[0].
fn pick_five_hour(limits: &[Value]) -> Option<&Value> {
    limits
        .iter()
        .find(|l| {
            l.get("window")
                .map(|w| {
                    get_f64(w, "duration") == 300.0
                        && w.get("timeUnit").and_then(Value::as_str) == Some("TIME_UNIT_MINUTE")
                })
                .unwrap_or(false)
        })
        .or_else(|| limits.first())
}

/// JSON number-or-string -> f64, missing -> 0.
fn get_f64(v: &Value, key: &str) -> f64 {
    match v.get(key) {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(Value::String(s)) => s.trim().parse::<f64>().unwrap_or(0.0),
        _ => 0.0,
    }
}

/// JSON number-or-string -> i64.
fn get_i64(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.trim().parse::<i64>().ok(),
        _ => None,
    }
}

/// resetTime: RFC3339 first, then the looser shapes DateTimeOffset.TryParse
/// accepts (space separator, with/without offset; no offset = local time).
fn parse_reset_time(s: &str) -> Option<DateTime<Local>> {
    if let Ok(d) = DateTime::parse_from_rfc3339(s) {
        return Some(d.with_timezone(&Local));
    }
    for fmt in [
        "%Y-%m-%d %H:%M:%S %:z",
        "%Y-%m-%d %H:%M:%S %z",
        "%Y-%m-%dT%H:%M:%S %:z",
        "%Y-%m-%d %H:%M:%S%.f %:z",
    ] {
        if let Ok(d) = DateTime::parse_from_str(s, fmt) {
            return Some(d.with_timezone(&Local));
        }
    }
    for fmt in ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S"] {
        if let Ok(n) = chrono::NaiveDateTime::parse_from_str(s, fmt)
            && let Some(d) = n.and_local_timezone(Local).single()
        {
            return Some(d);
        }
    }
    None
}

fn parse_segment(v: &Value) -> QuotaSegment {
    let used = get_f64(v, "used");
    let mut limit = get_f64(v, "limit");
    if limit <= 0.0 {
        limit = 1.0; // guard against division by zero
    }
    let reset_at = v
        .get("resetTime")
        .and_then(|x| x.as_str())
        .and_then(parse_reset_time);
    // get_f64 can yield inf/NaN from hostile strings ("1e999", "NaN"); keep
    // percent finite so serialization never emits a non-finite double
    let percent = used / limit * 100.0;
    QuotaSegment {
        percent: if percent.is_finite() { percent } else { 0.0 },
        reset_at,
    }
}

fn parse_cents(money: Option<&Value>) -> Option<i64> {
    money
        .filter(|m| m.is_object())
        .and_then(|m| m.get("priceInCents"))
        .and_then(get_i64)
}

fn parse_extra(wallet: Option<&Value>) -> ExtraInfo {
    let mut info = ExtraInfo {
        state: ExtraState::NotActivated,
        balance_cents: None,
        monthly_enabled: false,
        monthly_used_cents: None,
        monthly_limit_cents: None,
    };
    let Some(w) = wallet.filter(|w| w.is_object()) else {
        return info; // not an object -> NotActivated
    };

    // isEnabled defense: when the booster is disabled, amountLeft is an estimate
    // (monthly limit minus used), NOT the real balance -> must read as NotActivated.
    // Only strict boolean false triggers; non-bool values like string "false" or number 0 continue on the enabled path.
    if w.get("isEnabled").and_then(|v| v.as_bool()) == Some(false) {
        return info;
    }

    if let Some(raw) = w
        .get("balance")
        .filter(|b| b.is_object())
        .and_then(|b| b.get("amountLeft"))
        .and_then(get_i64)
    {
        info.state = ExtraState::Ready;
        // saturating_add: a pathological amountLeft near i64::MAX must not overflow
        info.balance_cents = Some(raw.saturating_add(500_000) / 1_000_000); // 1e-8 yuan -> cents, rounded
    } else {
        info.state = ExtraState::NoData;
    }

    if w.get("monthlyChargeLimitEnabled").and_then(|v| v.as_bool()) == Some(true) {
        info.monthly_enabled = true;
        info.monthly_used_cents = parse_cents(w.get("monthlyUsed"));
        info.monthly_limit_cents = parse_cents(w.get("monthlyChargeLimit"));
    }
    info
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Timelike};
    use serde_json::json;

    /// Golden fixed clock (quota_test.go:14-15):
    /// atZero = 1893456000000ms exactly; atFracs = +123456789ns.
    /// In the +08:00 local timezone these serialize to "2030-01-01T08:00:00+08:00"
    /// and "2030-01-01T08:00:00.123456789+08:00" respectively.
    fn at_fracs() -> DateTime<Local> {
        Local
            .timestamp_millis_opt(1_893_456_000_000)
            .unwrap()
            .with_nanosecond(123_456_789)
            .unwrap()
    }

    /// SPEC 16.3: numbers may arrive as strings or as real JSON numbers;
    /// both shapes must parse identically.
    #[test]
    fn segment_accepts_mixed_string_and_number_fields() {
        let as_strings = parse_segment(&json!({
            "used": "68", "limit": "100",
            "resetTime": "2030-01-01T00:00:00+08:00"
        }));
        let as_numbers = parse_segment(&json!({
            "used": 68, "limit": 100,
            "resetTime": "2030-01-01T00:00:00+08:00"
        }));
        assert_eq!(as_strings.percent, 68.0);
        assert_eq!(as_numbers.percent, 68.0);
        assert!(as_strings.reset_at.is_some());
        assert!(as_numbers.reset_at.is_some());
    }

    /// SPEC 16.3: limit <= 0 is clamped to 1 so percent stays finite and
    //  the division-by-zero guard never yields inf/NaN.
    #[test]
    fn segment_guards_division_by_zero() {
        let seg = parse_segment(&json!({ "used": "50", "limit": "0" }));
        assert!(seg.percent.is_finite());
        assert_eq!(seg.percent, 5000.0); // 50 / 1 * 100

        let hostile = parse_segment(&json!({ "used": "1e999", "limit": 1 }));
        assert_eq!(hostile.percent, 0.0); // inf -> clamped to 0

        let missing = parse_segment(&json!({}));
        assert_eq!(missing.percent, 0.0);
        assert!(missing.reset_at.is_none());
    }

    /// SPEC 16.3: isEnabled == false means amountLeft is only an estimate,
    /// so the whole card must read NotActivated regardless of balance data.
    #[test]
    fn disabled_wallet_is_not_activated() {
        let info = parse_extra(Some(&json!({
            "isEnabled": false,
            "balance": { "amountLeft": "123456789" }
        })));
        assert_eq!(info.state, ExtraState::NotActivated);
        assert_eq!(info.balance_cents, None);

        // Missing / non-object wallet -> NotActivated too
        assert_eq!(parse_extra(None).state, ExtraState::NotActivated);
        assert_eq!(
            parse_extra(Some(&json!("nope"))).state,
            ExtraState::NotActivated
        );
    }

    /// SPEC 6.8: the isEnabled defense triggers only on strict boolean false; string
    /// "false" and number 0 do not trigger, parsing continues on the enabled path (golden isenabled_string_false / isenabled_zero).
    #[test]
    fn isenabled_non_bool_does_not_trigger_defense() {
        let as_str = parse_extra(Some(&json!({
            "isEnabled": "false", "balance": { "amountLeft": "123456789" }
        })));
        assert_eq!(as_str.state, ExtraState::Ready);
        assert_eq!(as_str.balance_cents, Some(123));

        let as_zero = parse_extra(Some(&json!({
            "isEnabled": 0, "balance": { "amountLeft": "1500000" }
        })));
        assert_eq!(as_zero.state, ExtraState::Ready);
        assert_eq!(as_zero.balance_cents, Some(2)); // 1.5 cents -> rounds up
    }

    /// SPEC 16.3: amountLeft is in 1e-8 yuan; cents = (raw + 500000) / 1000000
    /// (round half up). String and numeric shapes both accepted.
    #[test]
    fn amount_left_unit_conversion_rounds() {
        // 123456789 * 1e-8 yuan = 1.23456789 yuan = 123.456789 cents -> 123
        let info = parse_extra(Some(&json!({
            "isEnabled": true,
            "balance": { "amountLeft": "123456789" }
        })));
        assert_eq!(info.state, ExtraState::Ready);
        assert_eq!(info.balance_cents, Some(123));

        // 1500000 * 1e-8 yuan = 0.015 yuan = 1.5 cents -> rounds up to 2
        let info = parse_extra(Some(&json!({
            "isEnabled": true,
            "balance": { "amountLeft": 1500000 }
        })));
        assert_eq!(info.balance_cents, Some(2));

        // Unparseable amountLeft -> NoData
        let info = parse_extra(Some(&json!({
            "isEnabled": true,
            "balance": { "amountLeft": "not-a-number" }
        })));
        assert_eq!(info.state, ExtraState::NoData);
        assert_eq!(info.balance_cents, None);
    }

    /// SPEC 16.3: monthly fields come from priceInCents (already cents,
    /// string-modeled), only when monthlyChargeLimitEnabled is true.
    #[test]
    fn monthly_charge_fields() {
        let info = parse_extra(Some(&json!({
            "isEnabled": true,
            "balance": { "amountLeft": "100000000" },
            "monthlyChargeLimitEnabled": true,
            "monthlyUsed": { "priceInCents": "4567" },
            "monthlyChargeLimit": { "priceInCents": 10000 }
        })));
        assert!(info.monthly_enabled);
        assert_eq!(info.monthly_used_cents, Some(4567));
        assert_eq!(info.monthly_limit_cents, Some(10000));

        let info = parse_extra(Some(&json!({
            "isEnabled": true,
            "balance": { "amountLeft": "100000000" },
            "monthlyChargeLimitEnabled": false
        })));
        assert!(!info.monthly_enabled);
        assert_eq!(info.monthly_used_cents, None);
    }

    /// resetTime accepts RFC3339 and the looser DateTimeOffset.TryParse shapes.
    #[test]
    fn reset_time_formats() {
        assert!(parse_reset_time("2030-01-01T00:00:00+08:00").is_some());
        assert!(parse_reset_time("2030-01-01 00:00:00 +08:00").is_some());
        assert!(parse_reset_time("2030-01-01 00:00:00").is_some()); // no offset = local
        assert!(parse_reset_time("not a date").is_none());
    }

    /// REVIEW-RUST Major A entry path: priceInCents == i64::MIN (string form)
    /// must parse (SPEC §6.3).
    #[test]
    fn monthly_extreme_cents_string_parses() {
        let info = parse_extra(Some(&json!({
            "isEnabled": true,
            "balance": { "amountLeft": "0" },
            "monthlyChargeLimitEnabled": true,
            "monthlyUsed": { "priceInCents": "-9223372036854775808" },
            "monthlyChargeLimit": { "priceInCents": "10000" }
        })));
        assert_eq!(info.monthly_used_cents, Some(i64::MIN));
        assert_eq!(info.monthly_limit_cents, Some(10000));
    }

    /// SPEC §6.4 upgrade: prefer the limit matching window.duration == 300 &&
    /// timeUnit == TIME_UNIT_MINUTE (hits even when it is not limits[0]).
    #[test]
    fn five_hour_window_match_hits() {
        let root = json!({
            "limits": [
                {"window": {"duration": 60, "timeUnit": "TIME_UNIT_MINUTE"},
                 "detail": {"used": "1", "limit": "100"}},
                {"window": {"duration": "300", "timeUnit": "TIME_UNIT_MINUTE"},
                 "detail": {"used": "21", "limit": "100"}}
            ]
        });
        let r = parse_payload(&root, at_fracs());
        let seg = r.five_hour.expect("window-matched segment");
        assert_eq!(seg.percent, 21.0);
    }

    /// SPEC §6.4 fallback: fall back to limits[0] when no element matches; a malformed window shape falls back too.
    #[test]
    fn five_hour_window_match_falls_back() {
        // none matches -> limits[0]
        let root = json!({
            "limits": [
                {"window": {"duration": 60, "timeUnit": "TIME_UNIT_MINUTE"},
                 "detail": {"used": "68", "limit": "100"}},
                {"window": {"duration": 1440, "timeUnit": "TIME_UNIT_MINUTE"},
                 "detail": {"used": "9", "limit": "100"}}
            ]
        });
        let r = parse_payload(&root, at_fracs());
        assert_eq!(r.five_hour.unwrap().percent, 68.0);

        // window missing/not an object -> fall back to limits[0]
        let root = json!({
            "limits": [
                {"window": "weird", "detail": {"used": "68", "limit": "100"}}
            ]
        });
        let r = parse_payload(&root, at_fracs());
        assert_eq!(r.five_hour.unwrap().percent, 68.0);
    }

    /// SPEC §6.6 month segment, three cases: with a limit a segment is produced; missing / 0 limit produces none.
    #[test]
    fn month_segment_requires_nonzero_limit() {
        let with_limit = parse_payload(
            &json!({"totalQuota": {"used": "43", "limit": "100",
                                   "resetTime": "2030-10-31T00:00:00+08:00"}}),
            at_fracs(),
        );
        let seg = with_limit.month.expect("month segment with limit");
        assert_eq!(seg.percent, 43.0);
        assert!(seg.reset_at.is_some());

        let no_field = parse_payload(&json!({"totalQuota": {"used": "43"}}), at_fracs());
        assert!(no_field.month.is_none());

        let zero = parse_payload(
            &json!({"totalQuota": {"used": "43", "limit": 0}}),
            at_fracs(),
        );
        assert!(zero.month.is_none());

        // a non-object totalQuota likewise produces no segment
        let not_object = parse_payload(&json!({"totalQuota": "nope"}), at_fracs());
        assert!(not_object.month.is_none());
    }

    /// The month key serializes only when Some (SPEC §10.2): the output of a payload
    /// without totalQuota must not contain the "month" key (note extra's monthly*
    /// fields contain the "month" substring; match the full key name).
    #[test]
    fn month_key_absent_when_none() {
        let r = parse_payload(&json!({}), at_fracs());
        assert!(!r.to_pretty_json().contains("\"month\":"));
        let r = parse_payload(
            &json!({"totalQuota": {"used": "1", "limit": "2"}}),
            at_fracs(),
        );
        assert!(r.to_pretty_json().contains("\"month\":"));
    }

    /// SPEC §5.1 error classification pinned: non-2xx and other transport errors -> HttpRequestException.
    #[test]
    fn send_error_classification() {
        assert_eq!(
            classify_send_error(&ureq::Error::StatusCode(404)),
            "HttpRequestException"
        );
        assert_eq!(
            classify_send_error(&ureq::Error::Io(std::io::Error::other("boom"))),
            "HttpRequestException"
        );
    }

    /// SPEC §5.1 error classification pinned: timeout -> TaskCanceledException. A local
    /// listener that accepts but never responds, under a short global timeout, triggers ureq::Error::Timeout.
    #[test]
    fn timeout_classifies_as_task_canceled() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            // accept the connection and hold it without responding, until the test ends
            let mut held = Vec::new();
            for stream in listener.incoming().flatten() {
                held.push(stream);
            }
        });
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_millis(500)))
            .build()
            .new_agent();
        let err = agent
            .get(&format!("http://{addr}/usages"))
            .call()
            .expect_err("held connection must time out");
        assert_eq!(classify_send_error(&err), "TaskCanceledException");
    }
}
