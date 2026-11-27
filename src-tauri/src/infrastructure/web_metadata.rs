use std::{
    net::{IpAddr, Ipv4Addr},
    time::Duration,
};

use reqwest::{
    Client, Url,
    header::{ACCEPT, CONTENT_TYPE},
    redirect::Policy,
};
use scraper::{Html, Selector};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_RESPONSE_BYTES: usize = 512 * 1024;
/// 回源正文的字符上限，避免把整页导航也塞进详情面板。
const MAX_BODY_CHARS: usize = 20_000;
/// 链接摘要的兜底正文长度：只留一段，用于描述缺失时补全随手记摘要。
const MAX_EXCERPT_CHARS: usize = 200;

#[derive(Debug, Default)]
pub struct WebMetadata {
    pub title: Option<String>,
    pub description: Option<String>,
    pub canonical_url: Option<String>,
    /// 页面正文的首段纯文本；描述缺失时用它补摘要。
    pub excerpt: Option<String>,
}

pub(crate) async fn fetch_web_metadata(url: &Url) -> Option<WebMetadata> {
    let (html, final_url) = fetch_html(url).await?;
    let mut metadata = extract_metadata(&html, &final_url);
    metadata.excerpt = extract_text(&html).as_deref().and_then(text_excerpt);
    Some(metadata)
}

/// 详情按需加载用的正文回源：只取纯文本，不渲染 HTML。
pub(crate) async fn fetch_web_text(url: &Url) -> Option<String> {
    let (html, _) = fetch_html(url).await?;
    extract_text(&html)
}

async fn fetch_html(url: &Url) -> Option<(String, Url)> {
    if !is_fetchable(url) {
        return None;
    }

    let client = Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .redirect(Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 || !is_fetchable(attempt.url()) {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .user_agent("TomatoDesk/0.1.0")
        .build()
        .ok()?;

    let mut response = client
        .get(url.clone())
        .header(ACCEPT, "text/html,application/xhtml+xml")
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    if let Some(content_type) = response.headers().get(CONTENT_TYPE) {
        let content_type = content_type.to_str().ok()?.to_ascii_lowercase();
        if !content_type.contains("text/html") && !content_type.contains("application/xhtml+xml") {
            return None;
        }
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return None;
    }

    let final_url = response.url().clone();
    let mut body = Vec::new();
    loop {
        let chunk = response.chunk().await.ok()?;
        let Some(chunk) = chunk else {
            break;
        };
        if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return None;
        }
        body.extend_from_slice(&chunk);
    }

    Some((String::from_utf8_lossy(&body).into_owned(), final_url))
}

/// 按「article → main → body」的优先级抽正文，只保留块级文本。
fn extract_text(html: &str) -> Option<String> {
    let document = Html::parse_document(html);
    let block_selector = Selector::parse("h1, h2, h3, h4, p, li, blockquote, pre").ok()?;

    let mut lines: Vec<String> = Vec::new();
    for candidate in ["article", "main", "[role=\"main\"]", "body"] {
        let Ok(root_selector) = Selector::parse(candidate) else {
            continue;
        };
        let Some(root) = document.select(&root_selector).next() else {
            continue;
        };
        for element in root.select(&block_selector) {
            let collapsed = element
                .text()
                .collect::<String>()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            if !collapsed.is_empty() {
                lines.push(collapsed);
            }
        }
        if !lines.is_empty() {
            break;
        }
    }

    let text = lines.join("\n");
    if text.is_empty() {
        return None;
    }
    Some(text.chars().take(MAX_BODY_CHARS).collect())
}

/// 摘要只取正文开头一段，折叠空白后截断。
fn text_excerpt(text: &str) -> Option<String> {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }
    let mut excerpt = collapsed
        .chars()
        .take(MAX_EXCERPT_CHARS)
        .collect::<String>();
    if collapsed.chars().count() > MAX_EXCERPT_CHARS {
        excerpt.push('…');
    }
    Some(excerpt)
}

fn extract_metadata(html: &str, base_url: &Url) -> WebMetadata {
    let Ok(meta_selector) = Selector::parse("meta") else {
        return WebMetadata::default();
    };
    let Ok(title_selector) = Selector::parse("title") else {
        return WebMetadata::default();
    };
    let Ok(canonical_selector) = Selector::parse("link[rel][href]") else {
        return WebMetadata::default();
    };
    let document = Html::parse_document(html);

    let mut open_graph_title = None;
    let mut page_title = None;
    let mut open_graph_description = None;
    let mut page_description = None;

    for element in document.select(&meta_selector) {
        let content = element
            .value()
            .attr("content")
            .and_then(|value| non_empty_text(value.to_owned()));
        let Some(content) = content else {
            continue;
        };
        let property = element.value().attr("property").unwrap_or_default();
        let name = element.value().attr("name").unwrap_or_default();
        if property.eq_ignore_ascii_case("og:title") {
            open_graph_title.get_or_insert_with(|| content.clone());
        } else if property.eq_ignore_ascii_case("og:description") {
            open_graph_description.get_or_insert_with(|| content.clone());
        } else if name.eq_ignore_ascii_case("description") {
            page_description.get_or_insert(content);
        }
    }

    if let Some(element) = document.select(&title_selector).next() {
        page_title = non_empty_text(element.text().collect::<String>());
    }

    let canonical_url = document
        .select(&canonical_selector)
        .find(|element| {
            element
                .value()
                .attr("rel")
                .unwrap_or_default()
                .split_ascii_whitespace()
                .any(|relation| relation.eq_ignore_ascii_case("canonical"))
        })
        .and_then(|element| element.value().attr("href"))
        .and_then(|href| base_url.join(href.trim()).ok())
        .filter(is_http_url)
        .map(|url| url.to_string());

    WebMetadata {
        title: open_graph_title.or(page_title),
        description: open_graph_description.or(page_description),
        canonical_url,
        // 正文摘要由调用方在拿到 HTML 后统一补，元数据解析只负责标签。
        excerpt: None,
    }
}

fn non_empty_text(value: String) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.chars().take(4_096).collect())
    }
}

fn is_fetchable(url: &Url) -> bool {
    if !is_http_url(url) {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    let host = host
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(host)
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.ends_with(".internal")
    {
        return false;
    }

    if let Ok(address) = host.parse::<IpAddr>() {
        return is_public_ip(address);
    }
    host.contains('.')
}

fn is_http_url(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https") && url.host_str().is_some()
}

fn is_public_ip(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => is_public_ipv4(address),
        IpAddr::V6(address) => {
            if let Some(mapped_address) = address.to_ipv4_mapped() {
                return is_public_ipv4(mapped_address);
            }
            !(address.is_loopback()
                || address.is_unspecified()
                || address.is_unique_local()
                || address.is_unicast_link_local()
                || address.is_multicast())
        }
    }
}

fn is_public_ipv4(address: Ipv4Addr) -> bool {
    let octets = address.octets();
    let carrier_grade_nat = octets[0] == 100 && (64..=127).contains(&octets[1]);
    !(address.is_private()
        || address.is_loopback()
        || address.is_link_local()
        || address.is_broadcast()
        || address.is_unspecified()
        || address.is_multicast()
        || carrier_grade_nat)
}
