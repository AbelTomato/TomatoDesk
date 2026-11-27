//! 链接标签推导：只用 URL（可选叠加页面元数据）生成可读的标题与摘要。
//!
//! 写入随手记时先用 `labels_from_url` 把记录写得可区分，抓取成功后再用
//! `labels_with_metadata` 补全。两处共用同一套规则，避免写入与回填漂移。

use reqwest::Url;

use super::web_metadata::WebMetadata;

/// 列表只显示一行，标题和摘要都不需要太长。
const MAX_LINK_TITLE_CHARS: usize = 160;
const MAX_LINK_SUMMARY_CHARS: usize = 240;
/// 路径最多取最后三段，查询参数最多取三个非跟踪字段。
const MAX_PATH_SEGMENTS: usize = 3;
const MAX_QUERY_FIELDS: usize = 3;

/// 链接记录的标题与摘要，两者都保证非空。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkLabels {
    pub title: String,
    pub summary: String,
}

/// 只有 URL 时能推导出的标签：站点、路径与查询词。
///
/// 抓取失败或还没抓完时也要有可辨认的标题，否则列表里所有链接长得一样。
pub fn labels_from_url(url: &Url) -> LinkLabels {
    labels(url, None)
}

/// 抓到页面元数据后的标签：标题与描述优先用页面内容，缺什么补什么。
pub fn labels_with_metadata(url: &Url, metadata: &WebMetadata) -> LinkLabels {
    labels(url, Some(metadata))
}

fn labels(url: &Url, metadata: Option<&WebMetadata>) -> LinkLabels {
    let site = site_label(url);
    let location = location_hint(url);

    // 标题取最后一段路径：`posts / hello world` 里真正有辨识度的是 `hello world`，
    // 查询串（`id=42`）对标题是噪音，只留在摘要里。
    let title = metadata
        .and_then(|metadata| metadata.title.as_deref())
        .and_then(clean_text)
        .unwrap_or_else(|| match title_hint(url) {
            Some(hint) => format!("{hint} · {site}"),
            None => site.clone(),
        });

    let detail = metadata.and_then(|metadata| {
        metadata
            .description
            .as_deref()
            .and_then(clean_text)
            .or_else(|| metadata.excerpt.as_deref().and_then(clean_text))
    });

    let mut summary = site;
    if let Some(location) = location.as_deref() {
        summary.push_str(" · ");
        summary.push_str(location);
    }
    if let Some(detail) = detail.as_deref() {
        summary.push_str(" — ");
        summary.push_str(detail);
    }

    LinkLabels {
        title: truncate_chars(&title, MAX_LINK_TITLE_CHARS),
        summary: truncate_chars(&summary, MAX_LINK_SUMMARY_CHARS),
    }
}

/// 站点名去掉 `www.`，保留域名本身，作为链接的稳定来源标识。
fn site_label(url: &Url) -> String {
    let host = url.host_str().unwrap_or_default().trim_end_matches('.');
    let host = host.strip_prefix("www.").unwrap_or(host);
    if host.is_empty() {
        "网页链接".to_owned()
    } else {
        host.to_ascii_lowercase()
    }
}

/// 路径与查询词：`/posts/hello-world?id=42` → `posts / hello world · id=42`。
fn location_hint(url: &Url) -> Option<String> {
    let path = path_labels(url).join(" / ");
    match (path.is_empty(), query_hint(url)) {
        (true, None) => None,
        (true, Some(query)) => Some(query),
        (false, None) => Some(path),
        (false, Some(query)) => Some(format!("{path} · {query}")),
    }
}

/// 标题只用最后一段路径：更短的层级（`posts`、`blog`）区分不了文章。
fn title_hint(url: &Url) -> Option<String> {
    path_labels(url).pop()
}

/// 路径最后三段的可读形式：百分号解码，`-` / `_` 当空格。
fn path_labels(url: &Url) -> Vec<String> {
    let segments: Vec<&str> = url
        .path_segments()
        .map(|segments| segments.filter(|segment| !segment.is_empty()).collect())
        .unwrap_or_default();
    let start = segments.len().saturating_sub(MAX_PATH_SEGMENTS);
    segments[start..]
        .iter()
        .filter_map(|segment| {
            let decoded = percent_decode(segment);
            let readable = decoded.replace(['-', '_'], " ");
            clean_text(&readable)
        })
        .collect()
}

fn query_hint(url: &Url) -> Option<String> {
    let fields: Vec<String> = url
        .query_pairs()
        .filter(|(key, value)| !value.trim().is_empty() && !is_tracking_key(key))
        .take(MAX_QUERY_FIELDS)
        .map(|(key, value)| format!("{}={}", key.trim(), value.trim()))
        .collect();
    (!fields.is_empty()).then(|| fields.join(" "))
}

/// 跟踪参数不进摘要，否则同一篇文章的不同分享链接会看起来不一样。
fn is_tracking_key(key: &str) -> bool {
    let key = key.trim().to_ascii_lowercase();
    key.starts_with("utm_")
        || matches!(
            key.as_str(),
            "fbclid" | "gclid" | "spm" | "ref" | "referrer" | "source" | "share"
        )
}

fn clean_text(value: &str) -> Option<String> {
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    (!collapsed.is_empty()).then_some(collapsed)
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let mut truncated = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        truncated.push('…');
    }
    truncated
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
        {
            decoded.push(high * 16 + low);
            index += 3;
            continue;
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata(
        title: Option<&str>,
        description: Option<&str>,
        excerpt: Option<&str>,
    ) -> WebMetadata {
        WebMetadata {
            title: title.map(str::to_owned),
            description: description.map(str::to_owned),
            canonical_url: None,
            excerpt: excerpt.map(str::to_owned),
        }
    }

    fn url(value: &str) -> Url {
        Url::parse(value).unwrap()
    }

    #[test]
    fn url_only_keeps_site_and_path() {
        let labels = labels_from_url(&url(
            "https://example.com/posts/hello-world?id=42&utm_source=newsletter",
        ));

        assert_eq!(labels.title, "hello world · example.com");
        assert!(labels.summary.contains("example.com"));
        assert!(labels.summary.contains("posts / hello world"));
        assert!(labels.summary.contains("id=42"));
        assert!(!labels.summary.contains("utm_source"));
    }

    #[test]
    fn metadata_prefers_title_and_description() {
        let labels = labels_with_metadata(
            &url("https://github.com/rust-lang/book"),
            &metadata(
                Some("The Rust Programming Language"),
                Some("A book about Rust."),
                None,
            ),
        );

        assert_eq!(labels.title, "The Rust Programming Language");
        assert!(labels.summary.contains("github.com"));
        assert!(labels.summary.contains("rust lang / book"));
        assert!(labels.summary.ends_with("A book about Rust."));
    }

    #[test]
    fn root_url_uses_site_only() {
        let labels = labels_from_url(&url("https://www.example.com/"));

        assert_eq!(labels.title, "example.com");
        assert_eq!(labels.summary, "example.com");
    }

    #[test]
    fn metadata_falls_back_to_page_excerpt() {
        let labels = labels_with_metadata(
            &url("https://example.com/a"),
            &metadata(Some("标题"), None, Some("正文第一段")),
        );

        assert!(labels.summary.contains("正文第一段"));
    }

    #[test]
    fn percent_encoded_path_is_decoded() {
        let labels = labels_from_url(&url(
            "https://example.com/%E4%B8%AD%E6%96%87%E6%A0%87%E9%A2%98",
        ));

        assert_eq!(labels.title, "中文标题 · example.com");
    }

    /// 元数据只有空白标题时仍应退回 URL 推导，不能写出空标题。
    #[test]
    fn blank_metadata_falls_back_to_url_labels() {
        let labels = labels_with_metadata(
            &url("https://example.com/posts/hello"),
            &metadata(Some("   "), Some(""), None),
        );

        assert_eq!(labels.title, "hello · example.com");
        assert_eq!(labels.summary, "example.com · posts / hello");
    }
}
