// Credential chain, ported 1:1 from repos/kimi-planbar-tui/rust/src/credentials.rs (SPEC §5.2):
// 1) <kimi_home>/credentials/kimi-code.json -> access_token (expires_at > now+30s)
// 2) <kimi_home>/config.toml -> provider whose base_url contains api.kimi.com/coding and whose api_key is non-empty
// 3) none of the above -> caller reports "no-token"
// Token freshness relies on the running CLI; this tool never refreshes tokens itself (SPEC §1.2/§5.2, read-only).

use serde_json::Value;
use std::fs;
use std::path::PathBuf;

/// JSON number-or-string -> f64 (the server models numbers as strings).
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

/// <kimi_home> resolution made pub: shared by the credential/cache/config paths (SPEC §5.2/§5.3/§8).
/// = %USERPROFILE%/.kimi-code, wholly overridden when env KIMI_CODE_HOME is non-empty.
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

    // 1) OAuth access token (from the credential store; expired counts as invalid and falls through)
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

    // 2) config.toml fallback: line-by-line parsing (not a full TOML parser)
    let cfg_path = kimi.join("config.toml");
    let text = fs::read_to_string(&cfg_path).ok()?;
    parse_config_provider(&text)
}

/// Line-by-line scan of config.toml, split out for unit tests (plain text -> api_key).
/// The reference implementation uses regex `^(base_url|api_key)\s*=\s*"([^"]*)"` to extract key-values; regex is not
/// in the SPEC §11 dependency allowlist, so this is a hand-written equivalent parser (semantics pinned by ported unit tests).
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

/// Equivalent to regex `^(base_url|api_key)\s*=\s*"([^"]*)"`: the key must sit right at line start
/// (only whitespace before `=`); the value is the first quoted segment (may be empty, trailing content ignored).
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
                // SPEC §5.2 erratum enhancement: international-site (api.kimi.ai/coding) pure API key
                // users also take this fallback (the reference implementation only accepted api.kimi.com/coding)
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

    /// SPEC §5.2 enhancement regression: an international-site provider (api.kimi.ai/coding) can also fall back to fetch the key.
    #[test]
    fn config_toml_matches_international_host() {
        let intl = "[providers.kimi-global]\nbase_url = \"https://api.kimi.ai/coding/v1\"\napi_key = \"key-global\"\n";
        assert_eq!(parse_config_provider(intl), Some("key-global".to_string()));
    }

    /// Edge cases aligning parse_kv with the original regex semantics: key-name prefix mismatch, unquoted value,
    /// unclosed quote, and trailing line content are all rejected without a false match.
    #[test]
    fn parse_kv_rejects_non_matching_shapes() {
        assert_eq!(parse_kv("base_urls = \"x\""), None); // what follows the key name is not whitespace/= 
        assert_eq!(parse_kv("foo_base_url = \"x\""), None); // line does not start with a key name
        assert_eq!(parse_kv("base_url = x"), None); // no quotes
        assert_eq!(parse_kv("api_key = \"unclosed"), None); // unclosed
        assert_eq!(parse_kv("# base_url = \"x\""), None); // comment line
        assert_eq!(parse_kv("base_url=\"v\" trailing"), Some(("base_url", "v")));
        assert_eq!(parse_kv("api_key=\"\""), Some(("api_key", "")));
    }
}
