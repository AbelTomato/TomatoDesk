//! 邮件连接器的应用层：字段映射、单账户同步、多账户轮询。
//!
//! 上半部分只做「呈现」层面的决定（分类、摘要、占位、payload）；下半部分负责
//! 遍历账户、读游标、调 IMAP 客户端、入库、推进游标与退避。判断动作不在这里，
//! 邮件进收件箱后与 GitHub 通知走同一套流程。

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::{SecondsFormat, Utc};
use tauri::Emitter;

use super::github::{INBOX_SYNC_EVENT, InboxSyncSummary};
use crate::{
    domain::{EmailAccountProbe, NewInboxMessage},
    infrastructure::{
        db::{Database, DbError},
        email::{
            EmailAccount, EmailConfig, EmailError,
            client::{self, FetchOutcome, MailCursor},
            parse::{MAX_EXTERNAL_ID_CHARS, MAX_PAYLOAD_BYTES, NO_SUBJECT, ParsedMail},
        },
        secrets,
    },
};

/// 轮询间隔。邮件比 GitHub 通知更看重「别太吵」，十分钟一轮足够。
const POLL_INTERVAL: Duration = Duration::from_secs(10 * 60);
/// 单账户一轮的时间预算，超出的封数留到下一轮。
const ACCOUNT_BUDGET: Duration = Duration::from_secs(60);
/// 整轮预算：到点后剩余账户留到下一轮，已完成的账户状态不受影响。
const ROUND_BUDGET: Duration = Duration::from_secs(5 * 60);
/// 认证类错误的退避时长。反复用错授权码重试会触发服务商风控。
const BACKOFF_DURATION: Duration = Duration::from_secs(60 * 60);

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

/// 取信动作的替换点：生产环境是 IMAP，测试用假实现。
///
/// 有了这个缝，「账户隔离」「失败不动游标」「重扫不重复」这些验收都能离线跑，
/// 不必每次连真实邮箱。
pub(crate) trait MailFetcher: Send + Sync {
    fn fetch_new(
        &self,
        account: &EmailAccount,
        password: &str,
        cursor: Option<&MailCursor>,
        deadline: Instant,
    ) -> Result<FetchOutcome, EmailError>;
}

/// 生产实现：直接转发到 IMAP 客户端。
pub(crate) struct ImapFetcher;

impl MailFetcher for ImapFetcher {
    fn fetch_new(
        &self,
        account: &EmailAccount,
        password: &str,
        cursor: Option<&MailCursor>,
        deadline: Instant,
    ) -> Result<FetchOutcome, EmailError> {
        client::fetch_new(account, password, cursor, deadline)
    }
}

/// 「测试连接」的替换点：设置页只关心结果，测试用假实现避免每次连真实邮箱。
pub(crate) trait MailProber: Send + Sync {
    fn probe(
        &self,
        account: &EmailAccount,
        password: &str,
    ) -> Result<EmailAccountProbe, EmailError>;
}

/// 生产实现：连接 → 登录 → `EXAMINE`，只读一次邮箱概况。
pub(crate) struct ImapProber;

impl MailProber for ImapProber {
    fn probe(
        &self,
        account: &EmailAccount,
        password: &str,
    ) -> Result<EmailAccountProbe, EmailError> {
        client::probe(account, password)
    }
}

/// 一轮里单个账户的结果，用来拼 `inbox-sync-updated` 的载荷。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AccountOutcome {
    pub source_id: String,
    pub fetched_count: usize,
    pub ingested_count: usize,
    pub synced_at: String,
}

/// 失败后的处置方式。文案已经写进该账户的 `last_error`，这里只决定下一轮的行为。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SyncFailure {
    /// 认证或文件夹问题：退避一小时再试。
    Backoff,
    /// 网络抖动或本地写入问题：下一轮直接重试。
    Retry,
}

/// 进程内的退避表，按 source id 记录「什么时候之前不要再试」。
///
/// 不落库：重启后重试一次是合理的，也避免为它加字段。一个账户的退避不影响其他账户。
#[derive(Debug, Default)]
pub(crate) struct Backoff {
    until: HashMap<String, Instant>,
}

impl Backoff {
    fn is_blocked(&self, source_id: &str, now: Instant) -> bool {
        self.until.get(source_id).is_some_and(|until| *until > now)
    }

    fn block(&mut self, source_id: &str, now: Instant) {
        self.until
            .insert(source_id.to_owned(), now + BACKOFF_DURATION);
    }

    fn clear(&mut self, source_id: &str) {
        self.until.remove(source_id);
    }
}

/// 后台轮询邮箱，取代界面上的手动同步。
///
/// 首轮立即执行，所以配置保存后不用等十分钟；没有启用账户时整轮跳过，
/// 既不连接也不发事件。
pub fn start_poller(database: Database, app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(POLL_INTERVAL);
        // 同步慢于一个间隔时不要补跑积压的 tick，直接从下一轮开始。
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let fetcher: Arc<dyn MailFetcher> = Arc::new(ImapFetcher);
        let mut backoff = Backoff::default();
        loop {
            interval.tick().await;
            for summary in sync_all(&database, &fetcher, &mut backoff).await {
                let _ = app.emit(INBOX_SYNC_EVENT, summary);
            }
        }
    });
}

/// 跑一轮同步。返回要广播的事件，元素顺序与账户顺序一致：
/// `Some` 是成功摘要，`None` 表示该账户失败（错误已经在它的 `last_error` 里）。
pub(crate) async fn sync_all(
    database: &Database,
    fetcher: &Arc<dyn MailFetcher>,
    backoff: &mut Backoff,
) -> Vec<Option<InboxSyncSummary>> {
    sync_selected(database, fetcher, backoff, None).await
}

/// 保存账户后立刻同步这一个账户，界面不用等下一次十分钟轮询。
///
/// 用独立的退避表：用户刚刚改过配置或授权码，上一次失败不应该拦住这次重试。
pub fn sync_account_soon(database: Database, app: tauri::AppHandle, source_id: String) {
    tauri::async_runtime::spawn(async move {
        let fetcher: Arc<dyn MailFetcher> = Arc::new(ImapFetcher);
        let mut backoff = Backoff::default();
        for summary in sync_selected(&database, &fetcher, &mut backoff, Some(&source_id)).await {
            let _ = app.emit(INBOX_SYNC_EVENT, summary);
        }
    });
}

/// `only` 是可选的单账户过滤：`None` 表示整轮全部启用账户。
async fn sync_selected(
    database: &Database,
    fetcher: &Arc<dyn MailFetcher>,
    backoff: &mut Backoff,
    only: Option<&str>,
) -> Vec<Option<InboxSyncSummary>> {
    let Ok(accounts) = enabled_accounts(database, only) else {
        return Vec::new();
    };

    let mut events = Vec::new();
    let started = Instant::now();
    for account in accounts {
        if started.elapsed() >= ROUND_BUDGET {
            break;
        }
        let now = Instant::now();
        if backoff.is_blocked(&account.id, now) {
            continue;
        }
        // 没有凭据的账户按「未配置」处理：不请求、不发事件、不改 last_error。
        let Ok(Some(password)) = secrets::get_email_password(&account.id) else {
            continue;
        };

        let budget = ROUND_BUDGET
            .saturating_sub(started.elapsed())
            .min(ACCOUNT_BUDGET);
        let deadline = now + budget;
        let job_database = database.clone();
        let job_account = account.clone();
        let job_fetcher = Arc::clone(fetcher);
        let result = tokio::task::spawn_blocking(move || {
            sync_account(
                &job_database,
                &job_account,
                &password,
                job_fetcher.as_ref(),
                deadline,
            )
        })
        .await;

        match result {
            Ok(Ok(outcome)) => {
                backoff.clear(&account.id);
                let pending_count = database
                    .inbox_messages()
                    .counts(None, None)
                    .map_or(0, |counts| counts.pending);
                events.push(Some(InboxSyncSummary {
                    source_id: outcome.source_id,
                    fetched_count: outcome.fetched_count,
                    ingested_count: outcome.ingested_count,
                    pending_count,
                    synced_at: outcome.synced_at,
                }));
            }
            Ok(Err(failure)) => {
                if failure == SyncFailure::Backoff {
                    backoff.block(&account.id, Instant::now());
                }
                events.push(None);
            }
            Err(_) => {
                // 阻塞任务异常结束：记一笔就继续，不能让一个账户拖垮整轮。
                let _ = database
                    .inbox_sources()
                    .mark_failed(&account.id, "同步任务异常终止");
                events.push(None);
            }
        }
    }
    events
}

/// 同步一个账户：读游标 → 取信 → 入库 → 推进游标。
///
/// 关键顺序：**先入库、后写游标**。中途失败时游标停在原处，下一轮从同一位置重来；
/// 已经写入的邮件靠 `(source_id, external_id)` 去重，不会产生副本。
fn sync_account(
    database: &Database,
    account: &EmailAccount,
    password: &str,
    fetcher: &dyn MailFetcher,
    deadline: Instant,
) -> Result<AccountOutcome, SyncFailure> {
    let cursor = database
        .inbox_sources()
        .cursor(&account.id)
        .map_err(|error| storage_failure(database, account, &error))?;
    let cursor = MailCursor::parse(cursor.as_deref());

    let fetched = fetcher
        .fetch_new(account, password, cursor.as_ref(), deadline)
        .map_err(|error| {
            let message = error.to_string();
            let _ = database.inbox_sources().mark_failed(&account.id, &message);
            if error.needs_backoff() {
                SyncFailure::Backoff
            } else {
                SyncFailure::Retry
            }
        })?;

    let synced_at = now_iso();
    let mut ingested_count = 0usize;
    for parsed in &fetched.mails {
        if database
            .inbox_messages()
            .ingest(to_new_message(&account.id, parsed, &synced_at))
            .is_err()
        {
            let _ = database
                .inbox_sources()
                .mark_failed(&account.id, "无法写入本地收件箱");
            return Err(SyncFailure::Retry);
        }
        ingested_count += 1;
    }

    database
        .inbox_sources()
        .mark_synced(
            &account.id,
            Some(&fetched.next_cursor.to_json()),
            &synced_at,
        )
        .map_err(|error| storage_failure(database, account, &error))?;

    Ok(AccountOutcome {
        source_id: account.id.clone(),
        fetched_count: fetched.fetched_count,
        ingested_count,
        synced_at,
    })
}

/// 启用且配置可解析的账户。配置损坏时跳过该账户，而不是让整轮同步失败。
///
/// 已删除的账户 `config_json` 是空串，`EmailConfig::parse` 返回 `None`，这里自然过滤掉。
fn enabled_accounts(database: &Database, only: Option<&str>) -> Result<Vec<EmailAccount>, DbError> {
    Ok(database
        .inbox_sources()
        .list_email()?
        .into_iter()
        .filter(|row| row.source.enabled)
        .filter(|row| only.is_none_or(|id| row.source.id == id))
        .filter_map(|row| {
            EmailConfig::parse(&row.config_json)
                .map(|config| EmailAccount::from_config(row.source.id, config))
        })
        .collect())
}

fn storage_failure(database: &Database, account: &EmailAccount, error: &DbError) -> SyncFailure {
    let message = format!("本地数据库读写失败：{error}");
    let _ = database.inbox_sources().mark_failed(&account.id, &message);
    SyncFailure::Retry
}

fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
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

    // ---- 同步编排 ----

    use std::{collections::VecDeque, sync::Mutex};

    use crate::{domain::EmailTls, infrastructure::db::Database};

    fn account(database: &Database, address: &str, host: &str) -> EmailAccount {
        let config = EmailConfig::new(address, (host, 993, EmailTls::Implicit), None);
        let config_json = config.to_json();
        let account = EmailAccount::new(config);
        database
            .inbox_sources()
            .upsert_email(&account.id, account.display_name(), &config_json)
            .expect("写入邮箱账户");
        account
    }

    /// 记录调用参数、按预置队列返回结果的取信器。
    #[derive(Default)]
    struct FakeFetcher {
        replies: Mutex<HashMap<String, VecDeque<Result<FetchOutcome, EmailError>>>>,
        calls: Mutex<Vec<(String, Option<MailCursor>)>>,
    }

    impl FakeFetcher {
        fn push(&self, source_id: &str, reply: Result<FetchOutcome, EmailError>) {
            self.replies
                .lock()
                .expect("取信器锁")
                .entry(source_id.to_owned())
                .or_default()
                .push_back(reply);
        }

        fn calls(&self) -> Vec<(String, Option<MailCursor>)> {
            self.calls.lock().expect("取信器锁").clone()
        }
    }

    impl MailFetcher for FakeFetcher {
        fn fetch_new(
            &self,
            account: &EmailAccount,
            _password: &str,
            cursor: Option<&MailCursor>,
            _deadline: Instant,
        ) -> Result<FetchOutcome, EmailError> {
            self.calls
                .lock()
                .expect("取信器锁")
                .push((account.id.clone(), cursor.copied()));
            self.replies
                .lock()
                .expect("取信器锁")
                .get_mut(&account.id)
                .and_then(VecDeque::pop_front)
                .unwrap_or_else(|| Err(EmailError::Protocol("没有预置应答".to_owned())))
        }
    }

    fn outcome(uid_validity: u32, last_uid: u32, mails: Vec<ParsedMail>) -> FetchOutcome {
        FetchOutcome {
            mailbox_total: u32::try_from(mails.len()).unwrap_or(0),
            fetched_count: mails.len(),
            skipped_count: 0,
            mails,
            next_cursor: MailCursor {
                uid_validity,
                last_uid,
            },
        }
    }

    fn mail(uid: u32, message_id: &str) -> ParsedMail {
        ParsedMail {
            subject: format!("邮件 {uid}"),
            message_id: Some(message_id.to_owned()),
            body: Some("正文".to_owned()),
            server: MailServerMeta {
                uid,
                uid_validity: 1,
                size_bytes: 120,
            },
            ..ParsedMail::default()
        }
    }

    fn deadline() -> Instant {
        Instant::now() + Duration::from_secs(60)
    }

    fn stored_cursor(database: &Database, account: &EmailAccount) -> Option<MailCursor> {
        let raw = database
            .inbox_sources()
            .cursor(&account.id)
            .expect("读游标");
        MailCursor::parse(raw.as_deref())
    }

    #[test]
    fn sync_writes_messages_and_advances_the_cursor() {
        let database = Database::in_memory().expect("内存库");
        let account = account(&database, "me@qq.com", "imap.qq.com");
        let fetcher = FakeFetcher::default();
        fetcher.push(
            &account.id,
            Ok(outcome(
                1,
                42,
                vec![mail(41, "a@example.com"), mail(42, "b@example.com")],
            )),
        );

        let result =
            sync_account(&database, &account, "授权码", &fetcher, deadline()).expect("同步应成功");
        assert_eq!(result.fetched_count, 2);
        assert_eq!(result.ingested_count, 2);
        assert_eq!(result.source_id, account.id);
        assert_eq!(
            database
                .inbox_messages()
                .counts(None, None)
                .expect("计数")
                .pending,
            2
        );
        assert_eq!(
            stored_cursor(&database, &account),
            Some(MailCursor {
                uid_validity: 1,
                last_uid: 42
            })
        );
    }

    #[test]
    fn the_stored_cursor_is_handed_back_to_the_fetcher() {
        let database = Database::in_memory().expect("内存库");
        let account = account(&database, "me@qq.com", "imap.qq.com");
        let seeded = MailCursor {
            uid_validity: 7,
            last_uid: 1200,
        };
        let seeded_json = seeded.to_json();
        database
            .inbox_sources()
            .mark_synced(&account.id, Some(&seeded_json), NOW)
            .expect("预置游标");

        let fetcher = FakeFetcher::default();
        fetcher.push(
            &account.id,
            Ok(outcome(7, 1201, vec![mail(1201, "c@example.com")])),
        );
        sync_account(&database, &account, "授权码", &fetcher, deadline()).expect("同步应成功");

        assert_eq!(fetcher.calls(), vec![(account.id.clone(), Some(seeded))]);
    }

    #[test]
    fn a_second_round_does_not_duplicate_messages() {
        let database = Database::in_memory().expect("内存库");
        let account = account(&database, "me@qq.com", "imap.qq.com");
        let fetcher = FakeFetcher::default();
        for _ in 0..2 {
            fetcher.push(
                &account.id,
                Ok(outcome(
                    1,
                    42,
                    vec![mail(41, "a@example.com"), mail(42, "b@example.com")],
                )),
            );
        }

        sync_account(&database, &account, "授权码", &fetcher, deadline()).expect("第一轮");
        sync_account(&database, &account, "授权码", &fetcher, deadline()).expect("第二轮");

        assert_eq!(
            database
                .inbox_messages()
                .counts(None, None)
                .expect("计数")
                .all,
            2,
            "同一 Message-ID 重复同步必须只有一行"
        );
    }

    #[test]
    fn auth_failure_keeps_the_cursor_and_records_the_error() {
        let database = Database::in_memory().expect("内存库");
        let account = account(&database, "me@qq.com", "imap.qq.com");
        let seeded = MailCursor {
            uid_validity: 1,
            last_uid: 42,
        };
        let seeded_json = seeded.to_json();
        database
            .inbox_sources()
            .mark_synced(&account.id, Some(&seeded_json), NOW)
            .expect("预置游标");

        let fetcher = FakeFetcher::default();
        fetcher.push(&account.id, Err(EmailError::Auth("授权码无效".into())));

        let failure = sync_account(&database, &account, "错的授权码", &fetcher, deadline())
            .expect_err("认证失败应返回错误");
        assert_eq!(failure, SyncFailure::Backoff, "认证问题要退避，别每轮重试");
        assert_eq!(
            stored_cursor(&database, &account),
            Some(seeded),
            "失败不能推进游标"
        );

        let source = database
            .inbox_sources()
            .get(&account.id)
            .expect("读来源")
            .expect("来源应存在");
        assert!(
            source.last_error.unwrap_or_default().contains("登录失败"),
            "错误文案要指向授权码"
        );
    }

    #[test]
    fn network_failure_is_retried_without_backoff() {
        let database = Database::in_memory().expect("内存库");
        let account = account(&database, "me@qq.com", "imap.qq.com");
        let fetcher = FakeFetcher::default();
        fetcher.push(&account.id, Err(EmailError::Unreachable("超时".into())));

        let failure = sync_account(&database, &account, "授权码", &fetcher, deadline())
            .expect_err("断网应返回错误");
        assert_eq!(failure, SyncFailure::Retry);
        assert_eq!(stored_cursor(&database, &account), None);
    }

    #[test]
    fn a_changed_uid_validity_rescans_without_duplicates() {
        let database = Database::in_memory().expect("内存库");
        let account = account(&database, "me@qq.com", "imap.qq.com");
        let fetcher = FakeFetcher::default();
        fetcher.push(
            &account.id,
            Ok(outcome(
                1,
                2,
                vec![mail(1, "a@example.com"), mail(2, "b@example.com")],
            )),
        );
        fetcher.push(
            &account.id,
            Ok(outcome(
                9,
                3,
                vec![
                    mail(1, "a@example.com"),
                    mail(2, "b@example.com"),
                    mail(3, "c@example.com"),
                ],
            )),
        );

        sync_account(&database, &account, "授权码", &fetcher, deadline()).expect("第一轮");
        sync_account(&database, &account, "授权码", &fetcher, deadline()).expect("重扫");

        assert_eq!(
            database
                .inbox_messages()
                .counts(None, None)
                .expect("计数")
                .all,
            3,
            "UID 重排后重扫，老邮件靠 Message-ID 去重"
        );
        assert_eq!(
            stored_cursor(&database, &account),
            Some(MailCursor {
                uid_validity: 9,
                last_uid: 3
            })
        );
    }

    #[test]
    fn a_failing_account_does_not_disturb_another() {
        let database = Database::in_memory().expect("内存库");
        let broken = account(&database, "old@163.com", "imap.163.com");
        let healthy = account(&database, "me@qq.com", "imap.qq.com");
        let fetcher = FakeFetcher::default();
        fetcher.push(&broken.id, Err(EmailError::Auth("授权码失效".into())));
        fetcher.push(
            &healthy.id,
            Ok(outcome(1, 5, vec![mail(5, "ok@example.com")])),
        );

        assert_eq!(
            sync_account(&database, &broken, "旧授权码", &fetcher, deadline())
                .expect_err("旧账户应失败"),
            SyncFailure::Backoff
        );
        let result = sync_account(&database, &healthy, "授权码", &fetcher, deadline())
            .expect("另一个账户不受影响");
        assert_eq!(result.ingested_count, 1);
        assert_eq!(
            database
                .inbox_messages()
                .counts(None, None)
                .expect("计数")
                .all,
            1
        );
    }

    #[test]
    fn a_round_without_accounts_touches_nothing() {
        let database = Database::in_memory().expect("内存库");
        let fake = Arc::new(FakeFetcher::default());
        let fetcher: Arc<dyn MailFetcher> = fake.clone();
        let mut backoff = Backoff::default();

        let events = tauri::async_runtime::block_on(sync_all(&database, &fetcher, &mut backoff));

        assert!(events.is_empty(), "没有账户就不该发事件");
        assert!(fake.calls().is_empty(), "没有账户就不该发起连接");
    }

    #[test]
    fn disabled_accounts_are_not_polled() {
        let database = Database::in_memory().expect("内存库");
        let account = account(&database, "me@qq.com", "imap.qq.com");
        database
            .inbox_sources()
            .set_enabled(&account.id, false)
            .expect("停用账户");

        let fake = Arc::new(FakeFetcher::default());
        let fetcher: Arc<dyn MailFetcher> = fake.clone();
        let events =
            tauri::async_runtime::block_on(sync_all(&database, &fetcher, &mut Backoff::default()));

        assert!(events.is_empty());
        assert!(fake.calls().is_empty());
    }

    #[test]
    fn backoff_blocks_only_until_it_expires() {
        let mut backoff = Backoff::default();
        let now = Instant::now();
        assert!(!backoff.is_blocked("source-email-a", now));

        backoff.block("source-email-a", now);
        assert!(backoff.is_blocked("source-email-a", now));
        assert!(
            !backoff.is_blocked("source-email-a", now + BACKOFF_DURATION),
            "退避到点后应重新尝试"
        );
        assert!(!backoff.is_blocked("source-email-b", now), "退避按账户隔离");

        backoff.clear("source-email-a");
        assert!(!backoff.is_blocked("source-email-a", now));
    }
}
