use serde::{Deserialize, Serialize};
use std::str::FromStr;

use super::cursor::KeysetCursor;

/// 待办列表沿用同一套键集游标编码。
pub type TaskCursor = KeysetCursor;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Open,
    Done,
    Cancelled,
}

impl TaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
        }
    }
}

impl FromStr for TaskStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "open" => Ok(Self::Open),
            "done" => Ok(Self::Done),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(format!("unknown task status: {value}")),
        }
    }
}

/// 待办只有短文本，列表与详情共用同一个投影，不再拆 summary 类型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub title: String,
    pub note: Option<String>,
    pub status: TaskStatus,
    /// 来源：`manual` / `inbox` / `capture`；手动创建为 `manual`。
    pub origin_kind: Option<String>,
    /// 来源侧的 id，弱关联，不建外键。
    pub origin_id: Option<String>,
    /// 来源消息所属的消息源类型（`github` / `manual` …）；非收件箱来源为空。
    /// 列表行用它把「来自收件箱」细化为「来自 GitHub 通知」，不必再查一次消息。
    pub origin_source_kind: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// 完成时间；取消或重开后回到 `None`。
    pub completed_at: Option<String>,
}

/// 列表查询：状态筛选与键集游标全部下沉到 SQL。
#[derive(Debug, Clone)]
pub struct TaskPageQuery {
    /// `None` 表示全部。
    pub status: Option<TaskStatus>,
    pub cursor: Option<TaskCursor>,
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskPage {
    pub tasks: Vec<Task>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskCounts {
    pub open: i64,
    pub done: i64,
    pub cancelled: i64,
    pub all: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTask {
    pub title: String,
    pub note: Option<String>,
    pub origin_kind: Option<String>,
    pub origin_id: Option<String>,
}
