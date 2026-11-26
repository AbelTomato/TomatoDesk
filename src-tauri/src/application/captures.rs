use reqwest::Url;

use crate::{
    domain::{Capture, CaptureKind, NewCapture, excerpt},
    infrastructure::{
        db::{Database, DbResult},
        web_metadata::fetch_web_metadata,
    },
};

pub(crate) async fn capture_clipboard_content(
    database: &Database,
    value: String,
    note: Option<String>,
) -> DbResult<Capture> {
    if let Some(url) = parse_web_url(&value) {
        let metadata = fetch_web_metadata(&url).await.unwrap_or_default();
        let stored_url = metadata.canonical_url.unwrap_or_else(|| url.to_string());
        return database.captures().create(NewCapture {
            kind: CaptureKind::Link,
            source: "quick_capture".into(),
            url: Some(stored_url),
            title: metadata.title,
            summary: metadata.description,
            note,
            body: None,
            repo_id: None,
        });
    }

    // 笔记类：原文进 body，摘要从正文压出来，note 留给用户补充说明。
    let summary = excerpt(&value);
    database.captures().create(NewCapture {
        kind: CaptureKind::Note,
        source: "quick_capture".into(),
        url: None,
        title: None,
        summary,
        note: None,
        body: Some(value),
        repo_id: None,
    })
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
