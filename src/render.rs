// 渲染模块（SPEC §7）：stdin 快照 + 本地缓存 -> 拼一行带 ANSI 颜色的文本。
// 顺序：permissionMode -> model -> thinking -> 额度组(5h/week/month) ->
// gitBranch；每段"有值才显示"，段间灰色 |，额度组内灰色 ·。
// 宽度感知降级（§7.5）：丢 reset 后缀 -> 丢 gitBranch -> 只留额度组 -> 原样输出。

use crate::config::{Config, Field};
use crate::quota::{ExtraState, QuotaResult};
use chrono::{DateTime, Local};
use serde_json::Value;

const RESET: &str = "\x1b[0m";
const SEP_SEG: &str = " \x1b[90m|\x1b[0m "; // 段间分隔符（灰 |）
const SEP_PART: &str = " \x1b[90m·\x1b[0m "; // 额度组内分隔符（灰 ·）
const GRAY: &str = "90";
const CYAN: &str = "36";
const GREEN: &str = "32";
const YELLOW: &str = "33";
const RED: &str = "31";
const MAGENTA: &str = "35";
const WHITE: &str = "37";

/// thinking 段取值结果（config.toml 阶梯输出，SPEC §7.1）
#[derive(Debug, PartialEq)]
pub enum Thinking {
    Off,
    Effort(String),
}

/// stdin 字节 -> payload Value：先 lossy UTF-8（U+FFFD，SPEC §7.6）再解析；
/// 解析失败按空 payload（Value::Null，语义同 quota-status.py:177-181）。
pub fn payload_from_bytes(bytes: &[u8]) -> Value {
    let text = String::from_utf8_lossy(bytes);
    serde_json::from_str(&text).unwrap_or(Value::Null)
}

/// thinking 阶梯（SPEC §7.1，语义同 quota-status.py:101-119）：
/// 1. [thinking] enabled == false（严格布尔）-> 灰 off；
/// 2. 否则 [thinking] effort（非空字符串）；
/// 3. 缺失 -> [models.*] 中 display_name 或 model 匹配 stdin model 的条目，
///    取 overrides.default_effort，再退 default_effort；
/// 4. 都取不到 -> None（该段省略）。
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

/// 额度段颜色（SPEC §7.2）：percent < green_below 绿 / < yellow_below 黄 / 否则红。
/// 边界 `>= 85` 红、`>= 60` 黄（阈值默认 60/85）。
pub fn quota_color(percent: f64, green_below: f64, yellow_below: f64) -> &'static str {
    if percent < green_below {
        GREEN
    } else if percent < yellow_below {
        YELLOW
    } else {
        RED
    }
}

/// reset 后缀（SPEC §7.3）：同日 ` (rst HH:MM)`；跨天 ` (rst MM/DD HH:MM)`。
/// 本地时区；resetAt 解析失败/缺失 -> None（无后缀）。
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

/// 单段渲染：`\033[{color}m{text}\033[0m`
fn span(color: &str, text: &str) -> String {
    format!("\x1b[{color}m{text}{RESET}")
}

/// percent 按整行格式化（%.0f = round-half-to-even，SPEC §7.3）
fn fmt_percent(p: f64) -> String {
    format!("{p:.0}")
}

struct QuotaPart {
    label: &'static str,
    percent: f64,
    reset_at: Option<DateTime<Local>>,
}

/// 额度组缓存侧取数（SPEC §7.1）：对应段存在且开启才进组；
/// 缓存含 error（理论不发生，防御）视为无数据。
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

/// 行拼接（SPEC §7.4）。`opts` 控制降级变体：
/// reset=false 丢所有 reset 后缀；git=false 丢 gitBranch 段；
/// quota_only=true 只留额度组。只删不重排。
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
    now: DateTime<Local>,
    opts: &VariantOpts,
) -> String {
    let mut segs: Vec<String> = Vec::new();
    // 按配置 order 迭代（重排生效）；quota_only 只处理额度组；
    // 降级丢 gitBranch 只删不重排
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
            // thinking（off 灰 / effort cyan，SPEC §7.2）
            Field::Thinking => match thinking {
                Some(Thinking::Off) => segs.push(span(GRAY, "off")),
                Some(Thinking::Effort(eff)) => segs.push(span(CYAN, eff)),
                None => {}
            },
            // 额度组（组内灰 · 连接，SPEC §7.3）
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
                // booster 默认不渲染（SPEC §1.2/§7.1）；开启且 Ready 时以 cyan
                // 显示余额（元）。格式取纯 ASCII（避免 ¥ 等符号在非 UTF-8
                // 终端的兼容性问题）
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
                    segs.push(parts.join(SEP_PART));
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
    segs.join(SEP_SEG)
}

/// 可见宽度：剔除 ANSI 转义序列后按字符数近似（SPEC §7.4；
/// 字段以 ASCII 为主，最终由宿主 truncateToWidth 兜底）。
pub fn visible_width(line: &str) -> usize {
    let mut width = 0usize;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            // 消费到 'm'（我们只输出 SGR 序列）
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

/// 宽度感知降级阶梯（SPEC §7.5）：0 全量 -> 1 丢 reset -> 2 丢 gitBranch ->
/// 3 只留额度组 -> 4 仍超宽原样输出（交宿主截断）。
pub fn render_line(
    payload: &Value,
    cached: Option<&QuotaResult>,
    cfg: &Config,
    thinking: Option<&Thinking>,
    width: u32,
    now: DateTime<Local>,
) -> String {
    // 全量（reset 开关关闭则天然无后缀）；降级只删不重排
    let full = render_variant(
        payload,
        cached,
        cfg,
        thinking,
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
        now,
        &VariantOpts {
            reset: false,
            git: false,
            quota_only: true,
        },
    );

    // 逐级尝试直到可容纳；全超 -> 原样输出（full）
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

    /// 颜色阈值边界（PLAN P3 单测清单）：59.x 绿 / 60 黄 / 84.x 黄 / 85 红。
    #[test]
    fn color_threshold_boundaries() {
        assert_eq!(quota_color(59.9, 60.0, 85.0), "32");
        assert_eq!(quota_color(60.0, 60.0, 85.0), "33");
        assert_eq!(quota_color(84.9, 60.0, 85.0), "33");
        assert_eq!(quota_color(85.0, 60.0, 85.0), "31");
    }

    /// percent 舍入边界（%.0f = round-half-to-even）：60.5->60、61.5->62。
    #[test]
    fn percent_rounds_half_to_even() {
        assert_eq!(fmt_percent(60.5), "60");
        assert_eq!(fmt_percent(61.5), "62");
        assert_eq!(fmt_percent(0.5), "0");
        assert_eq!(fmt_percent(1.5), "2");
        assert_eq!(fmt_percent(21.0), "21");
    }

    /// reset 跨天格式（PLAN P3 单测清单）：同日 HH:MM，跨天 MM/DD HH:MM。
    #[test]
    fn reset_suffix_same_and_cross_day() {
        let n = now(); // 2030-01-01 08:00 本地
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

    /// thinking 阶梯（PLAN P3 单测清单）：enabled=false / effort / models
    /// 回退两级 / 全缺省略。
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

        // models 回退：overrides.default_effort 优先
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

        // models 回退：再退 default_effort
        let text = r#"
[models.other]
model = "kimi-model"
default_effort = "low"
"#;
        assert_eq!(
            thinking_from_config(Some(text), Some("kimi-model")),
            Some(Thinking::Effort("low".into()))
        );

        // 全缺 -> 省略
        assert_eq!(
            thinking_from_config(Some("[thinking]\n"), Some("Kimi")),
            None
        );
        assert_eq!(thinking_from_config(None, Some("Kimi")), None);
        // config.toml 整体非法 -> 省略
        assert_eq!(thinking_from_config(Some("not toml"), Some("Kimi")), None);
    }

    /// 行拼接（SPEC §7.4）：段序、颜色码、灰分隔符、每段重置。
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
        );
        assert!(line.starts_with("\x1b[31myolo\x1b[0m"), "yolo 红: {line:?}");
        assert!(line.contains("\x1b[36mKimi\x1b[0m"), "model cyan");
        assert!(line.contains("\x1b[36mhigh\x1b[0m"), "thinking cyan");
        assert!(line.contains("\x1b[32m5h 21%"), "5h 绿 + 无空格拼接");
        assert!(line.contains("(rst 00:00)"), "同日 reset");
        assert!(line.contains("\x1b[33mweek 68%"), "68 黄");
        assert!(
            line.contains("\x1b[32mmonth 43% (rst 11/05 00:00)"),
            "跨天 reset: {line:?}"
        );
        assert!(line.ends_with("\x1b[35mmain\x1b[0m"), "git magenta");
        assert_eq!(
            line.matches(" \x1b[90m|\x1b[0m ").count(),
            4,
            "段间 4 个灰 |"
        );
        assert_eq!(
            line.matches(" \x1b[90m·\x1b[0m ").count(),
            2,
            "组内 2 个灰 ·"
        );
        // 行内不含 contextTokens / maxContextTokens（SPEC §3.4）
        assert!(!line.contains("context"));
    }

    /// 宽度降级四级（PLAN P3 单测清单）：丢 reset -> 丢 gitBranch ->
    /// 只留额度组 -> 原样输出。
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
        );
        assert!(full.contains("(rst"), "全量含 reset");
        assert!(full.contains("\x1b[35mmain"), "全量含 git");

        // 第 1 级：宽度 < 全量 -> 丢 reset
        let w1 = visible_width(&full) as u32 - 1;
        let l1 = render_line(
            &payload_full(),
            Some(&c),
            &cfg,
            Some(&Thinking::Effort("high".into())),
            w1,
            now(),
        );
        assert!(!l1.contains("(rst"), "第 1 级丢 reset");
        assert!(l1.contains("\x1b[35mmain"), "第 1 级仍含 git");

        // 第 2 级：再缩 -> 丢 gitBranch
        let w2 = visible_width(&l1) as u32 - 1;
        let l2 = render_line(
            &payload_full(),
            Some(&c),
            &cfg,
            Some(&Thinking::Effort("high".into())),
            w2,
            now(),
        );
        assert!(!l2.contains("\x1b[35m"), "第 2 级丢 git");
        assert!(l2.contains("5h"), "第 2 级仍含额度组");
        assert!(l2.contains("\x1b[36mKimi"), "第 2 级仍含 model");

        // 第 3 级：只留额度组
        let w3 = visible_width(&l2) as u32 - 1;
        let l3 = render_line(
            &payload_full(),
            Some(&c),
            &cfg,
            Some(&Thinking::Effort("high".into())),
            w3,
            now(),
        );
        assert!(l3.contains("5h"), "第 3 级含额度组");
        assert!(!l3.contains("\x1b[36mKimi"), "第 3 级丢 model");
        assert!(l3.starts_with("\x1b[32m5h"), "第 3 级以额度组开头");

        // 第 4 级：宽度极小 -> 原样输出（等于 full）
        let l4 = render_line(
            &payload_full(),
            Some(&c),
            &cfg,
            Some(&Thinking::Effort("high".into())),
            1,
            now(),
        );
        assert_eq!(l4, full, "第 4 级原样输出交宿主截断");
    }

    /// reset_time=false 等价降级第 1 级（SPEC §8）。
    #[test]
    fn reset_time_config_off() {
        let cfg = parse_minimal("[render.quota]\nreset_time = false\n");
        let c = cached_full();
        let line = render_line(&payload_full(), Some(&c), &cfg, None, 400, now());
        assert!(!line.contains("(rst"), "reset_time=false 永久丢 reset");
        assert!(line.contains("\x1b[35mmain"), "其余字段正常");
    }

    fn parse_minimal(text: &str) -> Config {
        crate::config::parse(text)
    }

    /// 空/非法 payload：无字段可渲染时输出空行（宿主回退，SPEC §4.1 步骤 5）。
    #[test]
    fn empty_payload_renders_empty_line() {
        let cfg = Config::default();
        let empty = render_line(&serde_json::json!({}), None, &cfg, None, 120, now());
        assert_eq!(empty, "");
        // quota 组缓存缺失同样省略
        let null_payload = render_line(&Value::Null, None, &cfg, None, 120, now());
        assert_eq!(null_payload, "");
    }

    /// UTF-8 lossy（SPEC §7.6）：非法字节 -> U+FFFD，不崩溃；非法 JSON -> 空 payload。
    #[test]
    fn payload_lossy_and_invalid() {
        let v = payload_from_bytes(b"\xff\xfe not json");
        assert_eq!(v, Value::Null);

        // 合法 JSON 载体中的非法 UTF-8 字节：lossy 替换后正常解析
        let mut bytes = b"{\"model\":\"".to_vec();
        bytes.extend_from_slice(&[0xff]);
        bytes.extend_from_slice(b"\"}");
        let v = payload_from_bytes(&bytes);
        assert_eq!(v.get("model").and_then(|m| m.as_str()), Some("\u{FFFD}"));
    }

    /// order 重排生效；额度组缓存 error 时整体省略（防御）。
    #[test]
    fn order_and_error_cache() {
        let cfg = parse_minimal("[render]\norder = [\"quota\", \"permission_mode\"]\n");
        let c = cached_full();
        let line = render_line(&payload_full(), Some(&c), &cfg, None, 400, now());
        assert!(line.starts_with("\x1b[32m5h"), "额度组在首位: {line:?}");

        let mut err = cached_full();
        err.error = Some("HttpRequestException".into());
        let line = render_line(
            &payload_full(),
            Some(&err),
            &Config::default(),
            None,
            400,
            now(),
        );
        assert!(!line.contains("5h"), "error 缓存不渲染额度组");
        assert!(line.contains("\x1b[31myolo"), "其余字段照常");
    }

    /// visible_width 剔除 ANSI 后按字符数计。
    #[test]
    fn visible_width_strips_ansi() {
        assert_eq!(visible_width("\x1b[31myolo\x1b[0m"), 4);
        assert_eq!(visible_width("a\x1b[90m|\x1b[0mb"), 3);
        assert_eq!(visible_width(""), 0);
    }

    /// booster 渲染：默认关闭不显示；开启且 Ready 时以 ASCII 显示余额（元）。
    #[test]
    fn booster_render_switch_and_ascii_format() {
        let c = cached_full(); // balanceCents 1235 -> 12.35 元
        let off = render_line(
            &payload_full(),
            Some(&c),
            &Config::default(),
            None,
            400,
            now(),
        );
        assert!(!off.contains("boost"), "booster 默认不渲染");

        let cfg = parse_minimal("[render.quota]\nbooster = true\n");
        let on = render_line(&payload_full(), Some(&c), &cfg, None, 400, now());
        assert!(
            on.contains("\x1b[36mboost 12.35\x1b[0m"),
            "booster ASCII 余额: {on:?}"
        );

        // 非 Ready（NoData/NotActivated）不显示
        let mut nodata = cached_full();
        nodata.extra.as_mut().unwrap().state = ExtraState::NoData;
        let line = render_line(&payload_full(), Some(&nodata), &cfg, None, 400, now());
        assert!(!line.contains("boost"), "NoData 不显示余额");
    }
}
