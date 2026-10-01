// quota-status —— Kimi Code CLI statusline 额度显示器（SPEC §1）。
//
// 三种运行模式（SPEC §4）：
//   渲染模式（无参数）：stdin 快照 + 本地缓存 → 拼一行 ANSI 文本，毫秒级退出；
//   取数模式 --refresh：凭证链 → GET /usages → 防御解析 → 原子写缓存；
//   自检模式 --test-fetch：同取数但不写缓存，结果 pretty JSON 打印到 stdout。
//
// 实现全部在 lib crate（src/lib.rs），本文件只做 argv 分发。

use quota_status::{cache, config, console, credentials, quota, render};
use std::io::Write;

fn load_config() -> config::Config {
    credentials::kimi_home()
        .map(|k| config::load_from(&k))
        .unwrap_or_default()
}

/// 读 thinking 相关段的 config.toml 文本（失败 -> None，段省略）。
/// 与 credentials 的逐行 provider 扫描互不干扰（独立解析函数）。
fn config_toml_text() -> Option<String> {
    std::fs::read_to_string(credentials::kimi_home()?.join("config.toml")).ok()
}

fn main() {
    let mode = std::env::args().nth(1);
    let cfg = load_config();
    match mode.as_deref() {
        // 取数模式（SPEC §4.2）：凭证链 -> GET /usages（8s 超时，可配）->
        // 防御解析 -> 原子写缓存；任何失败不写缓存（LKG + fast-retry），总是 exit 0
        Some("--refresh") => {
            let result = quota::fetch(cfg.base_url.as_deref(), cfg.http_timeout_seconds);
            if result.error.is_none() {
                if let Some(kimi) = credentials::kimi_home() {
                    let _ = cache::write_cache_atomic_at(&cache::cache_path(&kimi), &result);
                }
            }
        }
        // 自检模式（SPEC §4.3）：完整取数一次，pretty JSON（2 空格缩进
        // camelCase）打印到 stdout，不读 stdin、不写缓存
        Some("--test-fetch") => {
            let r = quota::fetch(cfg.base_url.as_deref(), cfg.http_timeout_seconds);
            println!("{}", r.to_pretty_json());
        }
        // 渲染模式（SPEC §4.1）
        _ => {
            // 步骤 1：stdin 读到 EOF 再解析；非法按空 payload
            let mut buf = Vec::new();
            let _ = std::io::Read::read_to_end(&mut std::io::stdin(), &mut buf);
            let payload = render::payload_from_bytes(&buf);

            // 步骤 2–3：缓存判定（age >= TTL -> 回拨 mtime + 派生 detached --refresh）
            let cached = credentials::kimi_home().and_then(|kimi| {
                cache::refresh_if_stale(
                    &kimi,
                    cfg.ttl_seconds,
                    cfg.retry_seconds,
                    || {
                        let _ = console::spawn_detached_refresh();
                    },
                )
            });

            // 步骤 4：thinking 段（开关关闭时跳过 config.toml 读取，省 IO）
            let thinking = if cfg.order.contains(&config::Field::Thinking) {
                render::thinking_from_config(
                    config_toml_text().as_deref(),
                    payload.get("model").and_then(|v| v.as_str()),
                )
            } else {
                None
            };

            // 宽度感知降级（§7.5）+ 行拼接（§7.4）
            let line = render::render_line(
                &payload,
                cached.as_ref(),
                &cfg,
                thinking.as_ref(),
                console::console_width(),
                chrono::Local::now(),
            );

            // 输出恰一行 + \n，stdout 强制 UTF-8 字节（§7.6），任何情况 exit 0
            let stdout = std::io::stdout();
            let mut lock = stdout.lock();
            let _ = lock.write_all(line.as_bytes());
            let _ = lock.write_all(b"\n");
            let _ = lock.flush();
        }
    }
    // 渲染模式任何情况下 exit 0（SPEC §3.2）；其余模式同样以退出码 0 结束
    //（失败语义由"缓存未更新"/"error 字段"表达，SPEC §4.2/§4.3）。
    std::process::exit(0);
}
