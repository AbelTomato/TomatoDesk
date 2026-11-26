use serde::{Deserialize, Serialize};
use std::str::FromStr;

use super::cursor::KeysetCursor;

/// 随手记列表沿用同一套键集游标编码。
pub type CaptureCursor = KeysetCursor;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureKind {
    Note,
    Link,
}

impl CaptureKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Link => "link",
        }
    }
}

impl FromStr for CaptureKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "note" => Ok(Self::Note),
            "link" => Ok(Self::Link),
            // 0003 已清空 GitHub 条目，但旧库的 CHECK 仍保留该取值。
            "github" => Err("github items are no longer supported".into()),
            _ => Err(format!("unknown capture kind: {value}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureSort {
    Updated,
    Created,
}

impl CaptureSort {
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
pub struct CaptureSummary {
    pub id: String,
    pub kind: CaptureKind,
    pub source: String,
    pub url: Option<String>,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub note: Option<String>,
    /// 挂在某个 GitHub 仓库下时指向该仓库，普通随手记为 `None`。
    pub repo_id: Option<String>,
    /// 归档时间；`None` 表示未归档。
    pub archived_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<&Capture> for CaptureSummary {
    fn from(capture: &Capture) -> Self {
        Self {
            id: capture.id.clone(),
            kind: capture.kind,
            source: capture.source.clone(),
            url: capture.url.clone(),
            title: capture.title.clone(),
            summary: capture.summary.clone(),
            note: capture.note.clone(),
            repo_id: capture.repo_id.clone(),
            archived_at: capture.archived_at.clone(),
            created_at: capture.created_at.clone(),
            updated_at: capture.updated_at.clone(),
        }
    }
}

/// 列表查询：归档筛选、类型、仓库、排序与键集游标全部下沉到 SQL。
#[derive(Debug, Clone)]
pub struct CapturePageQuery {
    /// `None` 表示全部；`Some(false)` 只看未归档；`Some(true)` 只看已归档。
    pub archived: Option<bool>,
    pub kind: Option<CaptureKind>,
    /// 只看挂在某个仓库下的随手记。
    pub repo_id: Option<String>,
    pub sort: CaptureSort,
    pub cursor: Option<CaptureCursor>,
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapturePage {
    pub captures: Vec<CaptureSummary>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureCounts {
    pub all: i64,
    pub active: i64,
    pub archived: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureBody {
    pub id: String,
    pub kind: CaptureKind,
    pub url: Option<String>,
    pub body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capture {
    pub id: String,
    pub kind: CaptureKind,
    pub source: String,
    pub url: Option<String>,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub note: Option<String>,
    pub body: Option<String>,
    pub repo_id: Option<String>,
    pub archived_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewCapture {
    pub kind: CaptureKind,
    pub source: String,
    pub url: Option<String>,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub note: Option<String>,
    pub body: Option<String>,
    pub repo_id: Option<String>,
}

/// 列表摘要统一截断长度，与迁移里的 `substr(..., 1, 200)` 保持一致。
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
