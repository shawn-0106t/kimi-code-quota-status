// quota-status logic library: rendering/fetching/parsing/caching all live in this crate, shared by the
// bin entry and tests/ integration tests (golden parity) (P4 depends on the lib target to import parsing functions).

pub mod cache;
pub mod config;
pub mod console;
pub mod credentials;
pub mod http;
pub mod quota;
pub mod render;
pub mod tasks;
