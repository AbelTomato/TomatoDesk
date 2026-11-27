//! Infrastructure adapters for persistence, remote APIs, and secrets.

pub mod db;
pub mod email;
pub mod github;
pub mod link_label;
pub mod secrets;
/// `WebMetadata` 与抓取入口对回填脚本可见；正文回源仍留在 crate 内。
pub mod web_metadata;
