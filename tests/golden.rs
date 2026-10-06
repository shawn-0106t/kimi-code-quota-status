// Golden parity tests (SPEC §10.2 / PLAN P4).
//
// Mechanism mirrors repos/kimi-planbar-tui/go/internal/core/quota_test.go:206-293:
// input payload inlined (copied from quota_test.go:240-266) -> inject into the parse function ->
// serialize with a fixed clock -> byte-for-byte comparison against the golden files (CRLF normalized
// before comparison, same as goldens_test.go:23-25).
//
// All 33 cases are internalized into this repo's testdata/golden/ (2026-10-01: 30 pre-existing cases
// copied byte-for-byte from the reference repo, CI self-contained; 3 native new month cases). The reference repo remains
// the upstream source of truth; new cases only land in this repo, never written back to the reference repo (SPEC §10.2).
// Serialization is shared with the --test-fetch output via QuotaResult::to_pretty_json.

use chrono::{DateTime, Local, TimeZone, Timelike};
use quota_status::quota::{QuotaResult, parse_payload};
use serde_json::Value;

const GOLDEN_DIR: &str = "testdata/golden";

/// Fixed golden clock (quota_test.go:14-15): atZero = whole 1893456000000ms,
/// atFracs = +123456789ns. Under the local +08:00 timezone these serialize as
/// "2030-01-01T08:00:00+08:00" / "2030-01-01T08:00:00.123456789+08:00".
fn at_zero() -> DateTime<Local> {
    Local.timestamp_millis_opt(1_893_456_000_000).unwrap()
}

fn at_fracs() -> DateTime<Local> {
    Local
        .timestamp_millis_opt(1_893_456_000_000)
        .unwrap()
        .with_nanosecond(123_456_789)
        .unwrap()
}

/// Golden assertions are generated under the local +08:00 timezone (fetchedAt/resetAt offsets
/// fixed at +08:00, matching the time.Local semantics of the reference repo's Go tests). Fail-fast
/// on non-+08:00 machines with a clear reason, avoiding an unintuitive byte mismatch error.
fn require_cn_timezone() {
    let off = Local::now().offset().local_minus_utc();
    assert_eq!(
        off,
        8 * 3600,
        "golden parity must run on a machine in the UTC+08:00 timezone (golden file datetime offsets are +08:00); current UTC offset {}s",
        off
    );
}

/// Read a golden file and apply CRLF normalization (core.autocrlf checkout does not break comparison).
fn golden_text(dir: &str, name: &str) -> String {
    let path = format!("{}/{}/{}.txt", env!("CARGO_MANIFEST_DIR"), dir, name);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("missing golden {path}: {e}"));
    String::from_utf8_lossy(&bytes).replace("\r\n", "\n")
}

fn must_parse(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("payload parse: {e}: {text}"))
}

fn assert_matches(name: &str, got: &str, dir: &str) {
    let want = golden_text(dir, name);
    if got != want {
        panic!("golden {name} mismatch:\n--- got ---\n{got}\n--- want ---\n{want}");
    }
}

/// Input payloads (copied verbatim from quota_test.go:240-266).
fn payloads() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "quota-success_full",
            r#"{"limits":[{"detail":{"used":"21","limit":"100","resetTime":"2030-01-01T00:00:00+08:00"}}],"usage":{"used":18.5,"limit":100,"resetTime":"2030-01-08 00:00:00 +08:00"},"boosterWallet":{"isEnabled":true,"balance":{"amountLeft":"1234567890"},"monthlyChargeLimitEnabled":true,"monthlyUsed":{"priceInCents":"4567"},"monthlyChargeLimit":{"priceInCents":10000}}}"#,
        ),
        (
            "quota-mixed_string_number",
            r#"{"limits":[{"detail":{"used":"68","limit":"100","resetTime":"2030-01-01T00:00:00+08:00"}}],"usage":{"used":68,"limit":100}}"#,
        ),
        (
            "quota-div_zero",
            r#"{"limits":[{"detail":{"used":"50","limit":"0"}}]}"#,
        ),
        (
            "quota-neg_limit",
            r#"{"limits":[{"detail":{"used":"50","limit":-100}}]}"#,
        ),
        (
            "quota-hostile_inf",
            r#"{"usage":{"used":"1e999","limit":1}}"#,
        ),
        ("quota-hostile_nan", r#"{"usage":{"used":"NaN","limit":1}}"#),
        ("quota-nan_limit", r#"{"usage":{"used":"5","limit":"NaN"}}"#),
        ("quota-empty", r#"{}"#),
        (
            "quota-limits_not_array",
            r#"{"limits":{"detail":{"used":"1","limit":"2"}}}"#,
        ),
        ("quota-detail_string", r#"{"limits":[{"detail":"nope"}]}"#),
        ("quota-usage_null", r#"{"usage":null}"#),
        ("quota-usage_string", r#"{"usage":"nope"}"#),
        (
            "quota-disabled_wallet",
            r#"{"boosterWallet":{"isEnabled":false,"balance":{"amountLeft":"123456789"}}}"#,
        ),
        (
            "quota-isenabled_string_false",
            r#"{"boosterWallet":{"isEnabled":"false","balance":{"amountLeft":"123456789"}}}"#,
        ),
        (
            "quota-isenabled_zero",
            r#"{"boosterWallet":{"isEnabled":0,"balance":{"amountLeft":"1500000"}}}"#,
        ),
        ("quota-no_wallet", r#"{"boosterWallet":null}"#),
        ("quota-wallet_string", r#"{"boosterWallet":"nope"}"#),
        (
            "quota-amount_frac_number",
            r#"{"boosterWallet":{"isEnabled":true,"balance":{"amountLeft":1500000.7}}}"#,
        ),
        (
            "quota-amount_negative_round",
            r#"{"boosterWallet":{"isEnabled":true,"balance":{"amountLeft":"-1499999"}}}"#,
        ),
        (
            "quota-amount_negative_round2",
            r#"{"boosterWallet":{"isEnabled":true,"balance":{"amountLeft":"-1500001"}}}"#,
        ),
        (
            "quota-amount_huge",
            r#"{"boosterWallet":{"isEnabled":true,"balance":{"amountLeft":"9223372036854775807"}}}"#,
        ),
        (
            "quota-nodata_monthly_on",
            r#"{"boosterWallet":{"isEnabled":true,"balance":{"amountLeft":"not-a-number"},"monthlyChargeLimitEnabled":true,"monthlyUsed":{"priceInCents":"4567"},"monthlyChargeLimit":{"priceInCents":10000}}}"#,
        ),
        (
            "quota-monthly_off_but_present",
            r#"{"boosterWallet":{"isEnabled":true,"balance":{"amountLeft":"100000000"},"monthlyUsed":{"priceInCents":"4567"}}}"#,
        ),
        (
            "quota-monthly_string_true",
            r#"{"boosterWallet":{"isEnabled":true,"balance":{"amountLeft":"100000000"},"monthlyChargeLimitEnabled":"true","monthlyUsed":{"priceInCents":"4567"}}}"#,
        ),
        (
            "quota-reset_fraction",
            r#"{"limits":[{"detail":{"used":"1","limit":"3","resetTime":"2030-01-01T00:00:00.123456789+08:00"}}],"usage":{"used":"1","limit":"3","resetTime":"2030-01-02T00:00:00.500Z"}}"#,
        ),
    ]
}

/// Pre-existing 25 parse cases: internalized goldens aligned byte-for-byte (the
/// month key is guaranteed absent via skip_serializing_if, staying byte-identical).
#[test]
fn goldens_byte_for_byte() {
    require_cn_timezone();
    for (name, payload) in payloads() {
        let r = parse_payload(&must_parse(payload), at_fracs());
        assert_matches(name, &r.to_pretty_json(), GOLDEN_DIR);
    }
}

/// The 4 quota-error-* cases: no network involved; construct error results directly
/// and compare (same approach as quota_test.go:272-275).
#[test]
fn error_goldens_byte_for_byte() {
    require_cn_timezone();
    for kind in [
        "no-token",
        "HttpRequestException",
        "TaskCanceledException",
        "JsonException",
    ] {
        let name = format!("quota-error-{kind}");
        let r = QuotaResult {
            five_hour: None,
            week: None,
            month: None,
            extra: None,
            fetched_at: at_zero(),
            error: Some(kind.to_string()),
        };
        assert_matches(&name, &r.to_pretty_json(), GOLDEN_DIR);
    }
}

/// quota-fill-missing: the failure result backfills the three segments from
/// last-known-good (same approach as quota_test.go:276-280; month is backfilled too, but
/// this payload has no totalQuota, so serialization has no month key).
#[test]
fn fill_missing_golden_byte_for_byte() {
    require_cn_timezone();
    let last = parse_payload(
        &must_parse(
            payloads()
                .into_iter()
                .find(|(n, _)| *n == "quota-success_full")
                .unwrap()
                .1,
        ),
        at_fracs(),
    );
    let mut fresh = QuotaResult {
        five_hour: None,
        week: None,
        month: None,
        extra: None,
        fetched_at: at_zero(),
        error: Some("HttpRequestException".to_string()),
    };
    fresh.fill_missing_from(&last);
    assert_matches("quota-fill-missing", &fresh.to_pretty_json(), GOLDEN_DIR);
}

/// New month cases (this repo's testdata/golden/, SPEC §6.6 / §10.2):
/// a limit yields the month key; limit missing / zero yields no month key.
#[test]
fn month_goldens_byte_for_byte() {
    require_cn_timezone();
    let cases: Vec<(&str, &str)> = vec![
        (
            "quota-month-full",
            r#"{"limits":[{"window":{"duration":300,"timeUnit":"TIME_UNIT_MINUTE"},"detail":{"used":"21","limit":"100","resetTime":"2030-01-01T00:00:00+08:00"}}],"usage":{"used":18.5,"limit":100,"resetTime":"2030-01-08 00:00:00 +08:00"},"totalQuota":{"used":"43","limit":"100","resetTime":"2030-10-31T00:00:00+08:00"},"boosterWallet":{"isEnabled":true,"balance":{"amountLeft":"1234567890"},"monthlyChargeLimitEnabled":true,"monthlyUsed":{"priceInCents":"4567"},"monthlyChargeLimit":{"priceInCents":10000}}}"#,
        ),
        (
            "quota-month-no-limit",
            r#"{"limits":[{"detail":{"used":"10","limit":"100"}}],"totalQuota":{"used":"43"}}"#,
        ),
        (
            "quota-month-zero-limit",
            r#"{"limits":[{"detail":{"used":"10","limit":"100"}}],"totalQuota":{"used":"43","limit":0}}"#,
        ),
    ];
    for (name, payload) in cases {
        let r = parse_payload(&must_parse(payload), at_fracs());
        assert_matches(name, &r.to_pretty_json(), GOLDEN_DIR);
    }
}

/// Spot-check the byte-for-byte property (built-in version of PLAN P4 acceptance item 2): whether
/// quota-success_full's expected text and actual output are byte-identical before normalization depends
/// on the golden file itself; this test pins down that normalization really works - the CRLF text must match the output.
#[test]
fn crlf_normalization_is_load_bearing() {
    let raw = golden_text(GOLDEN_DIR, "quota-empty");
    let crlf = raw.replace('\n', "\r\n");
    let r = parse_payload(&must_parse("{}"), at_fracs());
    let got = r.to_pretty_json();
    assert_eq!(
        crlf.replace("\r\n", "\n"),
        got,
        "must match byte-for-byte after CRLF normalization"
    );
    assert_ne!(
        crlf, got,
        "unnormalized CRLF text must differ from the output (normalization is load-bearing)"
    );
}
