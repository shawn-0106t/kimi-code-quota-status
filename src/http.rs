// Shared HTTP client (OnceLock singleton, semantics ported from repos/kimi-planbar-tui http.rs's
// "OnceLock shared client"). ureq 3's Agent construction cannot fail, so the original
// "construction-failure degrades to None" path does not exist in this client.
// Timeout set to 8s per the decision list (SPEC §5.1; the reference implementation used 10s).

use std::sync::OnceLock;
use std::time::Duration;
use ureq::Agent;

/// In-process OnceLock singleton: **only the first call's timeout takes effect**, later arguments are silently ignored.
/// Currently called only once per process, from quota::fetch (--refresh/--test-fetch single fetch),
/// so no practical impact; render mode does no fetching and never touches this client.
pub(crate) fn shared_client(timeout: Duration) -> &'static Agent {
    static CLIENT: OnceLock<Agent> = OnceLock::new();
    CLIENT.get_or_init(|| {
        Agent::config_builder()
            .timeout_global(Some(timeout))
            .build()
            .new_agent()
    })
}
