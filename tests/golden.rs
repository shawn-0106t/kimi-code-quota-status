// golden parity 测试（SPEC §10.2 / PLAN P4）。
//
// 机制复刻 repos/kimi-planbar-tui/go/internal/core/quota_test.go:206-293：
// 输入 payload 内联（拷贝自 quota_test.go:240-266）-> 注入解析函数 ->
// 以固定时钟序列化 -> 与 golden 文件逐字节比对（比较前 CRLF 归一化，同
// goldens_test.go:23-25）。
//
// 全部 33 case 已内化到本仓库 testdata/golden/（2026-10-01：30 个既有 case
// 从参考仓库逐字节复制，CI 自足；month 3 个新 case 原生）。参考仓库仍为
// 上游事实来源；新增 case 只进本仓库，绝不回写参考仓库（SPEC §10.2）。
// 序列化与 --test-fetch 输出共用 QuotaResult::to_pretty_json。

use chrono::{DateTime, Local, TimeZone, Timelike};
use quota_status::quota::{QuotaResult, parse_payload};
use serde_json::Value;

const GOLDEN_DIR: &str = "testdata/golden";

/// golden 固定时钟（quota_test.go:14-15）：atZero = 1893456000000ms 整、
/// atFracs = +123456789ns。本机时区 +08:00 下序列化为
/// "2030-01-01T08:00:00+08:00" / "2030-01-01T08:00:00.123456789+08:00"。
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

/// golden 断言按本机时区 +08:00 生成（fetchedAt/resetAt 偏移固定 +08:00，
/// 与参考仓库 Go 测试的 time.Local 语义一致）。非 +08:00 机器上 fail-fast
/// 并给出明确原因，避免以不直观的 byte mismatch 报错。
fn require_cn_timezone() {
    let off = Local::now().offset().local_minus_utc();
    assert_eq!(
        off,
        8 * 3600,
        "golden parity 需在 UTC+08:00 时区机器上运行（golden 文件的 datetime 偏移为 +08:00）；当前 UTC 偏移 {}s",
        off
    );
}

/// 读 golden 并做 CRLF 归一化（core.autocrlf 检出不破坏比对）。
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

/// 输入 payload（逐字拷贝自 quota_test.go:240-266）。
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

/// 既有 25 个解析 case：逐字节对齐内化 golden（month 键以
/// skip_serializing_if 保证不出现，保持 byte-identical）。
#[test]
fn goldens_byte_for_byte() {
    require_cn_timezone();
    for (name, payload) in payloads() {
        let r = parse_payload(&must_parse(payload), at_fracs());
        assert_matches(name, &r.to_pretty_json(), GOLDEN_DIR);
    }
}

/// quota-error-* 4 个 case：不走网络，直接构造 error 结果比对
/// （quota_test.go:272-275 同款）。
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

/// quota-fill-missing：失败结果从 last-known-good 回填三段
///（quota_test.go:276-280 同款；month 同样回填但本 payload 无 totalQuota，
/// 序列化无 month 键）。
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

/// month 新 case（本项目 testdata/golden/，SPEC §6.6 / §10.2）：
/// 有 limit 产生 month 键；limit 缺失 / 为 0 不产生 month 键。
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

/// 抽查逐字节性（PLAN P4 验收 2 的机制内建版）：quota-success_full 的
/// 期望文本与实际输出在未归一化前逐字节相同性依赖 golden 文件本身；
/// 此处钉死归一化真实生效——CRLF 版本文本归一化后须与输出一致。
#[test]
fn crlf_normalization_is_load_bearing() {
    let raw = golden_text(GOLDEN_DIR, "quota-empty");
    let crlf = raw.replace('\n', "\r\n");
    let r = parse_payload(&must_parse("{}"), at_fracs());
    let got = r.to_pretty_json();
    assert_eq!(crlf.replace("\r\n", "\n"), got, "CRLF 归一化后须逐字节一致");
    assert_ne!(crlf, got, "未归一化的 CRLF 文本须与输出不同（归一化生效）");
}
