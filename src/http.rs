// 共享 HTTP client（OnceLock 单例，语义移植自 repos/kimi-planbar-tui http.rs
// 的"OnceLock 共享 client"）。ureq 3 的 Agent 构建不可失败，故原实现
// "构建失败降级 None" 的路径在本 client 下不存在。
// 超时按决策清单定 8s（SPEC §5.1；参考实现原值 10s）。

use std::sync::OnceLock;
use std::time::Duration;
use ureq::Agent;

/// 进程内 OnceLock 单例：**仅首次调用的 timeout 生效**，后续传参静默忽略。
/// 当前每进程只在 quota::fetch 调用一次（--refresh/--test-fetch 单次取数），
/// 无实际影响；渲染模式不取数、不触碰本 client。
pub(crate) fn shared_client(timeout: Duration) -> &'static Agent {
    static CLIENT: OnceLock<Agent> = OnceLock::new();
    CLIENT.get_or_init(|| {
        Agent::config_builder()
            .timeout_global(Some(timeout))
            .build()
            .new_agent()
    })
}
