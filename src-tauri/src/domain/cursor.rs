use serde::{Deserialize, Serialize};

/// 键集游标：按「排序值 + 行 id」定位下一页，避免 offset 分页的漂移。
///
/// 对前端是不透明字符串 `排序值|id`；解析失败按非法输入处理。
/// 条目列表与仓库线程列表共用同一套编码，排序值分别是 `updated_at` / `created_at`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeysetCursor {
    pub value: String,
    pub id: String,
}

impl KeysetCursor {
    pub fn new(value: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            id: id.into(),
        }
    }

    pub fn encode(&self) -> String {
        format!("{}|{}", self.value, self.id)
    }

    pub fn parse(raw: &str) -> Option<Self> {
        let (value, id) = raw.split_once('|')?;
        let (value, id) = (value.trim(), id.trim());
        if value.is_empty() || id.is_empty() {
            return None;
        }
        Some(Self {
            value: value.to_owned(),
            id: id.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::KeysetCursor;

    #[test]
    fn cursors_round_trip_and_reject_malformed_values() {
        let cursor = KeysetCursor::new("2026-11-25T10:00:00.000Z", "abc");
        assert_eq!(
            KeysetCursor::parse(&cursor.encode()),
            Some(cursor),
            "encode and parse should be inverse"
        );
        assert_eq!(KeysetCursor::parse("missing-separator"), None);
        assert_eq!(KeysetCursor::parse("|abc"), None);
        assert_eq!(KeysetCursor::parse("2026-11-25T10:00:00.000Z|"), None);
    }
}
