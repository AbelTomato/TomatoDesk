//! GitHub 通知连接器：只把通知事件写进收件箱，PR / Issue 正文仍留在仓库模块。
//!
//! 同步是幂等的：`(source_id, external_id)` 唯一，重复拉取只会更新同一条消息；
//! 已判断的消息不会被同步覆盖，未判断的消息有新进展时重新变为未读。

use std::time::Duration;

use chrono::{DateTime, Duration as ChronoDuration, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use tauri::Emitter;

use crate::{
    domain::NewInboxMessage,
    infrastructure::{
        db::Database,
        github::{GithubClient, GithubNotification},
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
        // 走应用层入库：新消息会立刻过一遍规则，老消息只做更新。
        super::ingest(database, message).map_err(|error| error.to_string())?;
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
        category_for_reason, convert_api_url, overlap_since, reason_label, subject_html_url,
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
}
