// 字段配置模块（SPEC §8）：解析 <kimi_home>/quota-bar.toml，控制字段开关与
// 顺序、颜色阈值、缓存 TTL、网络覆盖。缺失/非法/未知字段一律落回内置默认
//（渲染绝不因配置失败），路径随 KIMI_CODE_HOME 联动。

use std::path::Path;

/// 行内字段（order 中的合法名字；未知名字忽略）
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Field {
    PermissionMode,
    Model,
    Thinking,
    /// tasks/agents 徽章（SPEC §7.7 v1.5）：开关即 order——删去 "tasks" 即
    /// 关闭整段并连带跳过 sessions 目录扫描（省 IO）
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
    pub reset_time: bool, // false 等价于降级第 1 级（永久丢 reset 后缀）
    pub booster: bool,    // 默认不显示 booster 钱包（数据仍解析入缓存）
}

#[derive(Clone, Debug)]
pub struct Config {
    pub order: Vec<Field>,
    /// 单色开关（SPEC §7.2 v1.4）：false 时渲染输出纯文本（无任何 SGR），
    /// 整行由宿主包装为主题 text 色（与第 2 行 context 同色，随 /theme 联动）
    pub colors: bool,
    pub quota: QuotaFields,
    /// percent < green_below 绿；< yellow_below 黄；否则红
    pub green_below: f64,
    pub yellow_below: f64,
    pub ttl_seconds: u64,
    pub retry_seconds: u64,
    /// [network] base_url；None = 用内置默认端点
    pub base_url: Option<String>,
    pub http_timeout_seconds: u64,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            // v1.5：tasks 徽章位于额度组之前（SPEC §7.1/§8）
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

/// 从 quota-bar.toml 文本解析；整体 TOML 非法 -> 全默认。
/// 单键非法只影响该键（落回默认）；未知字段忽略。
pub fn parse(text: &str) -> Config {
    let mut cfg = Config::default();
    let Ok(val) = toml::from_str::<toml::Value>(text) else {
        return cfg;
    };

    // [render] order：数组则采用（可为空 = 全部不显示），非数组落默认
    let order = val.get("render").and_then(|r| r.get("order"));
    if let Some(list) = order.and_then(|o| o.as_array()) {
        // 重复字段只保留首次出现（同一字段渲染两次无意义）
        let mut seen = std::collections::HashSet::new();
        cfg.order = list
            .iter()
            .filter_map(|v| v.as_str().and_then(parse_field))
            .filter(|f| seen.insert(*f))
            .collect();
    }

    // [render] colors：false 时单色渲染（SPEC §7.2 v1.4）；非 bool 落默认
    if let Some(c) = val
        .get("render")
        .and_then(|r| r.get("colors"))
        .and_then(as_bool)
    {
        cfg.colors = c;
    }

    // [render.quota] 五键
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

    // [cache] ttl/retry：防御性钳位避免病态配置引发刷新风暴或 panic
    if let Some(c) = val.get("cache").and_then(|c| c.as_table()) {
        if let Some(v) = c.get("ttl_seconds").and_then(as_u64) {
            cfg.ttl_seconds = v.max(1);
        }
        if let Some(v) = c.get("retry_seconds").and_then(as_u64) {
            // retry 必须 < ttl：回拨目标才落在过期线内（快重试语义）
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

/// 从 <kimi_home>/quota-bar.toml 加载；文件缺失/不可读 -> 全默认。
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

    /// 缺失文件/非法 TOML -> 全默认（SPEC §8：渲染绝不因配置失败）。
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

    /// order 重排与未知字段忽略（SPEC §8）。
    #[test]
    fn order_reorder_and_unknown_fields_ignored() {
        let cfg = parse("[render]\norder = [\"quota\", \"git_branch\", \"wat\", \"model\"]\n");
        assert_eq!(
            cfg.order,
            vec![Field::Quota, Field::GitBranch, Field::Model]
        );

        // 空数组 = 全部不显示（合法的删减）
        let cfg = parse("[render]\norder = []\n");
        assert!(cfg.order.is_empty());

        // 非数组 -> 默认
        let cfg = parse("[render]\norder = \"quota\"\n");
        assert_eq!(cfg.order, Config::default().order);
    }

    /// quota 子段开关与阈值覆盖；非法类型落默认。
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
        assert!(cfg.quota.week); // 未列 = 默认 true
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

    /// 病态配置钳位：retry >= ttl 收到 ttl-1；ttl 最小 1；超时最小 1。
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

    /// order 重复字段去重（保留首次出现；SPEC §8 未定义，同一字段不重复渲染）。
    #[test]
    fn order_duplicates_deduped() {
        let cfg = parse("[render]\norder = [\"quota\", \"model\", \"quota\"]\n");
        assert_eq!(cfg.order, vec![Field::Quota, Field::Model]);
    }

    /// colors 单色开关（SPEC §7.2 v1.4）：默认 true；显式 false 生效；非 bool
    /// 与缺失落默认。
    #[test]
    fn colors_switch_parsed() {
        assert!(Config::default().colors);
        assert!(parse("").colors);
        assert!(!parse("[render]\ncolors = false\n").colors);
        assert!(parse("[render]\ncolors = true\n").colors);
        assert!(parse("[render]\ncolors = \"false\"\n").colors); // 非 bool 落默认
    }

    /// tasks 字段（SPEC §8 v1.5）：parse_field 认识 "tasks"；内置默认 order
    /// 含 tasks 且位于额度组之前（thinking 之后、quota 之前）。
    #[test]
    fn tasks_field_parsed_and_positioned_in_default_order() {
        let d = Config::default();
        let pos = |f: Field| d.order.iter().position(|x| *x == f).unwrap();
        assert!(
            pos(Field::Tasks) < pos(Field::Quota),
            "tasks 须在额度组之前"
        );
        assert!(
            pos(Field::Thinking) < pos(Field::Tasks),
            "tasks 在 thinking 之后"
        );

        let cfg = parse("[render]\norder = [\"model\", \"tasks\"]\n");
        assert_eq!(cfg.order, vec![Field::Model, Field::Tasks]);
    }
}
