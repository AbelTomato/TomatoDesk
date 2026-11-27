//! 邮件入库前的字段映射：把解析结果变成 `NewInboxMessage`。
//!
//! 这里只做「呈现」层面的决定（分类、摘要、占位、payload），不碰 IMAP 与数据库。

use chrono::{SecondsFormat, Utc};

use crate::{
    domain::NewInboxMessage,
    infrastructure::email::parse::{
        MAX_EXTERNAL_ID_CHARS, MAX_PAYLOAD_BYTES, NO_SUBJECT, ParsedMail,
    },
};

/// 邮件在收件箱里的固定类型，与迁移里的注释一致。
pub(crate) const MAIL_CATEGORY: &str = "mail";

/// 摘要长度，与随手记保持一致的 200 字。
pub(crate) const SUMMARY_CHARS: usize = 200;

/// 正文为空时占位文案里最多列几个附件名。
const PLACEHOLDER_NAMES: usize = 3;

/// 日期超前当前时间超过一天就当成时钟错误，避免把邮件顶到列表最前面。
const MAX_CLOCK_SKEW_SECONDS: i64 = 24 * 60 * 60;

/// 组装入库记录。
///
/// `now` 由调用方注入（RFC3339、UTC 毫秒），保证测试可复现；`Date` 缺失或不可信时
/// `occurred_at` 留空，由 SQL 填收到时间。注意 `external_updated_at` 与它保持一致：
/// 如果用「当前时间」，每轮同步都会变大，`ingest` 会把已读的邮件重新标成未读。
// S3 的同步循环接入后由 sync_account 调用。
#[allow(dead_code)]
pub(crate) fn to_new_message(source_id: &str, parsed: &ParsedMail, now: &str) -> NewInboxMessage {
    let occurred_at = parsed
        .date_unix
        .filter(|seconds| is_plausible(*seconds, now))
        .and_then(iso_from_unix);

    let body = parsed
        .body
        .clone()
        .or_else(|| attachment_placeholder(&parsed.attachment_names));
    let summary = body.as_deref().map(summarize);

    NewInboxMessage {
        source_id: source_id.to_owned(),
        external_id: external_id(parsed),
        external_updated_at: occurred_at.clone(),
        category: MAIL_CATEGORY.to_owned(),
        title: title(parsed),
        summary,
        body,
        // 不造 `mailto:`，避免点击后跳到外部邮件客户端。
        url: None,
        actor: parsed.from.clone(),
        occurred_at,
        related_kind: None,
        related_id: None,
        payload_json: Some(payload_json(parsed)),
    }
}

fn title(parsed: &ParsedMail) -> String {
    if parsed.subject.trim().is_empty() {
        NO_SUBJECT.to_owned()
    } else {
        parsed.subject.clone()
    }
}

/// `Message-ID` 是首选去重键；缺失或超长时退化成 `uid:<uidValidity>:<uid>`，
/// 仍在同一账户内稳定，重复同步不会产生副本。
fn external_id(parsed: &ParsedMail) -> String {
    let usable = parsed
        .message_id
        .as_deref()
        .filter(|id| !id.is_empty() && id.chars().count() <= MAX_EXTERNAL_ID_CHARS);
    match usable {
        Some(id) => id.to_owned(),
        None => format!("uid:{}:{}", parsed.server.uid_validity, parsed.server.uid),
    }
}

fn is_plausible(seconds: i64, now: &str) -> bool {
    if seconds <= 0 {
        return false;
    }
    let now = chrono::DateTime::parse_from_rfc3339(now)
        .map(|moment| moment.timestamp())
        .unwrap_or_else(|_| Utc::now().timestamp());
    seconds <= now.saturating_add(MAX_CLOCK_SKEW_SECONDS)
}

fn iso_from_unix(seconds: i64) -> Option<String> {
    chrono::DateTime::from_timestamp(seconds, 0)
        .map(|moment| moment.to_rfc3339_opts(SecondsFormat::Millis, true))
}

/// 正文为空但有附件时给一句占位，否则收件箱里会出现「有标题、无内容」的空条目。
fn attachment_placeholder(names: &[String]) -> Option<String> {
    if names.is_empty() {
        return None;
    }
    let shown: Vec<&str> = names
        .iter()
        .take(PLACEHOLDER_NAMES)
        .map(String::as_str)
        .collect();
    let mut text = format!("（无正文，含 {} 个附件：{}", names.len(), shown.join("、"));
    if names.len() > shown.len() {
        text.push_str(" 等");
    }
    text.push('）');
    Some(text)
}

/// 摘要取正文前 200 字，空行直接跳过，让预览是一句连贯的话。
fn summarize(body: &str) -> String {
    let mut summary = String::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if !summary.is_empty() {
            summary.push(' ');
        }
        summary.push_str(line);
        if summary.chars().count() >= SUMMARY_CHARS {
            break;
        }
    }
    summary.chars().take(SUMMARY_CHARS).collect()
}

/// 轻量元数据，不存原始 MIME，也不含任何凭据。
fn payload_json(parsed: &ParsedMail) -> String {
    let build = |with_recipients: bool| {
        let mut value = serde_json::json!({
            "uid": parsed.server.uid,
            "uidValidity": parsed.server.uid_validity,
            "messageId": parsed.message_id,
            "from": parsed.from_address,
            "size": parsed.server.size_bytes,
        });
        if with_recipients {
            value["to"] = serde_json::json!(parsed.to);
            value["cc"] = serde_json::json!(parsed.cc);
            value["attachments"] = serde_json::json!(parsed.attachment_names);
        }
        value.to_string()
    };

    let full = build(true);
    if full.len() <= MAX_PAYLOAD_BYTES {
        full
    } else {
        // 群发邮件可能带很多收件人，超预算就只留能定位这封信的字段。
        build(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::email::parse::{MailServerMeta, parse_message};

    const NOW: &str = "2026-11-26T15:00:00.000Z";

    fn parse(raw: &[u8]) -> ParsedMail {
        parse_message(raw)
    }

    #[test]
    fn maps_headers_into_an_inbox_message() {
        let parsed = parse(
            b"From: Ann Lee <ann@example.com>\r\n\
              Subject: Quarterly numbers\r\n\
              Date: Thu, 26 Nov 2026 10:00:00 +0800\r\n\
              Message-ID: <m1@example.com>\r\n\
              Content-Type: text/plain; charset=utf-8\r\n\
              \r\n\
              Numbers are attached.\r\n",
        );
        let message = to_new_message("source-email-x", &parsed, NOW);

        assert_eq!(message.category, MAIL_CATEGORY);
        assert_eq!(message.external_id, "m1@example.com");
        assert_eq!(message.title, "Quarterly numbers");
        assert_eq!(message.actor.as_deref(), Some("Ann Lee <ann@example.com>"));
        // +0800 的 10:00 是 UTC 02:00。
        assert_eq!(
            message.occurred_at.as_deref(),
            Some("2026-11-26T02:00:00.000Z")
        );
        assert_eq!(message.external_updated_at, message.occurred_at);
        assert_eq!(message.url, None);
        assert_eq!(message.related_kind, None);
        assert!(
            message
                .body
                .as_deref()
                .unwrap_or("")
                .contains("Numbers are attached")
        );
        assert!(message.summary.as_deref().unwrap_or("").contains("Numbers"));
    }

    #[test]
    fn missing_message_id_falls_back_to_uid() {
        let mut parsed = parse(b"Subject: No id\r\n\r\nbody\r\n");
        parsed.server = MailServerMeta {
            uid: 42,
            uid_validity: 7,
            size_bytes: 120,
        };
        let message = to_new_message("source-email-x", &parsed, NOW);
        assert_eq!(message.external_id, "uid:7:42");
    }

    #[test]
    fn future_date_is_rejected_instead_of_reordering_the_list() {
        let parsed = parse(
            b"Subject: From the future\r\nDate: Fri, 01 Jan 2027 00:00:00 +0000\r\n\r\nbody\r\n",
        );
        let message = to_new_message("source-email-x", &parsed, NOW);
        // 留空交给 SQL 填收到时间，`external_updated_at` 也不跟风变，避免把已读翻回未读。
        assert_eq!(message.occurred_at, None);
        assert_eq!(message.external_updated_at, None);
    }

    #[test]
    fn empty_subject_gets_a_placeholder_title() {
        let parsed = ParsedMail::default();
        let message = to_new_message("source-email-x", &parsed, NOW);
        assert_eq!(message.title, NO_SUBJECT);
    }

    #[test]
    fn attachments_replace_an_empty_body() {
        let mut parsed = ParsedMail {
            subject: "Only files".into(),
            ..ParsedMail::default()
        };
        parsed.attachment_names = vec!["a.pdf".into(), "b.png".into()];
        let message = to_new_message("source-email-x", &parsed, NOW);
        assert_eq!(
            message.body.as_deref(),
            Some("（无正文，含 2 个附件：a.pdf、b.png）")
        );
        assert_eq!(message.summary, message.body);
    }

    #[test]
    fn placeholder_lists_at_most_three_names() {
        let parsed = ParsedMail {
            attachment_names: vec![
                "a.pdf".into(),
                "b.png".into(),
                "c.zip".into(),
                "d.xlsx".into(),
            ],
            ..ParsedMail::default()
        };
        let body = to_new_message("source-email-x", &parsed, NOW)
            .body
            .expect("有附件应有占位正文");
        assert!(body.contains("含 4 个附件"));
        assert!(body.contains("c.zip"));
        assert!(!body.contains("d.xlsx"));
        assert!(body.ends_with("等）"));
    }

    #[test]
    fn summary_is_capped_at_two_hundred_characters() {
        let parsed = ParsedMail {
            subject: "Long".into(),
            body: Some("字".repeat(500)),
            ..ParsedMail::default()
        };
        let summary = to_new_message("source-email-x", &parsed, NOW)
            .summary
            .expect("有正文应有摘要");
        assert_eq!(summary.chars().count(), SUMMARY_CHARS);
    }

    #[test]
    fn payload_stays_small_and_carries_no_credentials() {
        let mut parsed = ParsedMail {
            subject: "Metadata".into(),
            from: Some("Ann <ann@example.com>".into()),
            from_address: Some("ann@example.com".into()),
            message_id: Some("m9@example.com".into()),
            body: Some("body".into()),
            ..ParsedMail::default()
        };
        parsed.to = (0..300)
            .map(|index| format!("user{index}@example.com"))
            .collect();
        parsed.server = MailServerMeta {
            uid: 5,
            uid_validity: 1,
            size_bytes: 2048,
        };

        let payload = to_new_message("source-email-x", &parsed, NOW)
            .payload_json
            .expect("payload 总是存在");
        assert!(
            payload.len() <= MAX_PAYLOAD_BYTES,
            "payload 超预算：{payload}"
        );
        assert!(payload.contains("\"uidValidity\":1"));
        assert!(payload.contains("\"messageId\":\"m9@example.com\""));
        assert!(!payload.contains("password"));
    }
}
