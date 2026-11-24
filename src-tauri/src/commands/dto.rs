use serde::{Deserialize, de::DeserializeOwned};
use serde_json::Value;

use crate::{
    commands::error::{CommandError, CommandResult},
    domain::{ItemKind, ItemStatus},
};

const MAX_ID_LENGTH: usize = 128;
const MAX_SEARCH_LENGTH: usize = 512;
const MAX_TAGS_PER_ITEM: usize = 100;
const MAX_CAPTURE_LENGTH: usize = 16_384;
const MAX_TOKEN_LENGTH: usize = 512;

pub(super) fn parse_input<T: DeserializeOwned>(input: Option<Value>) -> CommandResult<T> {
    let input = input.ok_or_else(|| CommandError::validation("input is required"))?;
    serde_json::from_value(input).map_err(|_| CommandError::validation("input is invalid"))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateItemInput {
    pub kind: ItemKind,
    pub source: String,
    pub url: Option<String>,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub note: Option<String>,
}

impl CreateItemInput {
    pub(super) fn validate(mut self) -> CommandResult<Self> {
        self.source = required_text("source", self.source, 4096)?;
        Ok(self)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptureItemInput {
    pub value: String,
    pub note: Option<String>,
}

impl CaptureItemInput {
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
pub struct UpdateItemInput {
    pub id: String,
    pub kind: ItemKind,
    pub source: String,
    pub url: Option<String>,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub note: Option<String>,
    pub status: ItemStatus,
}

impl UpdateItemInput {
    pub(super) fn validate(mut self) -> CommandResult<Self> {
        self.id = required_text("id", self.id, MAX_ID_LENGTH)?;
        self.source = required_text("source", self.source, 4096)?;
        Ok(self)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ItemIdInput {
    pub id: String,
}

impl ItemIdInput {
    pub(super) fn validate(mut self) -> CommandResult<Self> {
        self.id = required_text("id", self.id, MAX_ID_LENGTH)?;
        Ok(self)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchItemsInput {
    pub query: String,
}

impl SearchItemsInput {
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
pub struct SetItemTagsInput {
    pub item_id: String,
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

impl SetItemTagsInput {
    pub(super) fn validate(mut self) -> CommandResult<Self> {
        self.item_id = required_text("itemId", self.item_id, MAX_ID_LENGTH)?;
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
