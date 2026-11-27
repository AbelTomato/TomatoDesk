//! Infrastructure adapters for persistence, remote APIs, and secrets.

pub mod db;
pub mod email;
pub mod github;
pub mod link_label;
pub mod secrets;
/// 只有 `WebMetadata` 对外可见：`link_label` 的公开入参需要它，抓取函数仍留在 crate 内。
pub mod web_metadata;
