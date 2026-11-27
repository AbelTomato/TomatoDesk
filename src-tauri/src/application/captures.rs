use std::sync::OnceLock;

use reqwest::Url;
use tauri::{AppHandle, Emitter};
use tokio::sync::Semaphore;

use crate::{
    domain::{Capture, CaptureKind, NewCapture, excerpt},
    infrastructure::{
        db::{Database, DbResult},
        link_label::{labels_from_url, labels_with_metadata},
        web_metadata::fetch_web_metadata,
    },
};

/// 后台抓取最多同时跑两个：连续收集时不应该把网络和内存一起打满。
const MAX_CONCURRENT_ENRICH: usize = 2;

/// 入库结果：记录已经可以读，链接类再带一个待补全的地址。
pub(crate) struct Captured {
    pub capture: Capture,
    pub pending_link: Option<Url>,
}

/// 随手记入库不等网页抓取：链接先用站点与路径兜底，标题与摘要交给后台补全。
pub(crate) fn capture_clipboard_content(
    database: &Database,
    value: String,
    note: Option<String>,
) -> DbResult<Captured> {
    if let Some(url) = parse_web_url(&value) {
        let stored_url = url.to_string();
        // 抓取失败时也不能留空的标题和摘要：列表里所有链接记录长得一样，
        // 用户就无法分辨。站点、路径和查询词足以在没有元数据时区分它们。
        let labels = labels_from_url(&url);
        let capture = database.captures().create(NewCapture {
            kind: CaptureKind::Link,
            source: "quick_capture".into(),
            url: Some(stored_url),
            title: Some(labels.title),
            summary: Some(labels.summary),
            note,
            body: None,
            repo_id: None,
        })?;
        return Ok(Captured {
            capture,
            pending_link: Some(url),
        });
    }

    // 笔记类：原文进 body，摘要从正文压出来，note 留给用户补充说明。
    let summary = excerpt(&value);
    let capture = database.captures().create(NewCapture {
        kind: CaptureKind::Note,
        source: "quick_capture".into(),
        url: None,
        title: None,
        summary,
        note: None,
        body: Some(value),
        repo_id: None,
    })?;
    Ok(Captured {
        capture,
        pending_link: None,
    })
}

/// 后台补全刚存下的链接：抓取成功的标题与描述覆盖兜底值，然后通知界面刷新那一行。
///
/// 抓取失败、超时、404 一律静默返回，保留入库时的兜底标签；用户已经改过标题或摘要时
/// 条件更新不生效，补全不会覆盖用户输入。
pub(crate) fn enrich_link_capture(database: Database, app: AppHandle, capture: Capture, url: Url) {
    tauri::async_runtime::spawn(async move {
        let Ok(_permit) = enrich_slots().acquire().await else {
            return;
        };
        let Some(metadata) = fetch_web_metadata(&url).await else {
            return;
        };
        let stored_url = metadata
            .canonical_url
            .clone()
            .unwrap_or_else(|| url.to_string());
        let page_url = Url::parse(&stored_url).unwrap_or(url);
        let labels = labels_with_metadata(&page_url, &metadata);

        let Ok(true) = database.captures().update_link_metadata(
            &capture.id,
            &labels,
            &stored_url,
            capture.title.as_deref(),
            capture.summary.as_deref(),
        ) else {
            return;
        };
        let _ = app.emit("capture-enriched", capture.id);
    });
}

/// 并发上限是进程级的：抓取任务由命令层按需派发，不值得为它建一个 worker。
fn enrich_slots() -> &'static Semaphore {
    static SLOTS: OnceLock<Semaphore> = OnceLock::new();
    SLOTS.get_or_init(|| Semaphore::new(MAX_CONCURRENT_ENRICH))
}

fn parse_web_url(value: &str) -> Option<Url> {
    if value.chars().any(char::is_whitespace) {
        return None;
    }

    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if !value.contains("://") && value.contains(':') {
        return None;
    }
    let candidate = if value.contains("://") {
        value.to_owned()
    } else {
        format!("https://{value}")
    };
    let url = Url::parse(&candidate).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }

    let host = url.host_str()?;
    (host.contains('.') || host.eq_ignore_ascii_case("localhost") || host.contains(':'))
        .then_some(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_host_is_stored_as_https_link() {
        assert_eq!(
            parse_web_url("example.com/posts/1").map(|url| url.to_string()),
            Some("https://example.com/posts/1".to_owned())
        );
    }

    #[test]
    fn non_web_input_is_kept_as_note() {
        assert!(parse_web_url("随手记一条想法").is_none());
        assert!(parse_web_url("mailto:someone@example.com").is_none());
        assert!(parse_web_url("example.com 后面跟空格").is_none());
    }
}
