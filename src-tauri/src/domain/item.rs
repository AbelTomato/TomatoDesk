use serde::{Deserialize, Serialize};
use std::str::FromStr;

use super::cursor::KeysetCursor;

/// 条目列表沿用同一套键集游标编码，名字保留是为了不破坏既有调用点。
pub type ItemCursor = KeysetCursor;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Note,
    Link,
    Github,
}

impl ItemKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Link => "link",
            Self::Github => "github",
        }
    }
}

impl FromStr for ItemKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "note" => Ok(Self::Note),
            "link" => Ok(Self::Link),
            "github" => Ok(Self::Github),
            _ => Err(format!("unknown item kind: {value}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemStatus {
    Pending,
    Processed,
    Archived,
}

impl ItemStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Processed => "processed",
            Self::Archived => "archived",
        }
    }
}

impl FromStr for ItemStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "pending" => Ok(Self::Pending),
            "processed" => Ok(Self::Processed),
            "archived" => Ok(Self::Archived),
            _ => Err(format!("unknown item status: {value}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemSort {
    Updated,
    Created,
}

impl ItemSort {
    /// 键集分页使用的排序列，只来自枚举，不接受外部字符串。
    pub fn column(self) -> &'static str {
        match self {
            Self::Updated => "updated_at",
            Self::Created => "created_at",
        }
    }
}

/// 列表用的行投影：不含 body，保证首屏载荷只含索引字段。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemSummary {
    pub id: String,
    pub kind: ItemKind,
    pub source: String,
    pub url: Option<String>,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub note: Option<String>,
    /// 挂在某个 GitHub 仓库下时指向该仓库，普通条目为 `None`。
    pub repo_id: Option<String>,
    pub status: ItemStatus,
    pub created_at: String,
    pub updated_at: String,
}

impl From<&Item> for ItemSummary {
    fn from(item: &Item) -> Self {
        Self {
            id: item.id.clone(),
            kind: item.kind,
            source: item.source.clone(),
            url: item.url.clone(),
            title: item.title.clone(),
            summary: item.summary.clone(),
            note: item.note.clone(),
            repo_id: item.repo_id.clone(),
            status: item.status,
            created_at: item.created_at.clone(),
            updated_at: item.updated_at.clone(),
        }
    }
}

/// 列表查询：筛选、排序与键集游标全部下沉到 SQL。
#[derive(Debug, Clone)]
pub struct ItemPageQuery {
    pub status: Option<ItemStatus>,
    pub kind: Option<ItemKind>,
    /// 只看挂在某个仓库下的个人条目。
    pub repo_id: Option<String>,
    pub sort: ItemSort,
    pub cursor: Option<ItemCursor>,
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemPage {
    pub items: Vec<ItemSummary>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemStatusCounts {
    pub all: i64,
    pub pending: i64,
    pub processed: i64,
    pub archived: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemBody {
    pub id: String,
    pub kind: ItemKind,
    pub url: Option<String>,
    pub body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub kind: ItemKind,
    pub source: String,
    pub url: Option<String>,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub note: Option<String>,
    pub body: Option<String>,
    pub repo_id: Option<String>,
    pub status: ItemStatus,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewItem {
    pub kind: ItemKind,
    pub source: String,
    pub url: Option<String>,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub note: Option<String>,
    pub body: Option<String>,
    pub repo_id: Option<String>,
}

/// 列表摘要统一截断长度，与迁移里的 `substr(summary, 1, 200)` 保持一致。
pub const SUMMARY_MAX_CHARS: usize = 200;

/// 把正文压成一行短摘要：折叠空白后截断，超出部分用省略号收尾。
pub fn excerpt(text: &str) -> Option<String> {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }
    let mut excerpt = collapsed
        .chars()
        .take(SUMMARY_MAX_CHARS)
        .collect::<String>();
    if collapsed.chars().count() > SUMMARY_MAX_CHARS {
        excerpt.push('…');
    }
    Some(excerpt)
}
