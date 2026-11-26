use serde::{Deserialize, Serialize};
use std::str::FromStr;

use super::cursor::KeysetCursor;

/// 收件箱列表按 `occurred_at` 排序，游标编码与随手记、待办共用同一套。
pub type InboxMessageCursor = KeysetCursor;

/// 消息源类型；`manual` 只给开发用 ingest 与未来的手动转发使用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InboxSourceKind {
    Github,
    Email,
    Rss,
    Manual,
}

impl InboxSourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Github => "github",
            Self::Email => "email",
            Self::Rss => "rss",
            Self::Manual => "manual",
        }
    }
}

impl FromStr for InboxSourceKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "github" => Ok(Self::Github),
            "email" => Ok(Self::Email),
            "rss" => Ok(Self::Rss),
            "manual" => Ok(Self::Manual),
            _ => Err(format!("unknown inbox source kind: {value}")),
        }
    }
}

/// 消息源只暴露展示与同步状态，`config_json` 与游标留在仓储层，不进 IPC。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxSource {
    pub id: String,
    pub kind: InboxSourceKind,
    pub name: String,
    pub enabled: bool,
    pub last_synced_at: Option<String>,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 判断结果。`capture` 预留给后续的「提取为随手记」，MVP 只写入前两种。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriageOutcome {
    Dismissed,
    Task,
    Capture,
}

impl TriageOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dismissed => "dismissed",
            Self::Task => "task",
            Self::Capture => "capture",
        }
    }
}

impl FromStr for TriageOutcome {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "dismissed" => Ok(Self::Dismissed),
            "task" => Ok(Self::Task),
            "capture" => Ok(Self::Capture),
            _ => Err(format!("unknown triage outcome: {value}")),
        }
    }
}

/// 一条收件箱消息：原文不可编辑，用户只能改已读与判断结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxMessage {
    pub id: String,
    pub source_id: String,
    pub external_id: String,
    /// mention / review_requested / assigned / ci_failed / release / mail
    pub category: String,
    pub title: String,
    pub summary: Option<String>,
    pub body: Option<String>,
    pub url: Option<String>,
    /// 触发人 / 发件人
    pub actor: Option<String>,
    pub occurred_at: String,
    /// 看过时间；不等于判断过。
    pub read_at: Option<String>,
    pub triaged_at: Option<String>,
    pub triage_outcome: Option<TriageOutcome>,
    /// 指向 `tasks.id` 或 `captures.id`，弱关联。
    pub triage_ref_id: Option<String>,
    pub related_kind: Option<String>,
    pub related_id: Option<String>,
}

/// 列表查询：来源、类型、未判断、未读四个筛选与键集游标都下沉到 SQL。
#[derive(Debug, Clone)]
pub struct InboxPageQuery {
    /// `None` 表示全部；`false` 只看未判断；`true` 只看已判断。
    pub triaged: Option<bool>,
    pub unread_only: bool,
    pub category: Option<String>,
    pub source_id: Option<String>,
    pub cursor: Option<InboxMessageCursor>,
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxPage {
    pub messages: Vec<InboxMessage>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

/// 导航角标只取 `pending`；`unread` 用于「读完先放着」的筛选提示。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxCounts {
    pub all: i64,
    pub pending: i64,
    pub unread: i64,
}

/// 入库入参：连接器与开发用 ingest 共用，去重键是 `(source_id, external_id)`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewInboxMessage {
    pub source_id: String,
    pub external_id: String,
    pub external_updated_at: Option<String>,
    pub category: String,
    pub title: String,
    pub summary: Option<String>,
    pub body: Option<String>,
    pub url: Option<String>,
    pub actor: Option<String>,
    /// 为 `None` 时由 SQL 填当前时间。
    pub occurred_at: Option<String>,
    pub related_kind: Option<String>,
    pub related_id: Option<String>,
    pub payload_json: Option<String>,
}
