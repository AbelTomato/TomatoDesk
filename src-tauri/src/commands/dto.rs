use serde::{Deserialize, de::DeserializeOwned};
use serde_json::Value;

use crate::{
    commands::error::{CommandError, CommandResult},
    domain::{
        CaptureCursor, CaptureKind, CapturePageQuery, CaptureSort, EmailTls, InboxAction,
        InboxMessageCursor, InboxPageQuery, InboxScope, InboxTab, NewInboxMessage, TaskCursor,
        TaskPageQuery, TaskStatus, ThreadKind, ThreadQuery, ThreadRelationship, ThreadState,
    },
};

const MAX_ID_LENGTH: usize = 128;
const MAX_SEARCH_LENGTH: usize = 512;
const MAX_TAGS_PER_ITEM: usize = 100;
const MAX_CAPTURE_LENGTH: usize = 16_384;
const MAX_TOKEN_LENGTH: usize = 512;
const MAX_DESCRIPTION_LENGTH: usize = 4_096;
const MAX_NOTE_LENGTH: usize = 16_384;
const MAX_TASK_TITLE_LENGTH: usize = 512;
const MAX_EXTERNAL_ID_LENGTH: usize = 512;
const MAX_CATEGORY_LENGTH: usize = 64;
const MAX_TIMESTAMP_LENGTH: usize = 64;
const MAX_SUMMARY_LENGTH: usize = 4_096;
const MAX_BODY_LENGTH: usize = 65_536;
const MAX_URL_LENGTH: usize = 4_096;
const MAX_ACTOR_LENGTH: usize = 256;
const MAX_PAYLOAD_LENGTH: usize = 65_536;
const MAX_EMAIL_ADDRESS_LENGTH: usize = 320;
const MAX_EMAIL_HOST_LENGTH: usize = 253;
const MAX_EMAIL_PASSWORD_LENGTH: usize = 512;
const MAX_MAILBOX_LENGTH: usize = 128;
/// 显式勾选的上限：前端一页 50 条，这里留出余量但不接受无界输入。
const MAX_ACTION_IDS: usize = 200;
/// 批量转待办的上限：撤销「转待办」不会删掉待办，误转只能手删，
/// 所以这条只接受一页以内的显式勾选（见方案 6.5 / 12）。
const MAX_TASK_ACTION_IDS: usize = 50;
const DEFAULT_PAGE_SIZE: u32 = 50;
const MAX_PAGE_SIZE: u32 = 200;

pub(super) fn parse_input<T: DeserializeOwned>(input: Option<Value>) -> CommandResult<T> {
    let input = input.ok_or_else(|| CommandError::validation("input is required"))?;
    serde_json::from_value(input).map_err(|_| CommandError::validation("input is invalid"))
}

/// 可选入参：前端不传时取 `Default`，免得为了「没有筛选」专门造一个空对象。
pub(super) fn parse_optional_input<T: DeserializeOwned + Default>(
    input: Option<Value>,
) -> CommandResult<T> {
    match input {
        Some(value) => {
            serde_json::from_value(value).map_err(|_| CommandError::validation("input is invalid"))
        }
        None => Ok(T::default()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateCaptureInput {
    pub kind: CaptureKind,
    pub source: String,
    pub url: Option<String>,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub note: Option<String>,
    /// 完整正文；笔记类由调用方写入，链接类通常留空等回源抓取。
    pub body: Option<String>,
    /// 挂在某个 GitHub 仓库下时传入，普通随手记为 `None`。
    pub repo_id: Option<String>,
}

impl CreateCaptureInput {
    pub(super) fn validate(mut self) -> CommandResult<Self> {
        self.source = required_text("source", self.source, 4096)?;
        self.repo_id = optional_text("repoId", self.repo_id.take(), MAX_ID_LENGTH)?;
        Ok(self)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QuickCaptureInput {
    pub value: String,
    pub note: Option<String>,
}

impl QuickCaptureInput {
    pub(super) fn validate(mut self) -> CommandResult<Self> {
        self.value = required_text("value", self.value, MAX_CAPTURE_LENGTH)?;
        self.note = match self.note.take() {
            Some(note) if note.trim().is_empty() => None,
            Some(note) => Some(required_text("note", note, MAX_CAPTURE_LENGTH)?),
            None => None,
        };
        Ok(self)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateCaptureInput {
    pub id: String,
    pub kind: CaptureKind,
    pub source: String,
    pub url: Option<String>,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub note: Option<String>,
}

impl UpdateCaptureInput {
    pub(super) fn validate(mut self) -> CommandResult<Self> {
        self.id = required_text("id", self.id, MAX_ID_LENGTH)?;
        self.source = required_text("source", self.source, 4096)?;
        Ok(self)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListCapturesPageInput {
    /// `None` 表示全部；`false` 只看未归档；`true` 只看已归档。
    pub archived: Option<bool>,
    pub kind: Option<CaptureKind>,
    pub repo_id: Option<String>,
    pub sort: Option<CaptureSort>,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

impl ListCapturesPageInput {
    pub(super) fn into_query(self) -> CommandResult<CapturePageQuery> {
        let limit = self.limit.unwrap_or(DEFAULT_PAGE_SIZE);
        if limit == 0 || limit > MAX_PAGE_SIZE {
            return Err(CommandError::validation(format!(
                "limit must be between 1 and {MAX_PAGE_SIZE}"
            )));
        }
        let cursor = match self.cursor.as_deref().map(str::trim) {
            Some(raw) if !raw.is_empty() => {
                Some(CaptureCursor::parse(raw).ok_or_else(|| {
                    CommandError::validation("cursor is invalid, reload the list")
                })?)
            }
            _ => None,
        };
        Ok(CapturePageQuery {
            archived: self.archived,
            kind: self.kind,
            repo_id: optional_text("repoId", self.repo_id, MAX_ID_LENGTH)?,
            sort: self.sort.unwrap_or(CaptureSort::Updated),
            cursor,
            limit,
        })
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CountCapturesInput {
    pub kind: Option<CaptureKind>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdInput {
    pub id: String,
}

impl IdInput {
    pub(super) fn validate(mut self) -> CommandResult<Self> {
        self.id = required_text("id", self.id, MAX_ID_LENGTH)?;
        Ok(self)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchCapturesInput {
    pub query: String,
}

impl SearchCapturesInput {
    pub(super) fn validate(self) -> CommandResult<Self> {
        if self.query.chars().count() > MAX_SEARCH_LENGTH {
            return Err(CommandError::validation(format!(
                "query cannot exceed {MAX_SEARCH_LENGTH} characters"
            )));
        }
        Ok(self)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateTagInput {
    pub name: String,
    pub color: Option<String>,
}

impl CreateTagInput {
    pub(super) fn validate(mut self) -> CommandResult<Self> {
        self.name = required_text("name", self.name, 128)?;
        if self
            .color
            .as_ref()
            .is_some_and(|color| color.chars().count() > 32)
        {
            return Err(CommandError::validation(
                "color cannot exceed 32 characters",
            ));
        }
        Ok(self)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetCaptureTagsInput {
    pub capture_id: String,
    pub tag_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GithubRepoInput {
    pub url: String,
}

impl GithubRepoInput {
    pub(super) fn validate(mut self) -> CommandResult<Self> {
        self.url = required_text("url", self.url, 4096)?;
        Ok(self)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GithubTokenInput {
    pub token: String,
}

impl GithubTokenInput {
    pub(super) fn validate(mut self) -> CommandResult<Self> {
        self.token = required_text("token", self.token, MAX_TOKEN_LENGTH)?;
        Ok(self)
    }
}

impl SetCaptureTagsInput {
    pub(super) fn validate(mut self) -> CommandResult<Self> {
        self.capture_id = required_text("captureId", self.capture_id, MAX_ID_LENGTH)?;
        if self.tag_ids.len() > MAX_TAGS_PER_ITEM {
            return Err(CommandError::validation(format!(
                "tagIds cannot contain more than {MAX_TAGS_PER_ITEM} entries"
            )));
        }
        for tag_id in &mut self.tag_ids {
            *tag_id = required_text("tagIds entry", std::mem::take(tag_id), MAX_ID_LENGTH)?;
        }
        Ok(self)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateRepoMetaInput {
    pub id: String,
    pub custom_description: Option<String>,
    pub custom_note: Option<String>,
}

impl UpdateRepoMetaInput {
    /// 空字符串表示「清除自定义内容」，统一折叠成 `None`。
    pub(super) fn validate(self) -> CommandResult<(String, Option<String>, Option<String>)> {
        let id = required_text("id", self.id, MAX_ID_LENGTH)?;
        let custom_description = optional_text(
            "customDescription",
            self.custom_description,
            MAX_DESCRIPTION_LENGTH,
        )?;
        let custom_note = optional_text("customNote", self.custom_note, MAX_NOTE_LENGTH)?;
        Ok((id, custom_description, custom_note))
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListRepoThreadsInput {
    pub repo_id: String,
    pub kind: Option<ThreadKind>,
    pub state: Option<ThreadState>,
    pub relationship: Option<ThreadRelationship>,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

impl ListRepoThreadsInput {
    pub(super) fn into_query(self) -> CommandResult<ThreadQuery> {
        let limit = self.limit.unwrap_or(DEFAULT_PAGE_SIZE);
        if limit == 0 || limit > MAX_PAGE_SIZE {
            return Err(CommandError::validation(format!(
                "limit must be between 1 and {MAX_PAGE_SIZE}"
            )));
        }
        let cursor = match self.cursor.as_deref().map(str::trim) {
            Some(raw) if !raw.is_empty() => {
                Some(CaptureCursor::parse(raw).ok_or_else(|| {
                    CommandError::validation("cursor is invalid, reload the list")
                })?)
            }
            _ => None,
        };
        Ok(ThreadQuery {
            repo_id: required_text("repoId", self.repo_id, MAX_ID_LENGTH)?,
            kind: self.kind,
            state: self.state,
            relationship: self.relationship,
            cursor,
            limit,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThreadCommentsInput {
    pub id: String,
    /// 用户点「刷新」时忽略缓存。
    pub force: Option<bool>,
}

impl ThreadCommentsInput {
    pub(super) fn validate(self) -> CommandResult<(String, bool)> {
        let id = required_text("id", self.id, MAX_ID_LENGTH)?;
        Ok((id, self.force.unwrap_or(false)))
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateTaskInput {
    pub title: String,
    pub note: Option<String>,
}

impl CreateTaskInput {
    pub(super) fn validate(self) -> CommandResult<(String, Option<String>)> {
        Ok((
            required_text("title", self.title, MAX_TASK_TITLE_LENGTH)?,
            optional_text("note", self.note, MAX_NOTE_LENGTH)?,
        ))
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateTaskInput {
    pub id: String,
    pub title: String,
    pub note: Option<String>,
}

impl UpdateTaskInput {
    pub(super) fn validate(self) -> CommandResult<(String, String, Option<String>)> {
        Ok((
            required_text("id", self.id, MAX_ID_LENGTH)?,
            required_text("title", self.title, MAX_TASK_TITLE_LENGTH)?,
            optional_text("note", self.note, MAX_NOTE_LENGTH)?,
        ))
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListTasksPageInput {
    /// `None` 表示全部。
    pub status: Option<TaskStatus>,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

impl ListTasksPageInput {
    pub(super) fn into_query(self) -> CommandResult<TaskPageQuery> {
        let limit = self.limit.unwrap_or(DEFAULT_PAGE_SIZE);
        if limit == 0 || limit > MAX_PAGE_SIZE {
            return Err(CommandError::validation(format!(
                "limit must be between 1 and {MAX_PAGE_SIZE}"
            )));
        }
        let cursor =
            match self.cursor.as_deref().map(str::trim) {
                Some(raw) if !raw.is_empty() => Some(TaskCursor::parse(raw).ok_or_else(|| {
                    CommandError::validation("cursor is invalid, reload the list")
                })?),
                _ => None,
            };
        Ok(TaskPageQuery {
            status: self.status,
            cursor,
            limit,
        })
    }
}

/// 列表筛选与「选择全部 N 条」共用同一套条件：两处各写一遍会慢慢漂移。
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InboxScopeInput {
    /// 缺省为 `pending`（待处理）；非法值按校验错误处理，不静默退回「全部」。
    pub tab: Option<String>,
    #[serde(default)]
    pub unread_only: bool,
    pub category: Option<String>,
    pub source_id: Option<String>,
}

impl InboxScopeInput {
    pub(super) fn into_scope(self) -> CommandResult<InboxScope> {
        Ok(InboxScope {
            tab: parse_inbox_tab(self.tab.as_deref())?,
            unread_only: self.unread_only,
            category: optional_text("category", self.category, MAX_CATEGORY_LENGTH)?,
            source_id: optional_text("sourceId", self.source_id, MAX_ID_LENGTH)?,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListInboxMessagesInput {
    /// 缺省为 `pending`（待处理）；非法值按校验错误处理，不静默退回「全部」。
    pub tab: Option<String>,
    #[serde(default)]
    pub unread_only: bool,
    pub category: Option<String>,
    pub source_id: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

impl ListInboxMessagesInput {
    pub(super) fn into_query(self) -> CommandResult<InboxPageQuery> {
        let limit = self.limit.unwrap_or(DEFAULT_PAGE_SIZE);
        if limit == 0 || limit > MAX_PAGE_SIZE {
            return Err(CommandError::validation(format!(
                "limit must be between 1 and {MAX_PAGE_SIZE}"
            )));
        }
        let cursor = match self.cursor.as_deref().map(str::trim) {
            Some(raw) if !raw.is_empty() => {
                Some(InboxMessageCursor::parse(raw).ok_or_else(|| {
                    CommandError::validation("cursor is invalid, reload the list")
                })?)
            }
            _ => None,
        };
        Ok(InboxPageQuery {
            scope: InboxScopeInput {
                tab: self.tab,
                unread_only: self.unread_only,
                category: self.category,
                source_id: self.source_id,
            }
            .into_scope()?,
            cursor,
            limit,
        })
    }
}

/// 计数入参只接受来源与分类：Tab 与未读不参与，四个 Tab 的数字才能相加等于「全部」。
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CountInboxMessagesInput {
    pub source_id: Option<String>,
    pub category: Option<String>,
}

impl CountInboxMessagesInput {
    pub(super) fn validate(mut self) -> CommandResult<Self> {
        self.source_id = optional_text("sourceId", self.source_id.take(), MAX_ID_LENGTH)?;
        self.category = optional_text("category", self.category.take(), MAX_CATEGORY_LENGTH)?;
        Ok(self)
    }
}

/// 动作目标：要么是显式勾选的 id，要么是「当前筛选的全部 N 条」。
#[derive(Debug)]
pub(super) enum InboxActionTarget {
    Ids(Vec<String>),
    Scope { scope: InboxScope, expected: i64 },
}

/// 动作入参。`ids` 与 `scope` 二选一：跨页「选择全部」只在后端解析出 id。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplyInboxActionInput {
    pub action: String,
    pub ids: Option<Vec<String>>,
    pub scope: Option<InboxScopeInput>,
    /// `scope` 模式必填：它既是对账依据，也是「确认 123 执行 137」的守卫。
    pub expected_count: Option<i64>,
}

impl ApplyInboxActionInput {
    pub(super) fn validate(self) -> CommandResult<(InboxAction, InboxActionTarget)> {
        let action = parse_inbox_action(&self.action)?;
        match (self.ids, self.scope) {
            (Some(ids), None) => {
                let ids = normalize_action_ids(ids)?;
                if action == InboxAction::Task && ids.len() > MAX_TASK_ACTION_IDS {
                    return Err(CommandError::validation(format!(
                        "转待办一次最多 {MAX_TASK_ACTION_IDS} 条，请分批处理"
                    )));
                }
                Ok((action, InboxActionTarget::Ids(ids)))
            }
            (None, Some(scope)) => {
                // 撤销「转待办」只是解绑，待办仍在；误转只能手删，所以不放开跨页全选。
                if action == InboxAction::Task {
                    return Err(CommandError::validation(
                        "转待办只接受勾选的消息，不能对「全部」执行",
                    ));
                }
                let expected = self.expected_count.ok_or_else(|| {
                    CommandError::validation("expectedCount is required when scope is used")
                })?;
                if expected < 0 {
                    return Err(CommandError::validation("expectedCount cannot be negative"));
                }
                Ok((
                    action,
                    InboxActionTarget::Scope {
                        scope: scope.into_scope()?,
                        expected,
                    },
                ))
            }
            _ => Err(CommandError::validation(
                "pass either ids or scope, not both",
            )),
        }
    }
}

/// 撤销入参：`ids` / `taskIds` 就是上一次动作返回的那两个列表。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UndoInboxActionInput {
    pub action: String,
    pub ids: Vec<String>,
    #[serde(default)]
    pub task_ids: Vec<String>,
}

impl UndoInboxActionInput {
    pub(super) fn validate(self) -> CommandResult<(InboxAction, Vec<String>, Vec<String>)> {
        let action = parse_inbox_action(&self.action)?;
        let ids = normalize_action_ids(self.ids)?;
        let mut task_ids = Vec::with_capacity(self.task_ids.len());
        for task_id in self.task_ids {
            task_ids.push(required_text("taskIds entry", task_id, MAX_ID_LENGTH)?);
        }
        Ok((action, ids, task_ids))
    }
}

fn parse_inbox_tab(raw: Option<&str>) -> CommandResult<InboxTab> {
    match raw.map(str::trim) {
        Some(raw) if !raw.is_empty() => raw.parse::<InboxTab>().map_err(|_| {
            CommandError::validation("tab must be one of pending, task, dismissed, all")
        }),
        _ => Ok(InboxTab::Pending),
    }
}

fn parse_inbox_action(raw: &str) -> CommandResult<InboxAction> {
    raw.trim()
        .parse::<InboxAction>()
        .map_err(|_| CommandError::validation("action must be one of read, unread, dismiss, task"))
}

/// 去空白、去重（保持顺序）后逐个校验长度：重复的 id 会让 `skipped` 莫名变大。
fn normalize_action_ids(ids: Vec<String>) -> CommandResult<Vec<String>> {
    if ids.len() > MAX_ACTION_IDS {
        return Err(CommandError::validation(format!(
            "ids cannot contain more than {MAX_ACTION_IDS} entries"
        )));
    }
    let mut normalized: Vec<String> = Vec::with_capacity(ids.len());
    for id in ids {
        let id = required_text("ids entry", id, MAX_ID_LENGTH)?;
        if !normalized.contains(&id) {
            normalized.push(id);
        }
    }
    if normalized.is_empty() {
        return Err(CommandError::validation("ids cannot be empty"));
    }
    Ok(normalized)
}

/// 开发与测试用的入库入参：字段与 `NewInboxMessage` 一一对应，`source_id` 由命令层决定。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IngestInboxMessageInput {
    pub external_id: String,
    pub external_updated_at: Option<String>,
    pub category: String,
    pub title: String,
    pub summary: Option<String>,
    pub body: Option<String>,
    pub url: Option<String>,
    pub actor: Option<String>,
    /// 缺省时由 SQL 填当前时间。
    pub occurred_at: Option<String>,
    pub related_kind: Option<String>,
    pub related_id: Option<String>,
    pub payload_json: Option<String>,
}

impl IngestInboxMessageInput {
    pub(super) fn validate(self, source_id: String) -> CommandResult<NewInboxMessage> {
        let payload_json = optional_text("payloadJson", self.payload_json, MAX_PAYLOAD_LENGTH)?;
        if let Some(payload) = payload_json.as_deref() {
            // 迁移里的 CHECK 也会拦住非法 JSON，这里提前给出可读的校验错误。
            serde_json::from_str::<Value>(payload)
                .map_err(|_| CommandError::validation("payloadJson must be valid JSON"))?;
        }
        Ok(NewInboxMessage {
            source_id,
            external_id: required_text("externalId", self.external_id, MAX_EXTERNAL_ID_LENGTH)?,
            external_updated_at: optional_text(
                "externalUpdatedAt",
                self.external_updated_at,
                MAX_TIMESTAMP_LENGTH,
            )?,
            category: required_text("category", self.category, MAX_CATEGORY_LENGTH)?,
            title: required_text("title", self.title, MAX_TASK_TITLE_LENGTH)?,
            summary: optional_text("summary", self.summary, MAX_SUMMARY_LENGTH)?,
            body: optional_text("body", self.body, MAX_BODY_LENGTH)?,
            url: optional_text("url", self.url, MAX_URL_LENGTH)?,
            actor: optional_text("actor", self.actor, MAX_ACTOR_LENGTH)?,
            occurred_at: optional_text("occurredAt", self.occurred_at, MAX_TIMESTAMP_LENGTH)?,
            related_kind: optional_text("relatedKind", self.related_kind, MAX_CATEGORY_LENGTH)?,
            related_id: optional_text("relatedId", self.related_id, MAX_ID_LENGTH)?,
            payload_json,
        })
    }
}

/// 邮箱账户入参。`password` 选填：编辑端口 / 加密方式时留空表示沿用已保存的授权码，
/// 新增账户或要换授权码时才填。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmailAccountInput {
    pub address: String,
    pub password: Option<String>,
    pub host: String,
    /// 缺省时按加密方式给 993 / 143。
    pub port: Option<u16>,
    pub tls: Option<EmailTls>,
    pub mailbox: Option<String>,
}

/// 校验后的账户表单：端口已补默认值，其余字段已去空白并按上限截断校验。
#[derive(Debug, Clone)]
pub(super) struct EmailAccountForm {
    pub address: String,
    pub password: Option<String>,
    pub host: String,
    pub port: u16,
    pub tls: EmailTls,
    pub mailbox: Option<String>,
}

impl EmailAccountInput {
    pub(super) fn validate(self) -> CommandResult<EmailAccountForm> {
        let address = required_text("address", self.address, MAX_EMAIL_ADDRESS_LENGTH)?;
        if !address.contains('@') || address.contains(char::is_whitespace) {
            return Err(CommandError::validation(
                "address must be an email address without spaces",
            ));
        }

        let host = required_text("host", self.host, MAX_EMAIL_HOST_LENGTH)?;
        // 只收主机名：带协议前缀、端口或路径的输入会让连接层拿到一个不存在的域名。
        if host.contains(char::is_whitespace)
            || host.contains("://")
            || host.contains(':')
            || host.contains('/')
        {
            return Err(CommandError::validation(
                "host must be a plain host name, without scheme or port",
            ));
        }

        let tls = self.tls.unwrap_or_default();
        let port = self.port.unwrap_or_else(|| tls.default_port());
        if port == 0 {
            return Err(CommandError::validation("port must be between 1 and 65535"));
        }

        // 授权码内部可能带空格，只去首尾空白；全空白视为没填。
        let password = optional_text("password", self.password, MAX_EMAIL_PASSWORD_LENGTH)?;
        let mailbox = optional_text("mailbox", self.mailbox, MAX_MAILBOX_LENGTH)?;
        if mailbox
            .as_deref()
            .is_some_and(|value| value.contains(char::is_control))
        {
            return Err(CommandError::validation(
                "mailbox cannot contain control characters",
            ));
        }

        Ok(EmailAccountForm {
            address,
            password,
            host,
            port,
            tls,
            mailbox,
        })
    }
}

fn optional_text(
    field: &str,
    value: Option<String>,
    max_length: usize,
) -> CommandResult<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.chars().count() > max_length {
        return Err(CommandError::validation(format!(
            "{field} cannot exceed {max_length} characters"
        )));
    }
    Ok(Some(value.to_owned()))
}

fn required_text(field: &str, value: String, max_length: usize) -> CommandResult<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(CommandError::validation(format!("{field} cannot be empty")));
    }
    if value.chars().count() > max_length {
        return Err(CommandError::validation(format!(
            "{field} cannot exceed {max_length} characters"
        )));
    }
    Ok(value.to_owned())
}
