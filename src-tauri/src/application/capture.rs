use reqwest::Url;

use crate::{
    domain::{Item, ItemKind, NewItem},
    infrastructure::{
        db::{Database, DbResult},
        web_metadata::fetch_web_metadata,
    },
};

pub(crate) async fn capture_clipboard_content(
    database: &Database,
    value: String,
    note: Option<String>,
) -> DbResult<Item> {
    if let Some(url) = parse_web_url(&value) {
        let metadata = fetch_web_metadata(&url).await.unwrap_or_default();
        let stored_url = metadata.canonical_url.unwrap_or_else(|| url.to_string());
        return database.items().create(NewItem {
            kind: ItemKind::Link,
            source: "clipboard".into(),
            url: Some(stored_url),
            title: metadata.title,
            summary: metadata.description,
            note,
        });
    }

    database.items().create(NewItem {
        kind: ItemKind::Note,
        source: "clipboard".into(),
        url: None,
        title: None,
        summary: None,
        note: Some(value),
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
