// 额度取数 + 防御解析，移植自 repos/kimi-planbar-tui/rust/src/quota.rs（SPEC §2.2），
// 按 SPEC v1.1 完成三处变更：
//  - 5h 段由"只取 limits[0]"升级为 window 条件匹配（§6.4）
//  - 新增 month 段（root.totalQuota，§6.6）
//  - QuotaResult 扩展 month 字段，None 时跳过序列化（§5.3，保既有 golden 不变）
// 防御规则逐条对齐 SPEC §6 十条（数字按字符串建模、limit≤0 钳 1、NaN/inf 归零、
// i64::MIN 可解析、resetTime 宽松阶梯、boosterWallet 防御、1e-8 元单位换算）。

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
    /// 月度额度段（本工具新增，SPEC §6.6）。None 时整个键不序列化，
    /// 既有 30 个 golden（无 month 键）保持 byte-identical（SPEC §10.2）。
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
    /// (SPEC 16.5 step 2 / §5.3)。
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

    /// --test-fetch 输出与 golden 比对共用的序列化函数（SPEC §10.2）：
    /// serde pretty、2 空格缩进、camelCase。解析路径保证 percent 有限，
    /// 序列化不可能失败；fallback 仅为防御（panic=abort 下不可 unwinding）。
    pub fn to_pretty_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// base_url 覆盖链（SPEC §5.1）：env KIMI_CODE_BASE_URL > quota-bar.toml
/// [network] base_url > 默认。`configured` 即 config 解析出的 base_url。
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

/// 取数入口（base_url/超时由 config 提供，SPEC §5.1/§8）。
pub fn fetch(configured_base_url: Option<&str>, timeout_secs: u64) -> QuotaResult {
    fetch_from(&resolve_base_url(configured_base_url), timeout_secs)
}

/// 取数模式核心流程（SPEC §4.2 步骤 1–3；写缓存由调用方负责）。
/// 错误类型名沿用 .NET 风格（SPEC §6.10）：
/// 超时 -> TaskCanceledException；非 2xx 与其他传输错误 -> HttpRequestException；
/// 响应体非法 -> JsonException；无凭证 -> no-token。
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
        // body 阶段超时（headers 已回、body 挂起）同为 TaskCanceledException，
        // 其余（含非法 UTF-8）按响应体非法归 JsonException
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

/// 发送阶段错误分类（SPEC §5.1/§6.10，.NET 风格类型名）：超时 ->
/// TaskCanceledException；非 2xx（ureq 默认 http_status_as_error，含纯 API key
/// 账号的 404）与其余传输错误 -> HttpRequestException。
fn classify_send_error(e: &ureq::Error) -> &'static str {
    match e {
        ureq::Error::Timeout(_) => "TaskCanceledException",
        _ => "HttpRequestException",
    }
}

/// 防御解析（SPEC §6）。`now` 经参数注入：生产路径传 Local::now()，
/// golden 测试传固定时钟（P4 parity 前置，避免返工）。
pub fn parse_payload(root: &Value, now: DateTime<Local>) -> QuotaResult {
    let mut r = QuotaResult {
        five_hour: None,
        week: None,
        month: None,
        extra: None,
        fetched_at: now,
        error: None,
    };
    // 5h 段：limits[] 中 window 条件匹配（SPEC §6.4），detail 非对象时
    // parse_segment 落回全零值（对齐参考实现，golden quota-detail_string）
    if let Some(detail) = root
        .get("limits")
        .and_then(|l| l.as_array())
        .and_then(|a| pick_five_hour(a))
        .and_then(|first| first.get("detail"))
    {
        r.five_hour = Some(parse_segment(detail));
    }
    // 周段：顶层 root.usage（对象才解析，SPEC §6.5）
    if let Some(u) = root.get("usage")
        && u.is_object()
    {
        r.week = Some(parse_segment(u));
    }
    // 月段：root.totalQuota，limit 缺失/为 0/非有限值（NaN 等）不产生段
    //（SPEC §6.6/§6.3；NaN != 0.0 恒真，须显式排除）
    if let Some(t) = root.get("totalQuota") {
        let limit = get_f64(t, "limit");
        if t.is_object() && limit.is_finite() && limit != 0.0 {
            r.month = Some(parse_segment(t));
        }
    }
    r.extra = Some(parse_extra(root.get("boosterWallet")));
    r
}

/// 5h 段选窗（SPEC §6.4）：优先 window.duration == 300 且 window.timeUnit ==
/// "TIME_UNIT_MINUTE" 的元素（duration 兼容字符串建模），找不到回落 limits[0]。
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
    // 仅严格布尔 false 触发；字符串 "false"、数字 0 等非布尔值按启用路径继续。
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

    /// golden 固定时钟（quota_test.go:14-15）：
    /// atZero = 1893456000000ms 整；atFracs = +123456789ns。
    /// 本机时区 +08:00 下分别序列化为 "2030-01-01T08:00:00+08:00" 与
    /// "2030-01-01T08:00:00.123456789+08:00"。
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

    /// SPEC 6.8：isEnabled 防御仅严格布尔 false 触发；字符串 "false" 与数字 0
    /// 不触发，按启用路径继续解析（golden isenabled_string_false / isenabled_zero）。
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

    /// SPEC §6.4 升级：优先匹配 window.duration == 300 && timeUnit ==
    /// TIME_UNIT_MINUTE 的 limit（不是 limits[0] 也能命中）。
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

    /// SPEC §6.4 回落：无命中元素时回落 limits[0]；window 字段形状异常同样回落。
    #[test]
    fn five_hour_window_match_falls_back() {
        // 无一命中 -> limits[0]
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

        // window 缺失/非对象 -> 回落 limits[0]
        let root = json!({
            "limits": [
                {"window": "weird", "detail": {"used": "68", "limit": "100"}}
            ]
        });
        let r = parse_payload(&root, at_fracs());
        assert_eq!(r.five_hour.unwrap().percent, 68.0);
    }

    /// SPEC §6.6 月段三例：有 limit 产生段；limit 缺失 / 为 0 不产生段。
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

        // totalQuota 非对象同样不产生段
        let not_object = parse_payload(&json!({"totalQuota": "nope"}), at_fracs());
        assert!(not_object.month.is_none());
    }

    /// month 键仅在 Some 时序列化（SPEC §10.2）：无 totalQuota 的 payload
    /// 输出不得含 "month" 键（注意 extra 的 monthly* 字段含 "month" 子串，
    /// 须匹配完整键名）。
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

    /// SPEC §5.1 错误分类钉死：非 2xx 与其他传输错误 -> HttpRequestException。
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

    /// SPEC §5.1 错误分类钉死：超时 -> TaskCanceledException。本地监听但不
    /// 回应，短全局超时触发 ureq::Error::Timeout。
    #[test]
    fn timeout_classifies_as_task_canceled() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            // 接受连接并握住不放（不回应），保持到测试结束
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
