//! 邮件原文解析：把 RFC 822 的原始字节变成可入库的字段。
//!
//! 全部是纯函数，不触网、不写数据库，方便用 `.eml` fixtures 覆盖编码与结构边界。
//! `mail-parser` 已经负责 RFC 2047 解码与 `full_encoding` 下的 GBK / GB18030 / Big5，
//! 这里只做三件事：挑选字段、清洗控制字符、按上限截断。

use mail_parser::{AddressList, Mailbox, MessageParser, MessageRef, html_to_text};

/// 单封邮件交给解析器的字节上限。正文只取开头，超出部分截断；
/// 截断可能切断某个 MIME 分段的尾部，但主题、发件人与正文开头都在前面。
pub const MAX_RAW_BYTES: usize = 256 * 1024;

/// 正文入库上限（字符数）。
pub const MAX_BODY_CHARS: usize = 64 * 1024;
/// 标题上限（字符数）。
pub const MAX_TITLE_CHARS: usize = 512;
/// 发件人展示上限（字符数）。
pub const MAX_ACTOR_CHARS: usize = 256;
/// `Message-ID` 上限，超过就退化成 uid 兜底。
pub const MAX_EXTERNAL_ID_CHARS: usize = 512;
/// 单个附件名上限。
pub const MAX_ATTACHMENT_NAME_CHARS: usize = 256;
/// `payload_json` 的体积上限，超出就丢掉收件人列表。
pub const MAX_PAYLOAD_BYTES: usize = 4 * 1024;
/// 收件人最多保留多少个，避免群发邮件把 `payload_json` 撑大。
pub const MAX_ADDRESSES: usize = 20;

/// 无主题时的占位，保证 `title` 非空（入库有非空约束）。
pub const NO_SUBJECT: &str = "(无主题)";

/// 服务器侧元数据，来自 `FETCH`，由调用方填入；不参与 MIME 解析。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MailServerMeta {
    pub uid: u32,
    pub uid_validity: u32,
    /// 服务器上的字节数（未截断前的原始长度）。
    pub size_bytes: u32,
}

/// 解析结果：只保留入库需要的字段，不保留原始 MIME。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedMail {
    /// 已清洗并截断；无主题时为 [`NO_SUBJECT`]。
    pub subject: String,
    /// `显示名 <地址>`；无显示名时只有地址。
    pub from: Option<String>,
    /// 发件人地址，单独保留给 `payload_json`。
    pub from_address: Option<String>,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    /// `Date` 头对应的 Unix 秒。缺失或非法时为 `None`；
    /// 调用方可以在 `Date` 缺失时用 `INTERNALDATE` 填入，保持每封信的取值稳定。
    pub date_unix: Option<i64>,
    /// 已去掉尖括号的 `Message-ID`；缺失或超长时为 `None`。
    pub message_id: Option<String>,
    /// 优先 `text/plain`，只有 HTML 时转成纯文本；空正文为 `None`。
    pub body: Option<String>,
    pub attachment_names: Vec<String>,
    pub server: MailServerMeta,
}

/// 解析一封原始邮件。结构损坏也返回可入库的结果，不抛错：
/// 单封邮件解析失败不应该中断整轮同步。
pub fn parse_message(raw: &[u8]) -> ParsedMail {
    let size_bytes = u32::try_from(raw.len()).unwrap_or(u32::MAX);
    let raw = if raw.len() > MAX_RAW_BYTES {
        &raw[..MAX_RAW_BYTES]
    } else {
        raw
    };

    let Some(parsed) = MessageParser::new().parse(raw) else {
        // 连头部都解析不出来：保留大小信息，其余留空，交给上层按「无主题无正文」处理。
        return ParsedMail {
            server: MailServerMeta {
                size_bytes,
                ..MailServerMeta::default()
            },
            ..ParsedMail::default()
        };
    };
    let message = parsed.root();

    let subject = {
        let cleaned = message.subject().map(clean_inline).unwrap_or_default();
        if cleaned.is_empty() {
            NO_SUBJECT.to_owned()
        } else {
            truncate_chars(&cleaned, MAX_TITLE_CHARS)
        }
    };

    let from_mailbox = message.from().and_then(|list| list.first());
    let from_address = from_mailbox
        .as_ref()
        .and_then(Mailbox::address)
        .map(clean_inline)
        .filter(|value| !value.is_empty());
    let from = from_mailbox
        .as_ref()
        .and_then(format_mailbox)
        .map(|value| truncate_chars(&value, MAX_ACTOR_CHARS));

    let attachment_names = collect_attachments(&message);

    ParsedMail {
        subject,
        from,
        from_address,
        to: collect_addresses(message.to()),
        cc: collect_addresses(message.cc()),
        date_unix: message
            .date()
            .filter(|date| date.is_valid())
            .map(|date| date.to_timestamp())
            .filter(|seconds| *seconds > 0),
        message_id: message
            .message_id()
            .map(clean_inline)
            .map(|value| value.trim_matches(|ch| ch == '<' || ch == '>').to_owned())
            .filter(|value| !value.is_empty() && value.chars().count() <= MAX_EXTERNAL_ID_CHARS),
        body: pick_body(&message),
        attachment_names,
        server: MailServerMeta {
            size_bytes,
            ..MailServerMeta::default()
        },
    }
}

/// 优先纯文本；只有 HTML 时转纯文本。`html_to_text` 会丢掉 `head` / `style` /
/// `script` / `template`，只保留可见文字，链接保留锚文本且不带 `href`。
fn pick_body(message: &MessageRef<'_>) -> Option<String> {
    if let Some(text) = message.body_text(0) {
        let cleaned = clean_block(&text);
        if !cleaned.is_empty() {
            return Some(truncate_chars(&cleaned, MAX_BODY_CHARS));
        }
    }
    if let Some(html) = message.body_html(0) {
        let cleaned = clean_block(&html_to_text(&html));
        if !cleaned.is_empty() {
            return Some(truncate_chars(&cleaned, MAX_BODY_CHARS));
        }
    }
    None
}

/// 附件名只留文件名，不下载内容；`attachments()` 之外的 inline 图片也一并收进来。
fn collect_attachments(message: &MessageRef<'_>) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for part in message.attachments().chain(message.other_parts()) {
        let Some(name) = part.attachment_name() else {
            continue;
        };
        let name = clean_inline(name);
        if name.is_empty() || names.contains(&name) {
            continue;
        }
        names.push(truncate_chars(&name, MAX_ATTACHMENT_NAME_CHARS));
    }
    names
}

fn collect_addresses(list: Option<AddressList<'_>>) -> Vec<String> {
    let Some(list) = list else {
        return Vec::new();
    };
    list.mailboxes()
        .filter_map(|mailbox| format_mailbox(&mailbox))
        .map(|value| truncate_chars(&value, MAX_ACTOR_CHARS))
        .take(MAX_ADDRESSES)
        .collect()
}

fn format_mailbox(mailbox: &Mailbox<'_>) -> Option<String> {
    let name = mailbox.name().map(clean_inline).unwrap_or_default();
    let address = mailbox.address().map(clean_inline).unwrap_or_default();
    match (name.is_empty(), address.is_empty()) {
        (true, true) => None,
        (true, false) => Some(address),
        (false, true) => Some(name),
        (false, false) => Some(format!("{name} <{address}>")),
    }
}

/// 单行字段用：空白与控制字符一律压成一个空格，去掉首尾空白。
fn clean_inline(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut pending_space = false;
    for ch in value.chars() {
        if ch.is_whitespace() || ch.is_control() {
            pending_space = !result.is_empty();
            continue;
        }
        if pending_space {
            result.push(' ');
            pending_space = false;
        }
        result.push(ch);
    }
    result
}

/// 正文用：换行统一成 LF、去掉其余控制字符、三个以上连续换行压成两个。
fn clean_block(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut newlines = 0usize;
    for ch in value.chars() {
        match ch {
            '\r' => continue,
            '\n' => {
                newlines += 1;
                if newlines <= 2 {
                    result.push('\n');
                }
            }
            '\t' => {
                newlines = 0;
                result.push('\t');
            }
            ch if ch.is_control() => continue,
            ch => {
                newlines = 0;
                result.push(ch);
            }
        }
    }
    result.trim().to_owned()
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_owned();
    }
    value.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;

    fn fixture(name: &str) -> ParsedMail {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("email")
            .join(name);
        let raw = std::fs::read(&path)
            .unwrap_or_else(|error| panic!("读取 fixture {} 失败：{error}", path.display()));
        parse_message(&raw)
    }

    /// fixtures 目录里应当正好有这 7 个样本。
    fn fixture_names() -> Vec<String> {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("email");
        let mut names: Vec<String> = std::fs::read_dir(directory)
            .expect("fixtures 目录存在")
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| name.ends_with(".eml"))
            .collect();
        names.sort();
        names
    }

    #[test]
    fn plain_text_fixture_keeps_header_fields() {
        let parsed = fixture("plain.eml");
        assert_eq!(parsed.subject, "Weekly digest");
        assert_eq!(parsed.from.as_deref(), Some("Ann Lee <ann@example.com>"));
        assert_eq!(parsed.from_address.as_deref(), Some("ann@example.com"));
        assert_eq!(parsed.message_id.as_deref(), Some("plain-1@example.com"));
        assert!(parsed.date_unix.is_some(), "Date 合法时应解析出时间戳");
        assert!(parsed.body.as_deref().unwrap_or("").contains("Hello there"));
        assert!(parsed.attachment_names.is_empty());
    }

    #[test]
    fn gbk_fixture_decodes_subject_and_body() {
        let parsed = fixture("gbk.eml");
        assert_eq!(parsed.subject, "测试邮件");
        assert!(
            parsed
                .body
                .as_deref()
                .unwrap_or("")
                .contains("这是中文正文"),
            "GBK 正文应解码成功，实际：{:?}",
            parsed.body
        );
    }

    #[test]
    fn html_only_fixture_drops_markup_and_tracking_pixel() {
        let parsed = fixture("html-only.eml");
        let body = parsed.body.as_deref().expect("HTML 应转出正文");
        assert!(body.contains("打开查看"), "应保留可见文字：{body}");
        assert!(body.contains("公告链接"), "应保留锚文本：{body}");
        assert!(!body.contains("tracker.example.com"), "追踪像素不能进正文");
        assert!(!body.contains("alert("), "script 内容不能进正文");
        assert!(!body.contains("display:none"), "style 内容不能进正文");
        // 行内图片不下载、不计入附件：HTML 里的 <img> 不是 MIME 分段。
        assert!(parsed.attachment_names.is_empty());
    }

    #[test]
    fn alternative_with_attachment_prefers_plain_text() {
        let parsed = fixture("multipart-alternative-attachment.eml");
        assert_eq!(parsed.attachment_names, vec!["report.pdf".to_owned()]);
        let body = parsed.body.as_deref().expect("应取到正文");
        assert!(body.contains("纯文本正文"), "应优先 text/plain：{body}");
        assert!(!body.contains("<p>"), "不应拿 HTML 当正文：{body}");
    }

    #[test]
    fn missing_message_id_leaves_none() {
        let parsed = fixture("no-message-id.eml");
        assert!(parsed.message_id.is_none());
        // 标题与正文仍要可用，兜底 id 由映射层用 uid 生成。
        assert_eq!(parsed.subject, "Notice without Message-ID");
        assert!(parsed.body.is_some());
    }

    #[test]
    fn invalid_date_leaves_none() {
        let parsed = fixture("bad-date.eml");
        assert!(parsed.date_unix.is_none(), "非法 Date 不应产出时间戳");
        assert_eq!(parsed.subject, "Broken date");
    }

    #[test]
    fn multipart_signed_fixture_reads_first_part() {
        let parsed = fixture("multipart-signed.eml");
        assert_eq!(parsed.subject, "Signed message");
        assert!(
            parsed
                .body
                .as_deref()
                .unwrap_or("")
                .contains("签名邮件的正文"),
            "应取到第一段正文：{:?}",
            parsed.body
        );
    }

    #[test]
    fn fixtures_are_all_parseable_with_subject_and_body() {
        let names = fixture_names();
        assert_eq!(names.len(), 7, "fixtures 应正好 7 个：{names:?}");
        for name in names {
            let parsed = fixture(&name);
            assert!(!parsed.subject.is_empty(), "{name} 缺少标题");
            assert!(parsed.body.is_some(), "{name} 缺少正文");
        }
    }

    #[test]
    fn oversized_input_is_truncated_before_parsing() {
        let mut raw = b"From: a@example.com\r\nSubject: big\r\n\r\n".to_vec();
        raw.resize(MAX_RAW_BYTES + 4096, b'a');
        let parsed = parse_message(&raw);
        assert_eq!(parsed.subject, "big");
        assert_eq!(parsed.server.size_bytes as usize, MAX_RAW_BYTES + 4096);
    }

    #[test]
    fn unparseable_input_degrades_instead_of_panicking() {
        let parsed = parse_message(b"\x00\x01\x02 not a message at all");
        assert!(parsed.subject.is_empty());
        assert!(parsed.body.is_none());
        assert!(parsed.message_id.is_none());
    }

    #[test]
    fn cleaning_collapses_controls_and_blank_lines() {
        assert_eq!(clean_inline("  Ann\n Lee\t<ann@x> "), "Ann Lee <ann@x>");
        assert_eq!(clean_block("a\r\n\r\n\r\n\r\nb\u{0}"), "a\n\nb");
        assert_eq!(truncate_chars("测试文本", 2), "测试");
    }
}
