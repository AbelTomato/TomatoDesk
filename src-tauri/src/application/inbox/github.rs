//! GitHub 通知连接器：只把通知事件写进收件箱，PR / Issue 正文仍留在仓库模块。
//!
//! 同步是幂等的：`(source_id, external_id)` 唯一，重复拉取只会更新同一条消息；
//! 已判断的消息不会被同步覆盖，未判断的消息有新进展时重新变为未读。

use std::time::Duration;

use chrono::{DateTime, Duration as ChronoDuration, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use tauri::Emitter;

use crate::{
    domain::{InboxMessage, InboxReadBack, InboxSourceKind, NewInboxMessage, ReadBackErrorKind},
    infrastructure::{
        db::{Database, DbResult},
        github::{GithubClient, GithubError, GithubNotification},
        secrets,
    },
};

/// 同步时把游标往前留一点重叠：GitHub 的 `since` 只精确到秒，
/// 重叠部分靠 `(source_id, external_id)` 去重，不会产生重复消息。
const CURSOR_OVERLAP_MINUTES: i64 = 5;
const RELATED_THREAD_KIND: &str = "github_thread";

/// 一轮同步结束后广播：前端据此重载列表与角标。
/// 载荷是 `Option<InboxSyncSummary>`，失败时为 `null`——错误已经写进消息源的 `last_error`。
pub const INBOX_SYNC_EVENT: &str = "inbox-sync-updated";

/// 轮询间隔。通知比仓库内容更看重及时性，五分钟一次足够；
/// 失败也走同一个间隔，避免 Token 失效时每一轮都去打 GitHub。
const POLL_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// 已读回写队列：退避的最小步长是一分钟，队列也按一分钟一轮推进。
const READ_BACK_INTERVAL: Duration = Duration::from_secs(60);
/// 一轮最多处理这么多条，避免积压时一次性打满限流。
const READ_BACK_BATCH: u32 = 20;
/// 连续失败超过这个次数转 blocked，等待人工重试。
const READ_BACK_MAX_ATTEMPTS: u32 = 6;
const READ_BACK_BASE_SECONDS: i64 = 60;
const READ_BACK_MAX_SECONDS: i64 = 30 * 60;

/// 队列有变化时广播，前端据此刷新「等待重试 / 需要重新授权」提示。
pub const INBOX_READ_BACK_EVENT: &str = "inbox-read-back-updated";

/// 后台轮询 GitHub 通知，取代界面上的手动同步。
///
/// 第一次 tick 立即触发，所以打开应用就会补一次；没有 Token 时整轮跳过，
/// 既不请求也不广播，避免前端为一次空同步刷新。
pub fn start_poller(database: Database, app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(POLL_INTERVAL);
        // 同步慢于一个间隔时不要补跑积压的 tick，直接从下一轮开始。
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            if secrets::get_github_token().ok().flatten().is_none() {
                continue;
            }
            let summary = sync(&database).await.ok();
            let _ = app.emit(INBOX_SYNC_EVENT, summary);
        }
    });
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InboxSyncSummary {
    pub source_id: String,
    /// 本次从 GitHub 拉到的通知条数。
    pub fetched_count: usize,
    /// 实际写入或更新到本地的条数；标题为空等无法展示的条目会被跳过。
    pub ingested_count: usize,
    /// 同步后的待判断总数，前端直接用它刷新角标。
    pub pending_count: i64,
    pub synced_at: String,
}

/// 拉取一次 GitHub 通知并写入收件箱。
pub(crate) async fn sync(database: &Database) -> Result<InboxSyncSummary, String> {
    let token = secrets::get_github_token()?.ok_or_else(|| "请先保存 GitHub Token".to_owned())?;
    let source = database
        .inbox_sources()
        .ensure_github()
        .map_err(|error| error.to_string())?;
    let client =
        GithubClient::new(token, Duration::from_secs(20)).map_err(|error| error.to_string())?;

    let cursor = database
        .inbox_sources()
        .cursor(&source.id)
        .map_err(|error| error.to_string())?;
    let since = overlap_since(cursor.as_deref());
    let synced_at = now_iso();
    let notifications = match client.notifications(since.as_deref()).await {
        Ok(notifications) => notifications,
        Err(error) => {
            let message = error.to_string();
            let _ = database.inbox_sources().mark_failed(&source.id, &message);
            return Err(message);
        }
    };

    let mut ingested_count = 0;
    for notification in &notifications {
        let related = related_thread(database, notification)?;
        let payload = serde_json::to_string(notification).ok();
        let Some(message) = build_message(notification, &source.id, related, payload) else {
            continue;
        };
        database
            .inbox_messages()
            .ingest(message)
            .map_err(|error| error.to_string())?;
        ingested_count += 1;
    }

    // 同步可能把消息重新变成未读（远端有新动态），旧的「已读」意图随之作废。
    database
        .inbox_read_back()
        .remove_unread()
        .map_err(|error| error.to_string())?;

    database
        .inbox_sources()
        .mark_synced(&source.id, Some(&synced_at), &synced_at)
        .map_err(|error| error.to_string())?;
    let counts = database
        .inbox_messages()
        .counts(None, None)
        .map_err(|error| error.to_string())?;

    Ok(InboxSyncSummary {
        source_id: source.id,
        fetched_count: notifications.len(),
        ingested_count,
        pending_count: counts.pending,
        synced_at,
    })
}

/// 本地「已读」不再直接打网络：写进队列，由后台循环按退避重试。
///
/// 这样点已读不会被网络超时拖住，失败也不会被静默吞掉——
/// 队列里的每一行都表示「本地已读、远端还不知道」。
pub(crate) fn enqueue_read_back(database: &Database, message: &InboxMessage) -> DbResult<()> {
    let Some(source) = database.inbox_sources().get(&message.source_id)? else {
        return Ok(());
    };
    // 只回写 GitHub 通知；外部 id 不是数字时远端一定拒绝，不必入队。
    if source.kind != InboxSourceKind::Github || !is_notification_id(&message.external_id) {
        return Ok(());
    }
    database
        .inbox_read_back()
        .enqueue(&message.id, &message.source_id, &message.external_id)
}

fn is_notification_id(value: &str) -> bool {
    !value.is_empty() && value.chars().all(|digit| digit.is_ascii_digit())
}

/// 本地改回未读：撤销还没写出去的「已读」意图。
pub(crate) fn cancel_read_back(database: &Database, message_id: &str) -> DbResult<usize> {
    database.inbox_read_back().remove_for_message(message_id)
}

/// 重新保存 Token 后让 blocked 行回到自动重试，不必逐条点重试。
pub(crate) fn reset_after_token_saved(database: &Database) -> DbResult<usize> {
    database.inbox_read_back().reset_blocked()
}

/// 删除 Token 后 pending 行写不出去，直接丢弃；blocked 保留，继续提示需要授权。
pub(crate) fn clear_pending_after_token_removed(database: &Database) -> DbResult<usize> {
    database.inbox_read_back().clear_pending()
}

/// 处理一批到期的回写。没有 Token 时整轮跳过，不消耗退避次数。
async fn process_due(database: &Database) -> Result<usize, String> {
    let Some(token) = secrets::get_github_token()? else {
        return Ok(0);
    };
    let client =
        GithubClient::new(token, Duration::from_secs(10)).map_err(|error| error.to_string())?;
    let now = Utc::now();
    let due = database
        .inbox_read_back()
        .due(&iso(now), READ_BACK_BATCH)
        .map_err(|error| error.to_string())?;

    let mut handled = 0;
    for entry in due {
        match client.mark_notification_read(&entry.external_id).await {
            Ok(()) => {
                database
                    .inbox_read_back()
                    .remove(&entry.id)
                    .map_err(|error| error.to_string())?;
            }
            Err(error) => {
                apply_failure(database, &entry, &error, now).map_err(|error| error.to_string())?;
            }
        }
        handled += 1;
    }
    Ok(handled)
}

/// 队列循环：一分钟一轮，启动时立即跑一次。
pub fn start_read_back_worker(database: Database, app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(READ_BACK_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            let Ok(handled) = process_due(&database).await else {
                continue;
            };
            if handled == 0 {
                continue;
            }
            if let Ok(status) = database.inbox_read_back().status() {
                let _ = app.emit(INBOX_READ_BACK_EVENT, status);
            }
        }
    });
}

/// 失败后的处置方式：能重试的排时间，不能重试的删行或转人工。
#[derive(Debug, PartialEq, Eq)]
enum FailurePlan {
    /// 远端线程已不存在，或请求本身不可能成功：删行。
    Drop,
    /// 需要人工介入：保留记录，退出自动重试。
    Block(ReadBackErrorKind),
    /// 可恢复：到点再试。
    Retry {
        delay_seconds: i64,
        kind: ReadBackErrorKind,
    },
}

fn plan_failure(error: &GithubError, attempts: u32) -> FailurePlan {
    match error {
        GithubError::Unauthorized | GithubError::MissingNotificationsScope => {
            FailurePlan::Block(ReadBackErrorKind::Auth)
        }
        // 404 说明线程没了；其它 4xx 多为请求本身有问题，重试不会变好。
        GithubError::NotFound | GithubError::Api(_) | GithubError::InvalidRepository(_) => {
            FailurePlan::Drop
        }
        GithubError::RateLimited(reset) => {
            if attempts > READ_BACK_MAX_ATTEMPTS {
                FailurePlan::Block(ReadBackErrorKind::Exhausted)
            } else {
                FailurePlan::Retry {
                    delay_seconds: rate_limit_delay(reset.as_deref()),
                    kind: ReadBackErrorKind::RateLimited,
                }
            }
        }
        GithubError::Request(_) | GithubError::Unavailable(_) => {
            if attempts > READ_BACK_MAX_ATTEMPTS {
                FailurePlan::Block(ReadBackErrorKind::Exhausted)
            } else {
                FailurePlan::Retry {
                    delay_seconds: backoff_seconds(attempts),
                    kind: ReadBackErrorKind::Transient,
                }
            }
        }
    }
}

fn apply_failure(
    database: &Database,
    entry: &InboxReadBack,
    error: &GithubError,
    now: DateTime<Utc>,
) -> DbResult<()> {
    let attempts = entry.attempts + 1;
    let message = error.to_string();
    let repository = database.inbox_read_back();
    match plan_failure(error, attempts) {
        FailurePlan::Drop => repository.remove(&entry.id).map(|_| ()),
        FailurePlan::Block(kind) => repository.mark_blocked(&entry.id, attempts, &message, kind),
        FailurePlan::Retry {
            delay_seconds,
            kind,
        } => {
            let delay = jittered(delay_seconds, &entry.id, now);
            let next = iso(now + ChronoDuration::seconds(delay));
            repository.mark_retry(&entry.id, attempts, &next, &message, kind)
        }
    }
}

/// 指数退避：1、2、4、8、16、30 分钟，封顶 30 分钟。
fn backoff_seconds(attempts: u32) -> i64 {
    let factor = 1_i64 << attempts.saturating_sub(1).min(10);
    (READ_BACK_BASE_SECONDS * factor).min(READ_BACK_MAX_SECONDS)
}

/// 限流优先用远端给的时间：`x-ratelimit-reset` 是 epoch 秒，`retry-after` 是秒数。
fn rate_limit_delay(reset: Option<&str>) -> i64 {
    let Some(value) = reset.and_then(|value| value.parse::<i64>().ok()) else {
        return READ_BACK_BASE_SECONDS;
    };
    if value >= 1_000_000_000 {
        (value - Utc::now().timestamp()).clamp(30, READ_BACK_MAX_SECONDS)
    } else {
        value.clamp(10, READ_BACK_MAX_SECONDS)
    }
}

/// ±20% 抖动，避免多条同时失败后在同一个 tick 里挤在一起重试。
fn jittered(base: i64, seed_text: &str, now: DateTime<Utc>) -> i64 {
    let span = base / 5;
    if span <= 0 {
        return base;
    }
    let seed = seed_text.bytes().map(u32::from).sum::<u32>() ^ now.timestamp_subsec_millis();
    let offset = i64::from(seed % (span as u32 * 2 + 1)) - span;
    (base + offset).max(10)
}

/// 命中本地已同步的 PR / Issue 时写 `related_kind = github_thread`，点击直接进详情；
/// 未关注或未同步的仓库不建立关联，界面退回外部浏览器。
fn related_thread(
    database: &Database,
    notification: &GithubNotification,
) -> Result<Option<String>, String> {
    let Some((owner, name)) = notification.repository.full_name.split_once('/') else {
        return Ok(None);
    };
    let Some(repo_id) = database
        .github_repos()
        .find_id(owner, name)
        .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    let Some(number) = thread_number(notification) else {
        return Ok(None);
    };
    database
        .github_threads()
        .find_id(&repo_id, number)
        .map_err(|error| error.to_string())
}

/// 通知转成收件箱消息：标题取通知标题，摘要只放仓库与原因，不复制正文。
fn build_message(
    notification: &GithubNotification,
    source_id: &str,
    related_id: Option<String>,
    payload_json: Option<String>,
) -> Option<NewInboxMessage> {
    let external_id = external_id(notification)?;
    let title = notification.subject.title.trim();
    if title.is_empty() {
        return None;
    }
    let external_updated_at =
        Some(notification.updated_at.clone()).filter(|value| !value.trim().is_empty());

    Some(NewInboxMessage {
        source_id: source_id.to_owned(),
        external_id,
        external_updated_at: external_updated_at.clone(),
        category: category_for_reason(&notification.reason).to_owned(),
        title: title.to_owned(),
        summary: Some(format!(
            "{} · {}",
            notification.repository.full_name,
            reason_label(&notification.reason)
        )),
        body: None,
        url: subject_html_url(notification),
        actor: None,
        occurred_at: external_updated_at,
        related_kind: related_id.as_ref().map(|_| RELATED_THREAD_KIND.to_owned()),
        related_id,
        payload_json,
    })
}

fn external_id(notification: &GithubNotification) -> Option<String> {
    match &notification.id {
        serde_json::Value::String(value) if !value.trim().is_empty() => {
            Some(value.trim().to_owned())
        }
        serde_json::Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

/// GitHub 的 `reason` 比方案里的六类更细，这里收敛成收件箱的分类。
fn category_for_reason(reason: &str) -> &'static str {
    match reason {
        "mention" | "team_mention" => "mention",
        "review_requested" => "review_requested",
        "assign" => "assigned",
        "ci_activity" => "ci_failed",
        "release" => "release",
        "comment" | "state_change" => "comment",
        // subscribed / manual / invitation / security_alert 等统一归为普通通知。
        _ => "notification",
    }
}

fn reason_label(reason: &str) -> String {
    match reason {
        "mention" | "team_mention" => "有人提到了你".to_owned(),
        "review_requested" => "请求你评审".to_owned(),
        "assign" => "指派给你".to_owned(),
        "ci_activity" => "CI 有结果".to_owned(),
        "release" => "新版本发布".to_owned(),
        "comment" => "有新评论".to_owned(),
        "state_change" => "状态有变化".to_owned(),
        "subscribed" => "你关注的动态".to_owned(),
        "manual" => "手动订阅".to_owned(),
        other => other.to_owned(),
    }
}

/// 通知里的 `subject.url` 指向 api.github.com，展示时换成 github.com 页面。
fn subject_html_url(notification: &GithubNotification) -> Option<String> {
    match notification.subject.kind.as_deref() {
        // 检查套件没有对应的单页，统一落到 Actions 列表。
        Some("CheckSuite") | Some("CheckRun") => Some(format!(
            "{}/actions",
            notification.repository.html_url.trim_end_matches('/')
        )),
        // Release 的 api 地址带的是数据库 id，不是 tag，落到 Release 列表更稳。
        Some("Release") => Some(format!(
            "{}/releases",
            notification.repository.html_url.trim_end_matches('/')
        )),
        _ => notification
            .subject
            .url
            .as_deref()
            .and_then(convert_api_url)
            .or_else(|| Some(notification.repository.html_url.clone())),
    }
}

fn convert_api_url(url: &str) -> Option<String> {
    url.trim()
        .strip_prefix("https://api.github.com/repos/")
        .map(|path| format!("https://github.com/{path}"))
}

/// 只有 PR / Issue 能对应到本地线程；Release 与检查套件只留外部链接。
fn thread_number(notification: &GithubNotification) -> Option<i64> {
    if !matches!(
        notification.subject.kind.as_deref(),
        Some("PullRequest") | Some("Issue")
    ) {
        return None;
    }
    notification
        .subject
        .url
        .as_deref()?
        .trim_end_matches('/')
        .rsplit('/')
        .next()?
        .parse()
        .ok()
}

fn overlap_since(cursor: Option<&str>) -> Option<String> {
    let cursor = cursor?;
    let parsed = chrono::DateTime::parse_from_rfc3339(cursor).ok()?;
    Some(
        (parsed.with_timezone(&Utc) - ChronoDuration::minutes(CURSOR_OVERLAP_MINUTES))
            .to_rfc3339_opts(SecondsFormat::Secs, true),
    )
}

fn iso(now: DateTime<Utc>) -> String {
    now.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn now_iso() -> String {
    iso(Utc::now())
}

#[cfg(test)]
mod tests {
    use super::{
        FailurePlan, backoff_seconds, category_for_reason, convert_api_url, is_notification_id,
        jittered, overlap_since, plan_failure, rate_limit_delay, reason_label, subject_html_url,
        thread_number,
    };
    use crate::infrastructure::github::{
        GithubNotification, GithubNotificationRepository, GithubNotificationSubject,
    };

    fn notification(kind: &str, url: &str, reason: &str) -> GithubNotification {
        GithubNotification {
            id: serde_json::json!("42"),
            unread: true,
            reason: reason.to_owned(),
            updated_at: "2026-11-26T10:00:00Z".to_owned(),
            subject: GithubNotificationSubject {
                title: "标题".to_owned(),
                url: Some(url.to_owned()),
                latest_comment_url: None,
                kind: Some(kind.to_owned()),
            },
            repository: GithubNotificationRepository {
                full_name: "TencentCloud/Octop".to_owned(),
                html_url: "https://github.com/TencentCloud/Octop".to_owned(),
            },
        }
    }

    #[test]
    fn reasons_map_to_inbox_categories() {
        assert_eq!(category_for_reason("mention"), "mention");
        assert_eq!(category_for_reason("team_mention"), "mention");
        assert_eq!(category_for_reason("review_requested"), "review_requested");
        assert_eq!(category_for_reason("assign"), "assigned");
        assert_eq!(category_for_reason("ci_activity"), "ci_failed");
        assert_eq!(category_for_reason("release"), "release");
        assert_eq!(category_for_reason("comment"), "comment");
        assert_eq!(
            category_for_reason("subscribed"),
            "notification",
            "unknown reasons fall back to a generic category"
        );
        assert_eq!(reason_label("assign"), "指派给你");
    }

    #[test]
    fn api_subject_urls_become_html_urls() {
        assert_eq!(
            convert_api_url("https://api.github.com/repos/TencentCloud/Octop/pulls/482").as_deref(),
            Some("https://github.com/TencentCloud/Octop/pulls/482")
        );
        assert_eq!(convert_api_url("https://example.com/x"), None);

        let pull = notification(
            "PullRequest",
            "https://api.github.com/repos/TencentCloud/Octop/pulls/482",
            "mention",
        );
        assert_eq!(
            subject_html_url(&pull).as_deref(),
            Some("https://github.com/TencentCloud/Octop/pulls/482")
        );

        let check = notification(
            "CheckSuite",
            "https://api.github.com/repos/TencentCloud/Octop/check-suites/9",
            "ci_activity",
        );
        assert_eq!(
            subject_html_url(&check).as_deref(),
            Some("https://github.com/TencentCloud/Octop/actions")
        );

        let release = notification(
            "Release",
            "https://api.github.com/repos/TencentCloud/Octop/releases/123",
            "release",
        );
        assert_eq!(
            subject_html_url(&release).as_deref(),
            Some("https://github.com/TencentCloud/Octop/releases"),
            "release api ids are not tag names, so link to the release list"
        );
    }

    #[test]
    fn only_pull_requests_and_issues_expose_thread_numbers() {
        let pull = notification(
            "PullRequest",
            "https://api.github.com/repos/TencentCloud/Octop/pulls/482",
            "mention",
        );
        assert_eq!(thread_number(&pull), Some(482));

        let issue = notification(
            "Issue",
            "https://api.github.com/repos/TencentCloud/Octop/issues/466/",
            "assign",
        );
        assert_eq!(thread_number(&issue), Some(466));

        let release = notification(
            "Release",
            "https://api.github.com/repos/TencentCloud/Octop/releases/123",
            "release",
        );
        assert_eq!(thread_number(&release), None);
    }

    #[test]
    fn cursor_overlap_keeps_a_safe_window() {
        assert_eq!(overlap_since(None), None);
        assert_eq!(
            overlap_since(Some("2026-11-26T10:00:00.000Z")).as_deref(),
            Some("2026-11-26T09:55:00Z")
        );
        assert_eq!(overlap_since(Some("not-a-date")), None);
    }

    #[test]
    fn failures_choose_between_retry_drop_and_block() {
        use crate::domain::ReadBackErrorKind;
        use crate::infrastructure::github::GithubError;

        assert_eq!(
            plan_failure(&GithubError::Request("超时".into()), 1),
            FailurePlan::Retry {
                delay_seconds: 60,
                kind: ReadBackErrorKind::Transient
            }
        );
        assert_eq!(backoff_seconds(2), 120);
        assert_eq!(backoff_seconds(3), 240);
        assert_eq!(
            backoff_seconds(6),
            1800,
            "the last delay is capped at thirty minutes"
        );
        assert_eq!(
            plan_failure(&GithubError::Unavailable("HTTP 503".into()), 7),
            FailurePlan::Block(ReadBackErrorKind::Exhausted),
            "backoff gives up after the sixth retry"
        );

        // 认证与权限问题重试没有意义，等重新授权。
        assert_eq!(
            plan_failure(&GithubError::Unauthorized, 1),
            FailurePlan::Block(ReadBackErrorKind::Auth)
        );
        assert_eq!(
            plan_failure(&GithubError::MissingNotificationsScope, 1),
            FailurePlan::Block(ReadBackErrorKind::Auth)
        );

        // 线程没了、请求本身有问题：删行，不重试。
        assert_eq!(plan_failure(&GithubError::NotFound, 1), FailurePlan::Drop);
        assert_eq!(
            plan_failure(&GithubError::Api("HTTP 422".into()), 1),
            FailurePlan::Drop
        );

        assert_eq!(
            plan_failure(&GithubError::RateLimited(Some("120".into())), 1),
            FailurePlan::Retry {
                delay_seconds: 120,
                kind: ReadBackErrorKind::RateLimited
            }
        );
        assert_eq!(rate_limit_delay(None), 60);
        assert_eq!(rate_limit_delay(Some("5")), 10, "retry-after has a floor");
        assert_eq!(rate_limit_delay(Some("999999")), 1800, "and a ceiling");
    }

    #[test]
    fn backoff_jitter_stays_within_twenty_percent() {
        let now = chrono::Utc::now();
        for seed in ["a", "bb", "ccc", "notification-4242"] {
            let delay = jittered(600, seed, now);
            assert!(
                (480..=720).contains(&delay),
                "jitter must keep the retry inside ±20%"
            );
        }
    }

    #[test]
    fn only_numeric_notification_ids_are_written_back() {
        assert!(is_notification_id("4242"));
        assert!(!is_notification_id(""));
        assert!(!is_notification_id("thread-1"));
    }
}
