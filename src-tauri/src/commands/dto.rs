use serde::{Deserialize, de::DeserializeOwned};
use serde_json::Value;

use crate::{
    commands::error::{CommandError, CommandResult},
    domain::{
        CaptureCursor, CaptureKind, CapturePageQuery, CaptureSort, ThreadKind, ThreadQuery,
        ThreadRelationship, ThreadState,
    },
};

const MAX_ID_LENGTH: usize = 128;
const MAX_SEARCH_LENGTH: usize = 512;
const MAX_TAGS_PER_ITEM: usize = 100;
const MAX_CAPTURE_LENGTH: usize = 16_384;
const MAX_TOKEN_LENGTH: usize = 512;
const MAX_DESCRIPTION_LENGTH: usize = 4_096;
const MAX_NOTE_LENGTH: usize = 16_384;
const DEFAULT_PAGE_SIZE: u32 = 50;
const MAX_PAGE_SIZE: u32 = 200;

pub(super) fn parse_input<T: DeserializeOwned>(input: Option<Value>) -> CommandResult<T> {
    let input = input.ok_or_else(|| CommandError::validation("input is required"))?;
    serde_json::from_value(input).map_err(|_| CommandError::validation("input is invalid"))
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
