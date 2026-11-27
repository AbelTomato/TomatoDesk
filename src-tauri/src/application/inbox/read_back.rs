//! 已读回写队列：本地标已读 → 远端写回。
//!
//! 队列承载两种动作：GitHub 通知的已读（`github_read`）与邮件的 `\Seen`
//! （`email_seen`）。两者共用一套退避、可见性与「失败才留行」的约定，但执行方式、
//! 批量上限与失败分类完全不同，`process_due` 按 `action` 分派。
//!
//! 一条规则贯穿全部实现：**只写已读，不写未读**。改回未读只取消还没写出去的意图，
//! 不会把远端已经标上的已读翻回来。

use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::{DateTime, Duration as ChronoDuration, SecondsFormat, Utc};
use tauri::Emitter;

use super::email::MailSeenStore;
use crate::{
    domain::{InboxMessage, InboxReadBack, InboxSourceKind, ReadBackAction, ReadBackErrorKind},
    infrastructure::{
        db::{Database, DbResult},
        email::{EmailAccount, EmailConfig, EmailError},
        github::{GithubClient, GithubError},
    },
};

/// 一轮回写间隔：退避的最小步长是一分钟，队列也按一分钟推进。
const INTERVAL: Duration = Duration::from_secs(60);
/// GitHub 一条通知一个请求，一轮 20 条已经够用，也避免打满限流。
const GITHUB_BATCH: u32 = 20;
/// 邮件一次连接可以带 200 个 UID，一条 `STORE` 就是一批。
const EMAIL_BATCH: u32 = 200;
/// 单个账户一次回写的时间预算：到点后剩下的留给下一轮。
const EMAIL_BUDGET: Duration = Duration::from_secs(20);
/// 连续失败超过这个次数转 blocked，等待人工重试。
const MAX_ATTEMPTS: u32 = 6;
const BASE_SECONDS: i64 = 60;
const MAX_SECONDS: i64 = 30 * 60;

/// 队列有变化时广播，前端据此刷新「等待重试 / 需要重新授权」提示。
pub const INBOX_READ_BACK_EVENT: &str = "inbox-read-back-updated";

/// 本地标已读后入队。取不到远端标识就静默跳过——本地已读是唯一的事实来源，
/// 远端写不出去不该反过来拦住界面。
pub(crate) fn enqueue(database: &Database, message: &InboxMessage) -> DbResult<()> {
    let Some(source) = database.inbox_sources().get(&message.source_id)? else {
        return Ok(());
    };
    match source.kind {
        InboxSourceKind::Github => {
            // 外部 id 不是数字时 GitHub 一定拒绝，不必入队。
            if !is_notification_id(&message.external_id) {
                return Ok(());
            }
            database.inbox_read_back().enqueue_github(
                &message.id,
                &message.source_id,
                &message.external_id,
            )
        }
        // 邮件靠入库时记下的 UID 回写；没有 UID（历史数据）就跳过。
        InboxSourceKind::Email => database.inbox_read_back().enqueue_email_seen(&message.id),
        InboxSourceKind::Rss | InboxSourceKind::Manual => Ok(()),
    }
}

fn is_notification_id(value: &str) -> bool {
    !value.is_empty() && value.chars().all(|digit| digit.is_ascii_digit())
}

/// 本地改回未读：撤销还没写出去的「已读」意图。
pub(crate) fn cancel(database: &Database, message_id: &str) -> DbResult<usize> {
    database.inbox_read_back().remove_for_message(message_id)
}

/// 重新保存 Token 后让 blocked 行回到自动重试，不必逐条点重试。
pub(crate) fn reset_after_token_saved(database: &Database) -> DbResult<usize> {
    database.inbox_read_back().reset_blocked()
}

/// 删除 Token 后 GitHub 的 pending 行写不出去，直接丢弃；blocked 保留，继续提示需要授权。
/// 邮件行用各自的授权码，仓储层按动作限定，不受影响。
pub(crate) fn clear_pending_after_token_removed(database: &Database) -> DbResult<usize> {
    database.inbox_read_back().clear_pending()
}

/// 队列循环：一分钟一轮，启动时立即跑一次。
pub fn start_worker(database: Database, app: tauri::AppHandle) {
    let store: Arc<dyn MailSeenStore> = Arc::new(super::email::ImapSeenStore);
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            let Ok(handled) = process_due(&database, Arc::clone(&store)).await else {
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

/// 处理一轮到期行。两种动作各取各自的批量上限：
/// 邮件批量大是因为一条命令能带很多 UID，GitHub 批量小是因为一条一个请求。
async fn process_due(database: &Database, store: Arc<dyn MailSeenStore>) -> Result<usize, String> {
    let now = Utc::now();
    let stamp = iso(now);
    let read_back = database.inbox_read_back();
    let github = read_back
        .due_action(&stamp, ReadBackAction::GithubRead, GITHUB_BATCH)
        .map_err(|error| error.to_string())?;
    let email = read_back
        .due_action(&stamp, ReadBackAction::EmailSeen, EMAIL_BATCH)
        .map_err(|error| error.to_string())?;

    let mut handled = 0;
    if !github.is_empty() {
        handled += process_github(database, &github, now).await?;
    }
    if !email.is_empty() {
        // IMAP 是阻塞调用，丢进阻塞线程池，别把轮询任务卡住。
        let job_database = database.clone();
        handled += tokio::task::spawn_blocking(move || {
            process_email(&job_database, &email, store.as_ref(), now)
        })
        .await
        .map_err(|error| error.to_string())??;
    }
    Ok(handled)
}

/// GitHub 侧：一条通知一次 PATCH。
///
/// 没有 Token 时整轮跳过，不消耗退避次数——邮件行不受影响，上面已经单独取出来了。
async fn process_github(
    database: &Database,
    entries: &[InboxReadBack],
    now: DateTime<Utc>,
) -> Result<usize, String> {
    let Some(token) = crate::infrastructure::secrets::get_github_token()? else {
        return Ok(0);
    };
    let client =
        GithubClient::new(token, Duration::from_secs(10)).map_err(|error| error.to_string())?;

    let mut handled = 0;
    for entry in entries {
        match client.mark_notification_read(&entry.external_id).await {
            Ok(()) => {
                database
                    .inbox_read_back()
                    .remove(&entry.id)
                    .map_err(|error| error.to_string())?;
            }
            Err(error) => {
                let plan = plan_github_failure(&error, entry.attempts + 1);
                apply_plan(database, entry, plan, &error.to_string(), now)?;
            }
        }
        handled += 1;
    }
    Ok(handled)
}

/// 邮件侧：按（来源，UIDVALIDITY）分组，每个账户一次连接、一条 `STORE`。
fn process_email(
    database: &Database,
    entries: &[InboxReadBack],
    store: &dyn MailSeenStore,
    now: DateTime<Utc>,
) -> Result<usize, String> {
    let mut groups: BTreeMap<(String, u32), Vec<(u32, &InboxReadBack)>> = BTreeMap::new();
    let mut handled = 0;
    for entry in entries {
        let Some((uid, uid_validity)) = email_target(entry) else {
            // 没有可用的 UID：写不出去，留着只会每分钟重试一次。
            remove_entry(database, entry)?;
            handled += 1;
            continue;
        };
        groups
            .entry((entry.source_id.clone(), uid_validity))
            .or_default()
            .push((uid, entry));
    }

    for ((source_id, uid_validity), rows) in groups {
        handled += store_group(database, &source_id, uid_validity, &rows, store, now)?;
    }
    Ok(handled)
}

/// 一条邮件回写需要的两个数字。任一项缺失或越界都算「无法定位这封信」。
fn email_target(entry: &InboxReadBack) -> Option<(u32, u32)> {
    let uid = u32::try_from(entry.uid?).ok()?;
    let uid_validity = u32::try_from(entry.uid_validity?).ok()?;
    Some((uid, uid_validity))
}

fn store_group(
    database: &Database,
    source_id: &str,
    uid_validity: u32,
    rows: &[(u32, &InboxReadBack)],
    store: &dyn MailSeenStore,
    now: DateTime<Utc>,
) -> Result<usize, String> {
    // 账户已删除或配置损坏：重试永远不会成功，直接丢弃这些意图。
    let Some(config) = database
        .inbox_sources()
        .config(source_id)
        .map_err(|error| error.to_string())?
        .as_deref()
        .and_then(EmailConfig::parse)
    else {
        return drop_rows(database, rows);
    };
    let account = EmailAccount::from_config(source_id.to_owned(), config);

    let Some(password) = store.password(source_id)? else {
        // 没有授权码：等用户回设置页填一个，先把账户标成需要处理。
        return apply_to_rows(
            database,
            rows,
            |_| FailurePlan::Block(ReadBackErrorKind::Auth),
            "没有可用的邮箱授权码",
            now,
        );
    };

    let mut uids: Vec<u32> = rows.iter().map(|(uid, _)| *uid).collect();
    uids.sort_unstable();
    uids.dedup();

    match store.store_seen(
        &account,
        &password,
        &uids,
        uid_validity,
        Instant::now() + EMAIL_BUDGET,
    ) {
        Ok(()) => {
            for (_, entry) in rows {
                remove_entry(database, entry)?;
            }
            Ok(rows.len())
        }
        Err(error) => {
            let message = error.to_string();
            let plans: Vec<FailurePlan> = rows
                .iter()
                .map(|(_, entry)| plan_email_failure(&error, entry.attempts + 1))
                .collect();
            let mut handled = 0;
            for ((_, entry), plan) in rows.iter().zip(plans) {
                apply_plan(database, entry, plan, &message, now)?;
                handled += 1;
            }
            Ok(handled)
        }
    }
}

/// 账户不可用时统一丢弃：这些行属于「本地的已读意图」，远端不会再接受。
fn drop_rows(database: &Database, rows: &[(u32, &InboxReadBack)]) -> Result<usize, String> {
    for (_, entry) in rows {
        remove_entry(database, entry)?;
    }
    Ok(rows.len())
}

fn apply_to_rows(
    database: &Database,
    rows: &[(u32, &InboxReadBack)],
    plan: impl Fn(&InboxReadBack) -> FailurePlan,
    message: &str,
    now: DateTime<Utc>,
) -> Result<usize, String> {
    for (_, entry) in rows {
        apply_plan(database, entry, plan(entry), message, now)?;
    }
    Ok(rows.len())
}

fn remove_entry(database: &Database, entry: &InboxReadBack) -> Result<(), String> {
    database
        .inbox_read_back()
        .remove(&entry.id)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// 失败后的处置方式：能重试的排时间，不能重试的删行或转人工。
#[derive(Debug, PartialEq, Eq)]
enum FailurePlan {
    /// 远端对象已不存在，或请求本身不可能成功：删行。
    Drop,
    /// 需要人工介入：保留记录，退出自动重试。
    Block(ReadBackErrorKind),
    /// 可恢复：到点再试。
    Retry {
        delay_seconds: i64,
        kind: ReadBackErrorKind,
    },
}

fn plan_github_failure(error: &GithubError, attempts: u32) -> FailurePlan {
    match error {
        GithubError::Unauthorized | GithubError::MissingNotificationsScope => {
            FailurePlan::Block(ReadBackErrorKind::Auth)
        }
        // 404 说明线程没了；其它 4xx 多为请求本身有问题，重试不会变好。
        GithubError::NotFound | GithubError::Api(_) | GithubError::InvalidRepository(_) => {
            FailurePlan::Drop
        }
        GithubError::RateLimited(reset) => {
            if attempts > MAX_ATTEMPTS {
                FailurePlan::Block(ReadBackErrorKind::Exhausted)
            } else {
                FailurePlan::Retry {
                    delay_seconds: rate_limit_delay(reset.as_deref()),
                    kind: ReadBackErrorKind::RateLimited,
                }
            }
        }
        GithubError::Request(_) | GithubError::Unavailable(_) => {
            if attempts > MAX_ATTEMPTS {
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

/// 邮件侧的失败分类。授权问题停下来等用户，网络问题退避重试，
/// 被服务器拒绝（UID 不存在、编号变了、命令不支持）直接删行。
fn plan_email_failure(error: &EmailError, attempts: u32) -> FailurePlan {
    match error {
        // 授权码或文件夹问题：等用户改配置，别每轮重试。
        EmailError::Auth(_) | EmailError::Mailbox(_) => FailurePlan::Block(ReadBackErrorKind::Auth),
        // `Rejected` 是本模块主动判定的「不成立」，`Protocol` 是服务器回了看不懂的东西：
        // 两者重试都不会变好，宁可少标一封。
        EmailError::Rejected(_) | EmailError::Protocol(_) => FailurePlan::Drop,
        EmailError::Unreachable(_) => {
            if attempts > MAX_ATTEMPTS {
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

fn apply_plan(
    database: &Database,
    entry: &InboxReadBack,
    plan: FailurePlan,
    message: &str,
    now: DateTime<Utc>,
) -> Result<(), String> {
    let attempts = entry.attempts + 1;
    let repository = database.inbox_read_back();
    let result = match plan {
        FailurePlan::Drop => repository.remove(&entry.id).map(|_| ()),
        FailurePlan::Block(kind) => repository.mark_blocked(&entry.id, attempts, message, kind),
        FailurePlan::Retry {
            delay_seconds,
            kind,
        } => {
            let delay = jittered(delay_seconds, &entry.id, now);
            let next = iso(now + ChronoDuration::seconds(delay));
            repository.mark_retry(&entry.id, attempts, &next, message, kind)
        }
    };
    result.map_err(|error| error.to_string())
}

/// 指数退避：1、2、4、8、16、30 分钟，封顶 30 分钟。
fn backoff_seconds(attempts: u32) -> i64 {
    let factor = 1_i64 << attempts.saturating_sub(1).min(10);
    (BASE_SECONDS * factor).min(MAX_SECONDS)
}

/// 限流优先用远端给的时间：`x-ratelimit-reset` 是 epoch 秒，`retry-after` 是秒数。
fn rate_limit_delay(reset: Option<&str>) -> i64 {
    let Some(value) = reset.and_then(|value| value.parse::<i64>().ok()) else {
        return BASE_SECONDS;
    };
    if value >= 1_000_000_000 {
        (value - Utc::now().timestamp()).clamp(30, MAX_SECONDS)
    } else {
        value.clamp(10, MAX_SECONDS)
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

fn iso(now: DateTime<Utc>) -> String {
    now.to_rfc3339_opts(SecondsFormat::Millis, true)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
        time::Instant,
    };

    use super::{
        FailurePlan, MailSeenStore, ReadBackAction, backoff_seconds, is_notification_id, jittered,
        plan_email_failure, plan_github_failure, process_due, rate_limit_delay,
    };
    use crate::{
        domain::{EmailTls, NewInboxMessage, ReadBackErrorKind},
        infrastructure::{
            db::Database,
            email::{EmailAccount, EmailConfig, EmailError},
            github::GithubError,
        },
    };

    const NOW: &str = "2026-11-26T15:00:00.000Z";

    /// 记录调用参数、按账户返回预置结果的写入器。`password` 也从这里给，
    /// 测试因此完全不碰 Windows 凭据存储。
    #[derive(Default)]
    struct FakeStore {
        passwords: Mutex<HashMap<String, String>>,
        replies: Mutex<HashMap<String, Result<(), EmailError>>>,
        calls: Mutex<Vec<(String, u32, Vec<u32>)>>,
    }

    impl FakeStore {
        fn password(&self, source_id: &str, password: &str) {
            self.passwords
                .lock()
                .expect("密码表锁")
                .insert(source_id.to_owned(), password.to_owned());
        }

        fn reply(&self, source_id: &str, reply: Result<(), EmailError>) {
            self.replies
                .lock()
                .expect("应答表锁")
                .insert(source_id.to_owned(), reply);
        }

        fn calls(&self) -> Vec<(String, u32, Vec<u32>)> {
            self.calls.lock().expect("调用表锁").clone()
        }
    }

    impl MailSeenStore for FakeStore {
        fn password(&self, source_id: &str) -> Result<Option<String>, String> {
            Ok(self
                .passwords
                .lock()
                .expect("密码表锁")
                .get(source_id)
                .cloned())
        }

        fn store_seen(
            &self,
            account: &EmailAccount,
            _password: &str,
            uids: &[u32],
            uid_validity: u32,
            _deadline: Instant,
        ) -> Result<(), EmailError> {
            self.calls.lock().expect("调用表锁").push((
                account.id.clone(),
                uid_validity,
                uids.to_vec(),
            ));
            self.replies
                .lock()
                .expect("应答表锁")
                .get(&account.id)
                .cloned()
                .unwrap_or(Ok(()))
        }
    }

    fn email_account(database: &Database, address: &str) -> EmailAccount {
        let config = EmailConfig::new(address, ("imap.example.com", 993, EmailTls::Implicit), None);
        let config_json = config.to_json();
        let account = EmailAccount::new(config);
        database
            .inbox_sources()
            .upsert_email(&account.id, account.display_name(), &config_json)
            .expect("写入邮箱账户");
        account
    }

    /// 造一封带 UID 元数据的邮件，与生产环境 `payload_json` 的形状一致。
    fn ingest_mail(
        database: &Database,
        source_id: &str,
        uid: u64,
        uid_validity: u64,
        external_id: &str,
    ) -> String {
        database
            .inbox_messages()
            .ingest(NewInboxMessage {
                source_id: source_id.to_owned(),
                external_id: external_id.to_owned(),
                external_updated_at: Some(NOW.to_owned()),
                category: "mail".to_owned(),
                title: format!("邮件 {uid}"),
                summary: None,
                body: Some("正文".to_owned()),
                url: None,
                actor: None,
                occurred_at: Some(NOW.to_owned()),
                related_kind: None,
                related_id: None,
                payload_json: Some(format!(r#"{{"uid":{uid},"uidValidity":{uid_validity}}}"#)),
            })
            .expect("写入邮件")
            .message
            .id
    }

    fn mark_read(database: &Database, id: &str) {
        crate::application::inbox::mark_read(database, id, true).expect("标已读");
    }

    fn run(database: &Database, store: Arc<FakeStore>) {
        let result = tauri::async_runtime::block_on(process_due(database, store));
        result.expect("一轮回写不应报错");
    }

    #[test]
    fn a_queued_mail_is_written_back_and_the_row_disappears() {
        let database = Database::in_memory().expect("内存库");
        let account = email_account(&database, "me@qq.com");
        let store = Arc::new(FakeStore::default());
        store.password(&account.id, "授权码");
        let first = ingest_mail(&database, &account.id, 41, 7, "a@example.com");
        let second = ingest_mail(&database, &account.id, 42, 7, "b@example.com");
        mark_read(&database, &first);
        mark_read(&database, &second);

        run(&database, Arc::clone(&store));

        let calls = store.calls();
        assert_eq!(calls.len(), 1, "同一账户一轮只连一次");
        assert_eq!(calls[0].0, account.id);
        assert_eq!(calls[0].1, 7, "UIDVALIDITY 一起带过去做守卫");
        assert_eq!(calls[0].2, vec![41, 42], "同一批 UID 合并成一条命令");
        let status = database.inbox_read_back().status().expect("队列状态");
        assert_eq!((status.pending, status.blocked), (0, 0), "写成功后删行");
    }

    #[test]
    fn a_changed_uid_validity_drops_the_intent_instead_of_marking_the_wrong_mail() {
        let database = Database::in_memory().expect("内存库");
        let account = email_account(&database, "me@qq.com");
        let store = Arc::new(FakeStore::default());
        store.password(&account.id, "授权码");
        // 服务器重排过 UID：写入时按「编号已变化」拒绝。
        store.reply(
            &account.id,
            Err(EmailError::Rejected("邮箱编号已变化".into())),
        );
        let id = ingest_mail(&database, &account.id, 41, 7, "a@example.com");
        mark_read(&database, &id);

        run(&database, store);

        let status = database.inbox_read_back().status().expect("队列状态");
        assert_eq!(
            (status.pending, status.blocked),
            (0, 0),
            "重试也不会变好，直接丢弃"
        );
    }

    #[test]
    fn an_auth_failure_blocks_the_queue_until_the_user_retries() {
        let database = Database::in_memory().expect("内存库");
        let account = email_account(&database, "me@qq.com");
        let store = Arc::new(FakeStore::default());
        store.password(&account.id, "过期授权码");
        store.reply(&account.id, Err(EmailError::Auth("授权码无效".into())));
        let id = ingest_mail(&database, &account.id, 41, 7, "a@example.com");
        mark_read(&database, &id);

        run(&database, Arc::clone(&store));

        let status = database.inbox_read_back().status().expect("队列状态");
        assert_eq!((status.pending, status.blocked), (0, 1));
        assert!(
            status.last_error.unwrap_or_default().contains("授权码"),
            "错误原文要能指向下一步"
        );
        assert!(
            database
                .inbox_read_back()
                .has_blocked(ReadBackAction::EmailSeen)
                .expect("查询 blocked")
        );

        // 用户改好授权码后手动重试：blocked 回到 pending。
        super::reset_after_token_saved(&database).expect("重置 blocked");
        assert_eq!(
            database.inbox_read_back().status().expect("状态").pending,
            1
        );
    }

    #[test]
    fn a_network_failure_is_scheduled_for_retry() {
        let database = Database::in_memory().expect("内存库");
        let account = email_account(&database, "me@qq.com");
        let store = Arc::new(FakeStore::default());
        store.password(&account.id, "授权码");
        store.reply(&account.id, Err(EmailError::Unreachable("超时".into())));
        let id = ingest_mail(&database, &account.id, 41, 7, "a@example.com");
        mark_read(&database, &id);

        run(&database, store);

        let status = database.inbox_read_back().status().expect("队列状态");
        assert_eq!(
            (status.pending, status.blocked),
            (1, 0),
            "网络问题留在队列里等下一轮"
        );
        assert!(
            database
                .inbox_read_back()
                .due("2000-01-01T00:00:00Z", 10)
                .expect("到期行")
                .is_empty(),
            "退避时间在未来，当前时刻不该被取到"
        );
    }

    #[test]
    fn a_missing_password_blocks_instead_of_dropping() {
        let database = Database::in_memory().expect("内存库");
        let account = email_account(&database, "me@qq.com");
        let store = Arc::new(FakeStore::default());
        let id = ingest_mail(&database, &account.id, 41, 7, "a@example.com");
        mark_read(&database, &id);

        run(&database, Arc::clone(&store));

        assert!(store.calls().is_empty(), "没有授权码就不该发起连接");
        let status = database.inbox_read_back().status().expect("队列状态");
        assert_eq!((status.pending, status.blocked), (0, 1));
    }

    #[test]
    fn one_failing_account_does_not_stop_another() {
        let database = Database::in_memory().expect("内存库");
        let broken = email_account(&database, "old@163.com");
        let healthy = email_account(&database, "me@qq.com");
        let store = Arc::new(FakeStore::default());
        store.password(&broken.id, "旧授权码");
        store.reply(&broken.id, Err(EmailError::Auth("授权码失效".into())));
        store.password(&healthy.id, "授权码");
        let first = ingest_mail(&database, &broken.id, 1, 1, "a@example.com");
        let second = ingest_mail(&database, &healthy.id, 2, 1, "b@example.com");
        mark_read(&database, &first);
        mark_read(&database, &second);

        run(&database, Arc::clone(&store));

        let status = database.inbox_read_back().status().expect("队列状态");
        assert_eq!(
            (status.pending, status.blocked),
            (0, 1),
            "坏账户停下来等人处理，好账户的意图已经写完"
        );
        assert_eq!(store.calls().len(), 2, "两个账户各连一次，互不牵连");
        assert!(
            store.calls().iter().any(|call| call.0 == healthy.id),
            "好账户必须被写出去"
        );
    }

    #[test]
    fn rows_without_a_uid_are_dropped() {
        let database = Database::in_memory().expect("内存库");
        let account = email_account(&database, "me@qq.com");
        let store = Arc::new(FakeStore::default());
        store.password(&account.id, "授权码");
        // 历史数据没有 uid / uidValidity：入队时就被拦住了，这里直接确认不入队。
        let id = database
            .inbox_messages()
            .ingest(NewInboxMessage {
                source_id: account.id.clone(),
                external_id: "legacy@example.com".into(),
                external_updated_at: Some(NOW.into()),
                category: "mail".into(),
                title: "历史邮件".into(),
                summary: None,
                body: None,
                url: None,
                actor: None,
                occurred_at: Some(NOW.into()),
                related_kind: None,
                related_id: None,
                payload_json: None,
            })
            .expect("写入邮件")
            .message
            .id;
        mark_read(&database, &id);

        run(&database, Arc::clone(&store));

        assert!(store.calls().is_empty());
        assert_eq!(
            database.inbox_read_back().status().expect("状态").pending,
            0
        );
        assert!(
            !database
                .inbox_read_back()
                .has_blocked(ReadBackAction::EmailSeen)
                .expect("查询 blocked")
        );
    }

    #[test]
    fn github_failures_choose_between_retry_drop_and_block() {
        assert_eq!(
            plan_github_failure(&GithubError::Request("超时".into()), 1),
            FailurePlan::Retry {
                delay_seconds: 60,
                kind: ReadBackErrorKind::Transient
            }
        );
        assert_eq!(
            plan_github_failure(&GithubError::Unavailable("HTTP 503".into()), 7),
            FailurePlan::Block(ReadBackErrorKind::Exhausted),
            "backoff gives up after the sixth retry"
        );

        // 认证与权限问题重试没有意义，等重新授权。
        assert_eq!(
            plan_github_failure(&GithubError::Unauthorized, 1),
            FailurePlan::Block(ReadBackErrorKind::Auth)
        );
        assert_eq!(
            plan_github_failure(&GithubError::MissingNotificationsScope, 1),
            FailurePlan::Block(ReadBackErrorKind::Auth)
        );

        // 线程没了、请求本身有问题：删行，不重试。
        assert_eq!(
            plan_github_failure(&GithubError::NotFound, 1),
            FailurePlan::Drop
        );
        assert_eq!(
            plan_github_failure(&GithubError::Api("HTTP 422".into()), 1),
            FailurePlan::Drop
        );

        assert_eq!(
            plan_github_failure(&GithubError::RateLimited(Some("120".into())), 1),
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
    fn mail_failures_choose_between_retry_drop_and_block() {
        assert_eq!(
            plan_email_failure(&EmailError::Unreachable("超时".into()), 1),
            FailurePlan::Retry {
                delay_seconds: 60,
                kind: ReadBackErrorKind::Transient
            }
        );
        assert_eq!(
            plan_email_failure(&EmailError::Unreachable("超时".into()), 7),
            FailurePlan::Block(ReadBackErrorKind::Exhausted)
        );
        assert_eq!(
            plan_email_failure(&EmailError::Auth("授权码".into()), 1),
            FailurePlan::Block(ReadBackErrorKind::Auth)
        );
        assert_eq!(
            plan_email_failure(&EmailError::Mailbox("INBOX".into()), 1),
            FailurePlan::Block(ReadBackErrorKind::Auth)
        );
        assert_eq!(
            plan_email_failure(&EmailError::Rejected("编号变了".into()), 1),
            FailurePlan::Drop
        );
        assert_eq!(
            plan_email_failure(&EmailError::Protocol("看不懂的响应".into()), 1),
            FailurePlan::Drop
        );
        assert_eq!(backoff_seconds(2), 120);
        assert_eq!(backoff_seconds(6), 1800);
        assert!(is_notification_id("4242"));
        assert!(!is_notification_id("thread-1"));

        let now = chrono::Utc::now();
        for seed in ["a", "bb", "ccc"] {
            let delay = jittered(600, seed, now);
            assert!((480..=720).contains(&delay));
        }
    }
}
