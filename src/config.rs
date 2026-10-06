// Field configuration module (SPEC §8): parses <kimi_home>/quota-bar.toml, controlling field
// toggles/order, color thresholds, cache TTL, and network overrides. Missing/invalid/unknown
// fields always fall back to built-in defaults (rendering never fails due to config); paths follow KIMI_CODE_HOME.

use std::path::Path;

/// Inline fields (valid names in order; unknown names ignored)
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Field {
    PermissionMode,
    Model,
    Thinking,
    /// tasks/agents badges (SPEC §7.7 v1.5): the switch is order itself -- removing "tasks"
    /// disables the whole segment and also skips the sessions directory scan (saves IO)
    Tasks,
    Quota,
    GitBranch,
}

fn parse_field(s: &str) -> Option<Field> {
    match s {
        "permission_mode" => Some(Field::PermissionMode),
        "model" => Some(Field::Model),
        "thinking" => Some(Field::Thinking),
        "tasks" => Some(Field::Tasks),
        "quota" => Some(Field::Quota),
        "git_branch" => Some(Field::GitBranch),
        _ => None,
    }
}

#[derive(Clone, Debug)]
pub struct QuotaFields {
    pub five_hour: bool,
    pub week: bool,
    pub month: bool,
    pub reset_time: bool, // false is equivalent to degradation level 1 (reset suffix lost permanently)
    pub booster: bool,    // booster wallet hidden by default (data still parsed into the cache)
}

#[derive(Clone, Debug)]
pub struct Config {
    pub order: Vec<Field>,
    /// Monochrome switch (SPEC §7.2 v1.4): when false, rendering outputs plain text (no SGR),
    /// the whole line is wrapped by the host in the theme's text color (same color as the line-2 context, follows /theme)
    pub colors: bool,
    pub quota: QuotaFields,
    /// percent < green_below means green; < yellow_below means yellow; otherwise red
    pub green_below: f64,
    pub yellow_below: f64,
    pub ttl_seconds: u64,
    pub retry_seconds: u64,
    /// [network] base_url; None = use the built-in default endpoint
    pub base_url: Option<String>,
    pub http_timeout_seconds: u64,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            // v1.5: the tasks badge sits before the quota group (SPEC §7.1/§8)
            order: vec![
                Field::PermissionMode,
                Field::Model,
                Field::Thinking,
                Field::Tasks,
                Field::Quota,
                Field::GitBranch,
            ],
            colors: true,
            quota: QuotaFields {
                five_hour: true,
                week: true,
                month: true,
                reset_time: true,
                booster: false,
            },
            green_below: 60.0,
            yellow_below: 85.0,
            ttl_seconds: 60,
            retry_seconds: 30,
            base_url: None,
            http_timeout_seconds: 8,
        }
    }
}

/// Parses from quota-bar.toml text; invalid TOML as a whole -> all defaults.
/// An invalid single key only affects that key (falls back to default); unknown fields ignored.
pub fn parse(text: &str) -> Config {
    let mut cfg = Config::default();
    let Ok(val) = toml::from_str::<toml::Value>(text) else {
        return cfg;
    };

    // [render] order: adopted if an array (may be empty = display nothing), non-array falls to default
    let order = val.get("render").and_then(|r| r.get("order"));
    if let Some(list) = order.and_then(|o| o.as_array()) {
        // Duplicate fields keep only the first occurrence (rendering the same field twice is meaningless)
        let mut seen = std::collections::HashSet::new();
        cfg.order = list
            .iter()
            .filter_map(|v| v.as_str().and_then(parse_field))
            .filter(|f| seen.insert(*f))
            .collect();
    }

    // [render] colors: monochrome rendering when false (SPEC §7.2 v1.4); non-bool falls to default
    if let Some(c) = val
        .get("render")
        .and_then(|r| r.get("colors"))
        .and_then(as_bool)
    {
        cfg.colors = c;
    }

    // [render.quota] five keys
    if let Some(q) = val
        .get("render")
        .and_then(|r| r.get("quota"))
        .and_then(|q| q.as_table())
    {
        cfg.quota.five_hour = q
            .get("five_hour")
            .and_then(as_bool)
            .unwrap_or(cfg.quota.five_hour);
        cfg.quota.week = q.get("week").and_then(as_bool).unwrap_or(cfg.quota.week);
        cfg.quota.month = q.get("month").and_then(as_bool).unwrap_or(cfg.quota.month);
        cfg.quota.reset_time = q
            .get("reset_time")
            .and_then(as_bool)
            .unwrap_or(cfg.quota.reset_time);
        cfg.quota.booster = q
            .get("booster")
            .and_then(as_bool)
            .unwrap_or(cfg.quota.booster);
    }

    // [thresholds]
    if let Some(t) = val.get("thresholds").and_then(|t| t.as_table()) {
        cfg.green_below = t
            .get("green_below")
            .and_then(as_f64)
            .unwrap_or(cfg.green_below);
        cfg.yellow_below = t
            .get("yellow_below")
            .and_then(as_f64)
            .unwrap_or(cfg.yellow_below);
    }

    // [cache] ttl/retry: defensive clamping so pathological config cannot trigger a refresh storm or panic
    if let Some(c) = val.get("cache").and_then(|c| c.as_table()) {
        if let Some(v) = c.get("ttl_seconds").and_then(as_u64) {
            cfg.ttl_seconds = v.max(1);
        }
        if let Some(v) = c.get("retry_seconds").and_then(as_u64) {
            // retry must be < ttl: only then does the rewind target land within the expiry line (fast-retry semantics)
            cfg.retry_seconds = v.min(cfg.ttl_seconds - 1);
        }
    }

    // [network]
    if let Some(n) = val.get("network").and_then(|n| n.as_table()) {
        if let Some(s) = n.get("base_url").and_then(|b| b.as_str())
            && !s.trim().is_empty()
        {
            cfg.base_url = Some(s.trim().to_string());
        }
        if let Some(v) = n.get("http_timeout_seconds").and_then(as_u64) {
            cfg.http_timeout_seconds = v.max(1);
        }
    }
    cfg
}

/// Loads from <kimi_home>/quota-bar.toml; missing/unreadable file -> all defaults.
pub fn load_from(kimi: &Path) -> Config {
    match std::fs::read_to_string(kimi.join("quota-bar.toml")) {
        Ok(text) => parse(&text),
        Err(_) => Config::default(),
    }
}

fn as_bool(v: &toml::Value) -> Option<bool> {
    v.as_bool()
}

fn as_f64(v: &toml::Value) -> Option<f64> {
    match v {
        toml::Value::Integer(i) => Some(*i as f64),
        toml::Value::Float(f) => Some(*f),
        _ => None,
    }
}

fn as_u64(v: &toml::Value) -> Option<u64> {
    match v {
        toml::Value::Integer(i) if *i >= 0 => Some(*i as u64),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Missing file/invalid TOML -> all defaults (SPEC §8: rendering never fails due to config).
    #[test]
    fn missing_and_invalid_fall_back_to_defaults() {
        let d = Config::default();
        let parsed = parse("not [valid toml");
        assert_eq!(parsed.order.len(), d.order.len());
        assert_eq!(parsed.ttl_seconds, 60);
        assert_eq!(parsed.retry_seconds, 30);
        assert_eq!(parsed.green_below, 60.0);
        assert_eq!(parsed.yellow_below, 85.0);
        assert_eq!(parsed.http_timeout_seconds, 8);
        assert!(parsed.base_url.is_none());
        assert!(!parsed.quota.booster);
    }

    /// order reordering and unknown-field ignoring (SPEC §8).
    #[test]
    fn order_reorder_and_unknown_fields_ignored() {
        let cfg = parse("[render]\norder = [\"quota\", \"git_branch\", \"wat\", \"model\"]\n");
        assert_eq!(
            cfg.order,
            vec![Field::Quota, Field::GitBranch, Field::Model]
        );

        // Empty array = display nothing (a legal reduction)
        let cfg = parse("[render]\norder = []\n");
        assert!(cfg.order.is_empty());

        // Non-array -> default
        let cfg = parse("[render]\norder = \"quota\"\n");
        assert_eq!(cfg.order, Config::default().order);
    }

    /// quota sub-section switches and threshold overrides; invalid types fall to default.
    #[test]
    fn quota_switches_thresholds_and_network() {
        let text = r#"
[render.quota]
five_hour = false
booster = true

[thresholds]
green_below = 50.5
yellow_below = 90

[cache]
ttl_seconds = 120
retry_seconds = 45

[network]
base_url = "https://api.kimi.ai/coding/v1"
http_timeout_seconds = 5
"#;
        let cfg = parse(text);
        assert!(!cfg.quota.five_hour);
        assert!(cfg.quota.week); // not listed = default true
        assert!(cfg.quota.booster);
        assert_eq!(cfg.green_below, 50.5);
        assert_eq!(cfg.yellow_below, 90.0);
        assert_eq!(cfg.ttl_seconds, 120);
        assert_eq!(cfg.retry_seconds, 45);
        assert_eq!(
            cfg.base_url.as_deref(),
            Some("https://api.kimi.ai/coding/v1")
        );
        assert_eq!(cfg.http_timeout_seconds, 5);
    }

    /// Pathological config clamping: retry >= ttl gets ttl-1; ttl minimum 1; timeout minimum 1.
    #[test]
    fn pathological_values_clamped() {
        let cfg = parse("[cache]\nttl_seconds = 30\nretry_seconds = 60\n");
        assert_eq!(cfg.ttl_seconds, 30);
        assert_eq!(cfg.retry_seconds, 29);

        let cfg = parse("[cache]\nttl_seconds = 0\nretry_seconds = 0\n");
        assert_eq!(cfg.ttl_seconds, 1);

        let cfg = parse("[network]\nhttp_timeout_seconds = 0\n");
        assert_eq!(cfg.http_timeout_seconds, 1);
    }

    /// order duplicate-field dedup (first occurrence wins; not defined in SPEC §8, the same field is not rendered twice).
    #[test]
    fn order_duplicates_deduped() {
        let cfg = parse("[render]\norder = [\"quota\", \"model\", \"quota\"]\n");
        assert_eq!(cfg.order, vec![Field::Quota, Field::Model]);
    }

    /// colors monochrome switch (SPEC §7.2 v1.4): default true; explicit false takes effect; non-bool
    /// and missing fall to default.
    #[test]
    fn colors_switch_parsed() {
        assert!(Config::default().colors);
        assert!(parse("").colors);
        assert!(!parse("[render]\ncolors = false\n").colors);
        assert!(parse("[render]\ncolors = true\n").colors);
        assert!(parse("[render]\ncolors = \"false\"\n").colors); // non-bool falls to default
    }

    /// tasks field (SPEC §8 v1.5): parse_field recognizes "tasks"; the built-in default order
    /// contains tasks and places it before the quota group (after thinking, before quota).
    #[test]
    fn tasks_field_parsed_and_positioned_in_default_order() {
        let d = Config::default();
        let pos = |f: Field| d.order.iter().position(|x| *x == f).unwrap();
        assert!(
            pos(Field::Tasks) < pos(Field::Quota),
            "tasks must come before the quota group"
        );
        assert!(
            pos(Field::Thinking) < pos(Field::Tasks),
            "tasks comes after thinking"
        );

        let cfg = parse("[render]\norder = [\"model\", \"tasks\"]\n");
        assert_eq!(cfg.order, vec![Field::Model, Field::Tasks]);
    }
}
