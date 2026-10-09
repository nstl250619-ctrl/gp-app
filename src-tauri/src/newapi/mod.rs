//! new-api 集成层（§8）。只做 HTTP 与 DTO，不含业务编排。

pub mod client;
pub mod types;

pub use client::NewApiClient;
pub use types::{InstanceStatus, LogItem, TopUpRecord};
