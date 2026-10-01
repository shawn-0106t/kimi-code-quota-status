// quota-status 逻辑库：渲染/取数/解析/缓存全在此 crate，供 bin 入口与
// tests/ 集成测试（golden parity）共用（P4 依赖 lib 目标导入解析函数）。

pub mod cache;
pub mod config;
pub mod console;
pub mod credentials;
pub mod http;
pub mod quota;
pub mod render;
