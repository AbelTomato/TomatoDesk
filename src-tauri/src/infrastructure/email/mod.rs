//! 邮箱连接器的领域无关部分：账户标识、非敏感配置、错误分类与原文解析。
//!
//! `client` 负责 IMAP 会话与取信，`parse` 负责 MIME 解析；两者都不碰数据库。

pub mod client;
pub mod parse;

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::EmailTls;

/// 邮箱来源 id 前缀，与 GitHub 的 `source-github` 区分开。
pub const EMAIL_SOURCE_PREFIX: &str = "source-email-";

/// UUID v5 的命名空间串。固定不变，保证同一账户在任何机器、任何版本都派生出同一个 id。
const NAMESPACE: &str = "tomatodesk:email";

/// 账户数上限。单账户需求就是这个常量取 1，不需要另一套代码。
pub const MAX_EMAIL_ACCOUNTS: i64 = 5;

pub const DEFAULT_MAILBOX: &str = "INBOX";
pub const DEFAULT_INITIAL_DAYS: u32 = 14;
pub const DEFAULT_MAX_MESSAGES_PER_SYNC: u32 = 100;

/// 由「地址 + 主机」派生来源 id。
///
/// 只依赖这两个字段，所以重复保存是更新而不是新增；改地址或主机等于新账户，
/// 已入库消息仍归属原来的来源行。
pub fn account_id(address: &str, host: &str) -> String {
    let key = format!(
        "{NAMESPACE}:{}:{}",
        address.trim().to_lowercase(),
        host.trim().to_lowercase()
    );
    let uuid = Uuid::new_v5(&Uuid::NAMESPACE_URL, key.as_bytes());
    format!("{EMAIL_SOURCE_PREFIX}{uuid}")
}

pub fn is_email_account_id(id: &str) -> bool {
    id.starts_with(EMAIL_SOURCE_PREFIX) && Uuid::parse_str(&id[EMAIL_SOURCE_PREFIX.len()..]).is_ok()
}

/// 写进 `inbox_sources.config_json` 的非敏感配置。密码不在这里。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailConfig {
    pub address: String,
    pub host: String,
    pub port: u16,
    pub tls: EmailTls,
    pub mailbox: String,
    #[serde(default = "default_initial_days")]
    pub initial_days: u32,
    #[serde(default = "default_max_messages")]
    pub max_messages_per_sync: u32,
}

fn default_initial_days() -> u32 {
    DEFAULT_INITIAL_DAYS
}

fn default_max_messages() -> u32 {
    DEFAULT_MAX_MESSAGES_PER_SYNC
}

impl EmailConfig {
    pub fn new(address: &str, server: (&str, u16, EmailTls), mailbox: Option<&str>) -> Self {
        let (host, port, tls) = server;
        Self {
            address: address.trim().to_owned(),
            host: host.trim().to_lowercase(),
            port,
            tls,
            mailbox: mailbox
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .unwrap_or(DEFAULT_MAILBOX)
                .to_owned(),
            initial_days: DEFAULT_INITIAL_DAYS,
            max_messages_per_sync: DEFAULT_MAX_MESSAGES_PER_SYNC,
        }
    }

    /// 配置损坏时返回 `None`：调用方按「未配置」处理并跳过该账户，而不是让整轮同步失败。
    pub fn parse(raw: &str) -> Option<Self> {
        let config: Self = serde_json::from_str(raw).ok()?;
        if config.address.trim().is_empty() || config.host.trim().is_empty() || config.port == 0 {
            return None;
        }
        Some(config)
    }

    /// 序列化失败只可能是内存数据结构问题，退化成 `{}` 让该账户被判定为未配置。
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_owned())
    }
}

/// 运行时账户：来源 id + 配置。`EmailAccount` 只由 `EmailConfig` 构造，避免两处字段漂移。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmailAccount {
    pub id: String,
    pub config: EmailConfig,
}

impl EmailAccount {
    pub fn new(config: EmailConfig) -> Self {
        let id = account_id(&config.address, &config.host);
        Self { id, config }
    }

    pub fn from_config(id: impl Into<String>, config: EmailConfig) -> Self {
        Self {
            id: id.into(),
            config,
        }
    }

    pub fn address(&self) -> &str {
        &self.config.address
    }

    pub fn host(&self) -> &str {
        &self.config.host
    }

    pub fn port(&self) -> u16 {
        self.config.port
    }

    pub fn tls(&self) -> EmailTls {
        self.config.tls
    }

    pub fn mailbox(&self) -> &str {
        &self.config.mailbox
    }

    /// 来源名用地址本身，收件箱的来源筛选里一眼能对上账号。
    pub fn display_name(&self) -> &str {
        &self.config.address
    }
}

/// 同步错误的分类只影响重试策略与文案，不携带协议栈细节。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmailError {
    /// DNS、TCP、TLS 失败：网络不通、端口写错、证书不受信。
    Unreachable(String),
    /// 认证失败：授权码错、未开启 IMAP、服务器只支持 OAuth2。
    Auth(String),
    /// 文件夹不存在或不可只读打开。
    Mailbox(String),
    /// 协议异常：非预期响应、超长行、状态行格式错误。
    Protocol(String),
    /// 服务器拒绝了这次写入：UID 不存在、文件夹不可写、命令不被支持。
    /// 重试不会变好，已经入库的消息只是没标上已读。
    Rejected(String),
}

impl EmailError {
    /// 163 / 126 在没收到 `ID` 时用这句话拒绝 `EXAMINE`，与授权码无关，
    /// 单独识别避免误导用户去改密码。
    pub fn is_unsafe_login(detail: &str) -> bool {
        detail.to_lowercase().contains("unsafe login")
    }

    /// 认证类错误值得退避 60 分钟：反复用错授权码重试会触发服务商风控。
    pub fn needs_backoff(&self) -> bool {
        matches!(self, Self::Auth(_) | Self::Mailbox(_))
    }
}

impl fmt::Display for EmailError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreachable(detail) => write!(formatter, "无法连接邮件服务器：{detail}"),
            Self::Auth(detail) => {
                write!(
                    formatter,
                    "登录失败，请确认已开启 IMAP 并使用授权码（{detail}）"
                )
            }
            Self::Mailbox(detail) => write!(formatter, "无法打开邮件文件夹：{detail}"),
            Self::Protocol(detail) => write!(formatter, "邮件服务器返回异常响应：{detail}"),
            Self::Rejected(detail) => write!(formatter, "邮件服务器没有接受这次操作：{detail}"),
        }
    }
}

impl std::error::Error for EmailError {}

impl From<EmailError> for String {
    fn from(error: EmailError) -> Self {
        error.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_id_is_deterministic_and_normalized() {
        let first = account_id("Me@QQ.com", "imap.qq.com");
        let second = account_id("  me@qq.com ", "IMAP.QQ.COM");
        assert_eq!(first, second);
        assert!(first.starts_with(EMAIL_SOURCE_PREFIX));
        assert!(is_email_account_id(&first));
    }

    #[test]
    fn account_id_differs_by_host_and_address() {
        let base = account_id("me@qq.com", "imap.qq.com");
        assert_ne!(base, account_id("me@qq.com", "imap.163.com"));
        assert_ne!(base, account_id("other@qq.com", "imap.qq.com"));
    }

    #[test]
    fn account_id_rejects_foreign_ids() {
        assert!(!is_email_account_id("source-github"));
        assert!(!is_email_account_id("source-email-"));
        assert!(!is_email_account_id("source-email-not-a-uuid"));
    }

    #[test]
    fn config_round_trips_through_json() {
        let config = EmailConfig::new(
            "me@qq.com",
            ("imap.qq.com", 993, EmailTls::Implicit),
            Some("INBOX"),
        );
        let parsed = EmailConfig::parse(&config.to_json()).expect("配置应能解析");
        assert_eq!(parsed, config);
        assert_eq!(parsed.initial_days, DEFAULT_INITIAL_DAYS);
        assert_eq!(parsed.max_messages_per_sync, DEFAULT_MAX_MESSAGES_PER_SYNC);
    }

    #[test]
    fn config_defaults_mailbox_and_rejects_broken_json() {
        let config = EmailConfig::new(
            "me@163.com",
            ("imap.163.com", 143, EmailTls::Starttls),
            Some("   "),
        );
        assert_eq!(config.mailbox, DEFAULT_MAILBOX);

        assert!(EmailConfig::parse("{}").is_none());
        assert!(EmailConfig::parse("not json").is_none());
        assert!(
            EmailConfig::parse(
                r#"{"address":"","host":"h","port":993,"tls":"implicit","mailbox":"INBOX"}"#
            )
            .is_none()
        );
    }

    #[test]
    fn unsafe_login_is_recognized_case_insensitively() {
        assert!(EmailError::is_unsafe_login(
            "No Response: EXAMINE Unsafe Login. Please contact kefu@188.com for help"
        ));
        assert!(!EmailError::is_unsafe_login("AUTHENTICATIONFAILED"));
    }

    #[test]
    fn auth_and_mailbox_errors_need_backoff() {
        assert!(EmailError::Auth("x".into()).needs_backoff());
        assert!(EmailError::Mailbox("x".into()).needs_backoff());
        assert!(!EmailError::Protocol("x".into()).needs_backoff());
        assert!(!EmailError::Unreachable("x".into()).needs_backoff());
    }
}
