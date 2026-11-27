//! 邮箱账户的 IPC 命令：列表、测试连接、保存、删除。
//!
//! 命令层只做三件事：校验入参、把凭据读写与试连抽成可替换的接口、把结果映射成
//! `CommandResult`。同步由后台轮询负责，这里最多在保存后补跑一次该账户。

use tauri::{AppHandle, State};

use crate::{
    application::inbox::email::{ImapProber, MailProber},
    domain::{EmailAccountProbe, EmailAccountStatus, InboxSourceKind},
    infrastructure::{
        db::Database,
        email::{EmailAccount, EmailConfig, MAX_EMAIL_ACCOUNTS, is_email_account_id},
        secrets,
    },
};

use super::{
    dto::{EmailAccountForm, EmailAccountInput, IdInput, parse_input},
    error::{CommandError, CommandResult},
};

/// 授权码的读写入口。抽成 trait 是为了让命令层测试不碰真实 Windows 凭据管理器。
pub(super) trait EmailCredentials: Send + Sync {
    fn load(&self, source_id: &str) -> Result<Option<String>, String>;
    fn save(&self, source_id: &str, password: &str) -> Result<(), String>;
    fn delete(&self, source_id: &str) -> Result<(), String>;
}

pub(super) struct KeyringCredentials;

impl EmailCredentials for KeyringCredentials {
    fn load(&self, source_id: &str) -> Result<Option<String>, String> {
        secrets::get_email_password(source_id)
    }

    fn save(&self, source_id: &str, password: &str) -> Result<(), String> {
        secrets::save_email_password(source_id, password)
    }

    fn delete(&self, source_id: &str) -> Result<(), String> {
        secrets::delete_email_password(source_id)
    }
}

/// 设置页的账户列表。已删除的账户 `config_json` 是 `{}`，解析不出来就自然不在列表里，
/// 但来源行与历史消息仍然保留。
pub(super) fn list_email_accounts_in(
    database: &Database,
) -> CommandResult<Vec<EmailAccountStatus>> {
    let rows = database
        .inbox_sources()
        .list_email()
        .map_err(CommandError::from)?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let config = EmailConfig::parse(&row.config_json)?;
            Some(status_of(
                row.source.id,
                &config,
                row.source.enabled,
                row.source.last_synced_at,
                row.source.last_error,
            ))
        })
        .collect())
}

/// 只连接验证，不落库；授权码留空时沿用该账户已保存的凭据。
pub(super) fn test_email_account_in(
    credentials: &dyn EmailCredentials,
    prober: &dyn MailProber,
    input: EmailAccountInput,
) -> CommandResult<EmailAccountProbe> {
    let form = input.validate()?;
    let (account, password) = resolve_account(credentials, form)?;
    Ok(prober.probe(&account, &password)?)
}

/// 保存账户：校验 → 派生来源 id → 上限检查 → 试连 → 写凭据 → upsert 来源。
///
/// 试连放在写库之前，连接失败时既不留半套配置，也不会用错授权码覆盖掉能用的旧值。
pub(super) fn save_email_account_in(
    database: &Database,
    prober: &dyn MailProber,
    credentials: &dyn EmailCredentials,
    input: EmailAccountInput,
) -> CommandResult<EmailAccountStatus> {
    let form = input.validate()?;
    let (account, password) = resolve_account(credentials, form)?;
    ensure_capacity(database, &account)?;
    prober.probe(&account, &password)?;

    credentials
        .save(&account.id, &password)
        .map_err(CommandError::internal)?;
    let source = database
        .inbox_sources()
        .upsert_email(
            &account.id,
            account.display_name(),
            &account.config.to_json(),
        )
        .map_err(CommandError::from)?;

    Ok(status_of(
        source.id,
        &account.config,
        source.enabled,
        source.last_synced_at,
        source.last_error,
    ))
}

/// 删除账户 = 软停用：删凭据、清配置与游标、`enabled = 0`，**不删来源行**，
/// 否则 `inbox_messages` 的 `ON DELETE CASCADE` 会把历史邮件一起清掉。
pub(super) fn delete_email_account_in(
    database: &Database,
    credentials: &dyn EmailCredentials,
    input: IdInput,
) -> CommandResult<Vec<EmailAccountStatus>> {
    let id = input.validate()?.id;
    if !is_email_account_id(&id) {
        return Err(CommandError::not_found("email account"));
    }
    database
        .inbox_sources()
        .get(&id)
        .map_err(CommandError::from)?
        .filter(|source| source.kind == InboxSourceKind::Email)
        .ok_or_else(|| CommandError::not_found("email account"))?;

    credentials.delete(&id).map_err(CommandError::internal)?;
    let repository = database.inbox_sources();
    repository
        .set_enabled(&id, false)
        .map_err(CommandError::from)?;
    repository.clear_cursor(&id).map_err(CommandError::from)?;
    repository.clear_config(&id).map_err(CommandError::from)?;

    list_email_accounts_in(database)
}

/// 表单 → 运行时账户 + 明文授权码。密码缺省时回落到凭据管理器。
fn resolve_account(
    credentials: &dyn EmailCredentials,
    form: EmailAccountForm,
) -> CommandResult<(EmailAccount, String)> {
    let account = EmailAccount::new(EmailConfig::new(
        &form.address,
        (&form.host, form.port, form.tls),
        form.mailbox.as_deref(),
    ));
    let password = match form.password {
        Some(password) => password,
        None => credentials
            .load(&account.id)
            .map_err(CommandError::internal)?
            .ok_or_else(|| CommandError::validation("请填写授权码或应用专用密码"))?,
    };
    Ok((account, password))
}

/// 上限只数启用中的账户；同一账户重复保存不算新增。
fn ensure_capacity(database: &Database, account: &EmailAccount) -> CommandResult<()> {
    let already_active = database
        .inbox_sources()
        .get(&account.id)
        .map_err(CommandError::from)?
        .is_some_and(|source| source.enabled);
    if already_active {
        return Ok(());
    }

    let count = database
        .inbox_sources()
        .count_configured_email()
        .map_err(CommandError::from)?;
    if count >= MAX_EMAIL_ACCOUNTS {
        return Err(CommandError::validation(format!(
            "邮箱账户最多 {MAX_EMAIL_ACCOUNTS} 个，请先删除一个再添加"
        )));
    }
    Ok(())
}

fn status_of(
    id: String,
    config: &EmailConfig,
    enabled: bool,
    last_synced_at: Option<String>,
    last_error: Option<String>,
) -> EmailAccountStatus {
    EmailAccountStatus {
        id,
        address: config.address.clone(),
        host: config.host.clone(),
        port: config.port,
        tls: config.tls,
        mailbox: config.mailbox.clone(),
        enabled,
        last_synced_at,
        last_error,
    }
}

/// 阻塞的网络与数据库操作丢到阻塞线程池，避免占住异步运行时。
async fn run_blocking<T, F>(job: F) -> CommandResult<T>
where
    F: FnOnce() -> CommandResult<T> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(job)
        .await
        .map_err(|_| CommandError::internal("邮件任务异常终止"))?
}

#[tauri::command]
pub fn list_email_accounts(
    database: State<'_, Database>,
) -> CommandResult<Vec<EmailAccountStatus>> {
    list_email_accounts_in(&database)
}

#[tauri::command]
pub async fn test_email_account(
    input: Option<serde_json::Value>,
) -> CommandResult<EmailAccountProbe> {
    let input = parse_input::<EmailAccountInput>(input)?;
    run_blocking(move || test_email_account_in(&KeyringCredentials, &ImapProber, input)).await
}

#[tauri::command]
pub async fn save_email_account(
    app: AppHandle,
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<EmailAccountStatus> {
    let input = parse_input::<EmailAccountInput>(input)?;
    let job_database = database.inner().clone();
    let status = run_blocking(move || {
        save_email_account_in(&job_database, &ImapProber, &KeyringCredentials, input)
    })
    .await?;

    // 保存即刻跑一次该账户的同步，首屏不用等下一轮十分钟轮询。
    crate::application::inbox::email::sync_account_soon(
        database.inner().clone(),
        app,
        status.id.clone(),
    );
    Ok(status)
}

#[tauri::command]
pub fn delete_email_account(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<Vec<EmailAccountStatus>> {
    let input = parse_input::<IdInput>(input)?;
    delete_email_account_in(&database, &KeyringCredentials, input)
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Mutex};

    use super::*;
    use crate::{
        commands::error::CommandErrorCode,
        domain::{EmailTls, NewInboxMessage},
        infrastructure::email::EmailError,
    };

    #[derive(Default)]
    struct FakeCredentials {
        store: Mutex<HashMap<String, String>>,
    }

    impl EmailCredentials for FakeCredentials {
        fn load(&self, source_id: &str) -> Result<Option<String>, String> {
            Ok(self.store.lock().unwrap().get(source_id).cloned())
        }

        fn save(&self, source_id: &str, password: &str) -> Result<(), String> {
            self.store
                .lock()
                .unwrap()
                .insert(source_id.to_owned(), password.to_owned());
            Ok(())
        }

        fn delete(&self, source_id: &str) -> Result<(), String> {
            self.store.lock().unwrap().remove(source_id);
            Ok(())
        }
    }

    struct FakeProber {
        attempts: Mutex<Vec<(String, String)>>,
        failure: Option<EmailError>,
    }

    impl Default for FakeProber {
        fn default() -> Self {
            Self {
                attempts: Mutex::new(Vec::new()),
                failure: None,
            }
        }
    }

    impl MailProber for FakeProber {
        fn probe(
            &self,
            account: &EmailAccount,
            password: &str,
        ) -> Result<EmailAccountProbe, EmailError> {
            self.attempts
                .lock()
                .unwrap()
                .push((account.id.clone(), password.to_owned()));
            match &self.failure {
                Some(error) => Err(error.clone()),
                None => Ok(EmailAccountProbe {
                    mailbox_total: 3,
                    uid_validity: 9,
                }),
            }
        }
    }

    fn input(
        address: &str,
        host: &str,
        password: Option<&str>,
        port: Option<u16>,
    ) -> EmailAccountInput {
        EmailAccountInput {
            address: address.into(),
            password: password.map(ToOwned::to_owned),
            host: host.into(),
            port,
            tls: None,
            mailbox: None,
        }
    }

    fn sample_message(source_id: &str) -> NewInboxMessage {
        NewInboxMessage {
            source_id: source_id.into(),
            external_id: "uid:1:42".into(),
            external_updated_at: Some("2026-11-27T01:00:00.000Z".into()),
            category: "mail".into(),
            title: "季度数据".into(),
            summary: Some("附件里有数字".into()),
            body: None,
            url: None,
            actor: Some("ann@example.com".into()),
            occurred_at: Some("2026-11-27T01:00:00.000Z".into()),
            related_kind: None,
            related_id: None,
            payload_json: None,
        }
    }

    #[test]
    fn invalid_fields_are_rejected_before_any_network_call() {
        let cases = [
            input("", "imap.qq.com", Some("code"), None),
            input("me", "imap.qq.com", Some("code"), None),
            input("me @qq.com", "imap.qq.com", Some("code"), None),
            input("me@qq.com", "", Some("code"), None),
            input("me@qq.com", "imap://imap.qq.com", Some("code"), None),
            input("me@qq.com", "imap.qq.com:993", Some("code"), None),
            input("me@qq.com", "imap.qq.com", Some("code"), Some(0)),
        ];
        for case in cases {
            let error = case.validate().unwrap_err();
            assert_eq!(
                error.code,
                CommandErrorCode::Validation,
                "{}",
                error.message
            );
        }

        let mut mailbox = input("me@qq.com", "imap.qq.com", Some("code"), None);
        mailbox.mailbox = Some("IN\nBOX".into());
        assert_eq!(
            mailbox.validate().unwrap_err().code,
            CommandErrorCode::Validation
        );
    }

    #[test]
    fn port_defaults_follow_the_tls_mode() {
        let implicit = input("me@qq.com", "imap.qq.com", Some("code"), None);
        assert_eq!(implicit.validate().unwrap().port, 993);

        let mut starttls = input("me@qq.com", "imap.qq.com", Some("code"), None);
        starttls.tls = Some(EmailTls::Starttls);
        assert_eq!(starttls.validate().unwrap().port, 143);
    }

    #[test]
    fn saving_the_same_account_twice_reuses_one_row() {
        let database = Database::in_memory().unwrap();
        let credentials = FakeCredentials::default();
        let prober = FakeProber::default();

        let first = save_email_account_in(
            &database,
            &prober,
            &credentials,
            input("Me@QQ.com", "imap.qq.com", Some("code"), None),
        )
        .unwrap();
        let second = save_email_account_in(
            &database,
            &prober,
            &credentials,
            input("me@qq.com", "IMAP.QQ.COM", None, Some(143)),
        )
        .unwrap();

        assert_eq!(first.id, second.id);
        assert_eq!(second.port, 143);
        assert_eq!(list_email_accounts_in(&database).unwrap(), vec![second]);
    }

    #[test]
    fn the_sixth_account_is_rejected_until_one_is_deleted() {
        let database = Database::in_memory().unwrap();
        let credentials = FakeCredentials::default();
        let prober = FakeProber::default();

        let mut ids = Vec::new();
        for index in 0..MAX_EMAIL_ACCOUNTS {
            let saved = save_email_account_in(
                &database,
                &prober,
                &credentials,
                input(
                    &format!("user{index}@qq.com"),
                    "imap.qq.com",
                    Some("code"),
                    None,
                ),
            )
            .unwrap();
            ids.push(saved.id);
        }

        let error = save_email_account_in(
            &database,
            &prober,
            &credentials,
            input("sixth@qq.com", "imap.qq.com", Some("code"), None),
        )
        .unwrap_err();
        assert_eq!(error.code, CommandErrorCode::Validation);
        assert!(error.message.contains('5'));

        let remaining =
            delete_email_account_in(&database, &credentials, IdInput { id: ids[0].clone() })
                .unwrap();
        assert_eq!(remaining.len(), MAX_EMAIL_ACCOUNTS as usize - 1);

        assert!(
            save_email_account_in(
                &database,
                &prober,
                &credentials,
                input("sixth@qq.com", "imap.qq.com", Some("code"), None),
            )
            .is_ok()
        );
    }

    #[test]
    fn deleting_an_account_keeps_its_messages_and_hides_it_from_the_list() {
        let database = Database::in_memory().unwrap();
        let credentials = FakeCredentials::default();
        let prober = FakeProber::default();

        let saved = save_email_account_in(
            &database,
            &prober,
            &credentials,
            input("me@163.com", "imap.163.com", Some("code"), None),
        )
        .unwrap();
        let message = database
            .inbox_messages()
            .ingest(sample_message(&saved.id))
            .unwrap();

        assert_eq!(list_email_accounts_in(&database).unwrap().len(), 1);

        let remaining = delete_email_account_in(
            &database,
            &credentials,
            IdInput {
                id: saved.id.clone(),
            },
        )
        .unwrap();
        assert!(remaining.is_empty());

        // 来源行还在，但已停用且配置被清空；历史消息不受影响。
        let rows = database.inbox_sources().list_email().unwrap();
        assert_eq!(rows.len(), 1);
        assert!(!rows[0].source.enabled);
        assert_eq!(rows[0].config_json, "{}");
        assert!(
            database
                .inbox_messages()
                .get(&message.id)
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn account_status_never_carries_the_password() {
        let database = Database::in_memory().unwrap();
        let credentials = FakeCredentials::default();
        let prober = FakeProber::default();
        save_email_account_in(
            &database,
            &prober,
            &credentials,
            input("me@qq.com", "imap.qq.com", Some("super-secret"), None),
        )
        .unwrap();

        let payload = serde_json::to_string(&list_email_accounts_in(&database).unwrap()).unwrap();
        assert!(!payload.contains("super-secret"));
        assert!(!payload.contains("password"));
    }

    #[test]
    fn probing_without_a_stored_password_is_a_validation_error() {
        let credentials = FakeCredentials::default();
        let prober = FakeProber::default();
        let error = test_email_account_in(
            &credentials,
            &prober,
            input("me@qq.com", "imap.qq.com", None, None),
        )
        .unwrap_err();
        assert_eq!(error.code, CommandErrorCode::Validation);
        assert!(prober.attempts.lock().unwrap().is_empty());
    }

    #[test]
    fn authentication_failures_map_to_validation() {
        let database = Database::in_memory().unwrap();
        let credentials = FakeCredentials::default();
        let prober = FakeProber {
            failure: Some(EmailError::Auth("AUTHENTICATIONFAILED".into())),
            ..Default::default()
        };

        let error = save_email_account_in(
            &database,
            &prober,
            &credentials,
            input("me@qq.com", "imap.qq.com", Some("wrong"), None),
        )
        .unwrap_err();
        assert_eq!(error.code, CommandErrorCode::Validation);
        assert!(error.message.contains("授权码"));
        // 试连失败不落库。
        assert!(list_email_accounts_in(&database).unwrap().is_empty());
        assert!(credentials.store.lock().unwrap().is_empty());
    }

    #[test]
    fn deleting_a_foreign_id_is_not_found() {
        let database = Database::in_memory().unwrap();
        let credentials = FakeCredentials::default();
        for id in ["source-github", "source-email-not-a-uuid"] {
            let error = delete_email_account_in(&database, &credentials, IdInput { id: id.into() })
                .unwrap_err();
            assert_eq!(error.code, CommandErrorCode::NotFound);
        }
    }
}
