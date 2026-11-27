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

/// 邮箱连接加密方式：`implicit` 是 993 直连 TLS，`starttls` 是 143 先明文再升级。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmailTls {
    #[default]
    Implicit,
    Starttls,
}

impl EmailTls {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Implicit => "implicit",
            Self::Starttls => "starttls",
        }
    }

    /// 用户没填端口时按加密方式给默认值，避免 993/143 配错。
    pub fn default_port(self) -> u16 {
        match self {
            Self::Implicit => 993,
            Self::Starttls => 143,
        }
    }
}

impl FromStr for EmailTls {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "implicit" => Ok(Self::Implicit),
            "starttls" => Ok(Self::Starttls),
            _ => Err(format!("unknown email tls mode: {value}")),
        }
    }
}

/// 设置页展示的账户状态：**不含密码**，凭据只在 Windows 凭据管理器里。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailAccountStatus {
    pub id: String,
    pub address: String,
    pub host: String,
    pub port: u16,
    pub tls: EmailTls,
    pub mailbox: String,
    pub enabled: bool,
    pub last_synced_at: Option<String>,
    pub last_error: Option<String>,
}

/// 「测试连接」的结果：只回读到的邮箱概况，不回任何凭据。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailAccountProbe {
    /// `EXAMINE` 返回的 EXISTS，即当前邮件总数。
    pub mailbox_total: u32,
    pub uid_validity: u32,
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

/// 收件箱的状态轴。四个 Tab 互斥且穷尽，结果直接成为 Tab，不再用减法凑数。
///
/// `Capture` 结果在 MVP 里不会写入，所以 `Pending` / `Task` / `Dismissed` 覆盖了
/// 现有全部数据；将来启用「提取为随手记」时需要补第五个 Tab。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InboxTab {
    /// `triaged_at IS NULL`
    #[default]
    Pending,
    /// `triage_outcome = 'task'`
    Task,
    /// `triage_outcome = 'dismissed'`
    Dismissed,
    /// 无条件
    All,
}

impl InboxTab {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Task => "task",
            Self::Dismissed => "dismissed",
            Self::All => "all",
        }
    }
}

impl FromStr for InboxTab {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "pending" => Ok(Self::Pending),
            "task" => Ok(Self::Task),
            "dismissed" => Ok(Self::Dismissed),
            "all" => Ok(Self::All),
            _ => Err(format!("unknown inbox tab: {value}")),
        }
    }
}

/// 列表与「选择全部 N 条」共用同一套条件，避免两处筛选漂移。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InboxScope {
    pub tab: InboxTab,
    pub unread_only: bool,
    pub category: Option<String>,
    pub source_id: Option<String>,
}

/// 列表查询：状态轴、未读、类型、来源四个筛选与键集游标都下沉到 SQL。
#[derive(Debug, Clone)]
pub struct InboxPageQuery {
    pub scope: InboxScope,
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

/// 一次查询给出五个数：导航角标只取 `pending`，四个 Tab 各取自己的一项。
///
/// 计数随来源与分类筛选，不随 Tab 与未读筛选 —— 这样同一来源下四个 Tab 的数字
/// 相加等于「全部」，切 Tab 不会出现对不上的数字。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxCounts {
    pub all: i64,
    pub pending: i64,
    pub task: i64,
    pub dismissed: i64,
    /// 与「只看未读」筛选同一口径：`read_at IS NULL`，不再要求未处理。
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

/// 已读回写队列的状态。`pending` 会按退避时间自动重试，`blocked` 需要人工处理。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadBackStatus {
    Pending,
    Blocked,
}

impl ReadBackStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Blocked => "blocked",
        }
    }
}

impl FromStr for ReadBackStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "pending" => Ok(Self::Pending),
            "blocked" => Ok(Self::Blocked),
            _ => Err(format!("unknown read back status: {value}")),
        }
    }
}

/// 失败原因分类只影响重试策略，不参与展示；展示用 `last_error` 的原文。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadBackErrorKind {
    /// 网络中断、超时或 5xx，属于可恢复的瞬时波动。
    Transient,
    /// 限流，下一次尝试按 `x-ratelimit-reset` 或 `retry-after` 安排。
    RateLimited,
    /// Token 无效或缺 `notifications` 权限，重试没有意义，等重新授权。
    Auth,
    /// 退避次数用尽，仍可能是瞬时问题，提供手动重试入口。
    Exhausted,
}

impl ReadBackErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Transient => "transient",
            Self::RateLimited => "rate_limited",
            Self::Auth => "auth",
            Self::Exhausted => "exhausted",
        }
    }
}

impl FromStr for ReadBackErrorKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "transient" => Ok(Self::Transient),
            "rate_limited" => Ok(Self::RateLimited),
            "auth" => Ok(Self::Auth),
            "exhausted" => Ok(Self::Exhausted),
            _ => Err(format!("unknown read back error kind: {value}")),
        }
    }
}

/// 队列中的一条已读回写：只在远端写入失败后存在，成功后即删除。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboxReadBack {
    pub id: String,
    pub message_id: String,
    pub source_id: String,
    pub external_id: String,
    pub status: ReadBackStatus,
    pub attempts: u32,
    pub next_attempt_at: String,
    pub last_error: Option<String>,
    pub last_error_kind: Option<ReadBackErrorKind>,
}

/// 设置页与收件箱共同展示的队列概览，一次查询给出计数与最近一次错误。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxReadBackStatus {
    pub pending: i64,
    pub blocked: i64,
    pub last_error: Option<String>,
}
