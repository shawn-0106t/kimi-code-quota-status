// 凭证链，1:1 移植自 repos/kimi-planbar-tui/rust/src/credentials.rs（SPEC §5.2）：
// 1) <kimi_home>/credentials/kimi-code.json -> access_token（expires_at > now+30s）
// 2) <kimi_home>/config.toml -> base_url 含 api.kimi.com/coding 且 api_key 非空的 provider
// 3) 皆无 -> 调用方报 "no-token"
// token 新鲜度依赖运行中的 CLI，本工具绝不自行刷新（SPEC §1.2/§5.2，只读不写）。

use serde_json::Value;
use std::fs;
use std::path::PathBuf;

/// JSON number-or-string -> f64（服务端把数字按字符串建模）。
fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

fn home_dir() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("USERPROFILE")
        && !p.is_empty()
    {
        return Some(PathBuf::from(p));
    }
    // Fallback: HOMEDRIVE + HOMEPATH
    if let (Ok(d), Ok(p)) = (std::env::var("HOMEDRIVE"), std::env::var("HOMEPATH")) {
        return Some(PathBuf::from(format!("{d}{p}")));
    }
    None
}

/// <kimi_home> 解析提为 pub：凭证/缓存/配置三处路径共用（SPEC §5.2/§5.3/§8）。
/// = %USERPROFILE%/.kimi-code，env KIMI_CODE_HOME 非空时整体覆盖。
pub fn kimi_home() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("KIMI_CODE_HOME")
        && !p.is_empty()
    {
        return Some(PathBuf::from(p));
    }
    home_dir().map(|h| h.join(".kimi-code"))
}

pub fn load_token() -> Option<String> {
    let kimi = kimi_home()?;

    // 1) OAuth access token（来自凭证存储；过期视为无效继续兜底）
    let cred = kimi.join("credentials").join("kimi-code.json");
    if let Ok(text) = fs::read_to_string(&cred)
        && let Ok(v) = serde_json::from_str::<Value>(&text)
        && let Some(at) = v.get("access_token").and_then(|x| x.as_str())
    {
        let exp = v.get("expires_at").and_then(as_f64).unwrap_or(0.0);
        let now = chrono::Utc::now().timestamp() as f64;
        if exp > now + 30.0 {
            return Some(at.to_string());
        }
    }

    // 2) config.toml 兜底：逐行解析（非完整 TOML parser）
    let cfg_path = kimi.join("config.toml");
    let text = fs::read_to_string(&cfg_path).ok()?;
    parse_config_provider(&text)
}

/// config.toml 逐行扫描，拆出供单测（纯文本 -> api_key）。
/// 参考实现用 regex `^(base_url|api_key)\s*=\s*"([^"]*)"` 抽取键值；regex 不在
/// SPEC §11 依赖白名单，此处手写等价解析（语义由移植单测钉死）。
fn parse_config_provider(text: &str) -> Option<String> {
    let mut section: Option<String> = None;
    let mut base_url: Option<String> = None;
    let mut api_key: Option<String> = None;
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            // Settle the previous section before starting a new one
            if let Some(found) =
                match_provider(section.as_deref(), base_url.as_deref(), api_key.as_deref())
            {
                return Some(found);
            }
            // REVIEW-RUST Suggestion 7: strip the brackets, then trim — the
            // real CLI writes "[providers.x]" compact, but "[ providers.x ]"
            // used to leave padding in the section name and miss the match.
            section = Some(
                line.trim_matches(|c| c == '[' || c == ']')
                    .trim()
                    .to_string(),
            );
            base_url = None;
            api_key = None;
            continue;
        }
        if let Some((key, val)) = parse_kv(line) {
            if key == "base_url" {
                base_url = Some(val.to_string());
            } else {
                api_key = Some(val.to_string());
            }
        }
    }
    match_provider(section.as_deref(), base_url.as_deref(), api_key.as_deref())
}

/// 等价 regex `^(base_url|api_key)\s*=\s*"([^"]*)"`：key 必须紧邻行首（其后仅
/// 空白到 `=`），值取首个引号包裹段（可含空串，行尾多余内容忽略）。
fn parse_kv(line: &str) -> Option<(&'static str, &str)> {
    let eq = line.find('=')?;
    let key = match line[..eq].trim_end() {
        "base_url" => "base_url",
        "api_key" => "api_key",
        _ => return None,
    };
    let val = line[eq + 1..].trim_start().strip_prefix('"')?;
    let end = val.find('"')?;
    Some((key, &val[..end]))
}

fn match_provider(
    section: Option<&str>,
    base_url: Option<&str>,
    api_key: Option<&str>,
) -> Option<String> {
    match (section, base_url, api_key) {
        (Some(s), Some(b), Some(k))
            if s.starts_with("providers.")
                // SPEC §5.2 勘误增强：国际站（api.kimi.ai/coding）纯 API key
                // 用户也走此兜底（参考实现只认 api.kimi.com/coding）
                && (b.contains("api.kimi.com/coding") || b.contains("api.kimi.ai/coding"))
                && !k.is_empty() =>
        {
            Some(k.to_string())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// REVIEW-RUST Suggestion 7 regression: compact (real CLI shape) and
    /// bracket-padded section names must both match.
    #[test]
    fn config_toml_compact_and_padded_sections() {
        let compact = "[providers.kimi]\nbase_url = \"https://api.kimi.com/coding/v1\"\napi_key = \"key-1\"\n";
        assert_eq!(parse_config_provider(compact), Some("key-1".to_string()));

        let padded = "[ providers.kimi ]\nbase_url = \"https://api.kimi.com/coding/v1\"\napi_key = \"key-2\"\n";
        assert_eq!(parse_config_provider(padded), Some("key-2".to_string()));
    }

    /// Section settlement: a non-matching section must not leak into the
    /// next one; an empty api_key is rejected (SPEC 16.2).
    #[test]
    fn config_toml_section_settlement() {
        let first_match_wins = "[providers.kimi]\nbase_url = \"https://api.kimi.com/coding/v1\"\napi_key = \"key-3\"\n\
            [providers.other]\nbase_url = \"https://api.kimi.com/coding/v1\"\napi_key = \"later\"\n";
        assert_eq!(
            parse_config_provider(first_match_wins),
            Some("key-3".to_string())
        );

        let skips_unrelated = "[providers.other]\nbase_url = \"https://example.com/v1\"\napi_key = \"ignored\"\n\
            [providers.kimi]\nbase_url = \"https://api.kimi.com/coding/v1\"\napi_key = \"key-4\"\n";
        assert_eq!(
            parse_config_provider(skips_unrelated),
            Some("key-4".to_string())
        );

        let empty_key =
            "[providers.kimi]\nbase_url = \"https://api.kimi.com/coding/v1\"\napi_key = \"\"\n";
        assert_eq!(parse_config_provider(empty_key), None);

        assert_eq!(parse_config_provider("not toml at all"), None);
    }

    /// SPEC §5.2 增强回归：国际站 provider（api.kimi.ai/coding）也能兜底取 key。
    #[test]
    fn config_toml_matches_international_host() {
        let intl = "[providers.kimi-global]\nbase_url = \"https://api.kimi.ai/coding/v1\"\napi_key = \"key-global\"\n";
        assert_eq!(parse_config_provider(intl), Some("key-global".to_string()));
    }

    /// parse_kv 与原 regex 语义对齐的边界：键名前缀不匹配、无引号值、未闭合
    /// 引号、行尾多余内容均不误取。
    #[test]
    fn parse_kv_rejects_non_matching_shapes() {
        assert_eq!(parse_kv("base_urls = \"x\""), None); // 键名后缀不是空白/= 
        assert_eq!(parse_kv("foo_base_url = \"x\""), None); // 行首不是键名
        assert_eq!(parse_kv("base_url = x"), None); // 无引号
        assert_eq!(parse_kv("api_key = \"unclosed"), None); // 未闭合
        assert_eq!(parse_kv("# base_url = \"x\""), None); // 注释行
        assert_eq!(parse_kv("base_url=\"v\" trailing"), Some(("base_url", "v")));
        assert_eq!(parse_kv("api_key=\"\""), Some(("api_key", "")));
    }
}
