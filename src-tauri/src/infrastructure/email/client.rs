//! IMAP 会话：建连、认证、能力探测与只读取信。
//!
//! 这一层只把服务器上的邮件变成 [`ParsedMail`]，不认识数据库，也不认识
//! `NewInboxMessage`；业务映射与入库留在 `application::inbox::email`。
//!
//! 连接构造沿用 S0 探针验证过的路径：自己建 TCP（带超时）→ TLS → 自己读问候 →
//! 登录前发 `ID` → 交给 `imap::Client` 接管。两个原因：
//!
//! 1. `imap-proto 0.10.2` 解析不了 RFC 2971 的 `* ID (...)`，`run_command_and_read_response`
//!    失败后残留的状态行会让下一条命令的 tag 断言 panic；
//! 2. `imap::connect` 不提供超时，后台轮询遇到半开连接会永久挂住。
//!
//! 因此所有 pre-auth 命令都在 `Client` 接管前由本模块自己读写，tag 用 `a0` / `s0`，
//! 避开 `Client` 从 `a1` 起的计数器。

use std::{
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    time::{Duration, Instant},
};

use chrono::{Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    domain::{EmailAccountProbe, EmailTls},
    infrastructure::email::{
        EmailAccount, EmailError,
        parse::{MailServerMeta, ParsedMail, parse_message},
    },
};

/// TCP 连接超时。后台线程里不能无限等，8 秒足够区分「慢」和「不通」。
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
/// 单条命令的读写超时。IMAP 服务器不会长时间只发一半响应，15 秒足够。
const IO_TIMEOUT: Duration = Duration::from_secs(15);
/// 单行响应的防御性上限，避免坏数据把读取循环拖死。
const MAX_LINE_BYTES: usize = 64 * 1024;

/// 一次 `UID FETCH` 取几封。
///
/// 分批是为了约束原始 MIME 的峰值内存：`BODY.PEEK[]<0.262144>` 这种 partial fetch
/// 会让 `imap-proto 0.10.2` 解析失败（只认单数字的 `<n>`），只能整封取回再本地截断。
const FETCH_CHUNK: usize = 20;

/// `ID` 命令的 tag。`Client::new` 的命令计数器从 `a1` 开始，这里用 `a0` 不会撞号。
const ID_TAG: &str = "a0";
/// `STARTTLS` 的 tag。必须与 `ID` 区分开：两条命令都在 `Client` 接管前发出。
const STARTTLS_TAG: &str = "s0";

const FETCH_QUERY: &str = "(UID RFC822.SIZE INTERNALDATE BODY.PEEK[])";

type Stream = native_tls::TlsStream<TcpStream>;
type Session = imap::Session<Stream>;

/// 每个账户一条游标，存在 `inbox_sources.cursor`。
///
/// `uid_validity` 与服务器当前值不一致时整条作废：UID 会被重新编号，
/// `last_uid` 在新编号下没有意义。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MailCursor {
    pub uid_validity: u32,
    pub last_uid: u32,
}

impl MailCursor {
    /// 读不出 / 字段缺失 / 类型不符 / UID 为 0 一律视为「没有游标」，走首次窗口。
    pub fn parse(raw: Option<&str>) -> Option<Self> {
        let raw = raw?.trim();
        if raw.is_empty() {
            return None;
        }
        let cursor: Self = serde_json::from_str(raw).ok()?;
        (cursor.last_uid > 0).then_some(cursor)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_owned())
    }
}

/// 一次取信的结果：解析好的邮件 + 该账户下一轮的游标。
#[derive(Debug, Clone)]
pub struct FetchOutcome {
    /// `EXAMINE` 返回的 EXISTS，用于「测试连接」展示。
    pub mailbox_total: u32,
    pub mails: Vec<ParsedMail>,
    pub next_cursor: MailCursor,
    /// 服务器返回的封数（含解析不出正文被跳过的）。
    pub fetched_count: usize,
    /// 取到但没有正文分段、只能跳过的封数。
    pub skipped_count: usize,
}

/// 只连接与验证，不落库。「测试连接」用。
pub fn probe(account: &EmailAccount, password: &str) -> Result<EmailAccountProbe, EmailError> {
    let mut session = open_session(account, password)?;
    let mailbox = examine(&mut session, account)?;
    let probe = EmailAccountProbe {
        mailbox_total: mailbox.exists,
        uid_validity: mailbox.uid_validity.unwrap_or(0),
    };
    let _ = session.logout();
    Ok(probe)
}

/// 拉取一个账户的新邮件。整体阻塞，调用方负责丢进阻塞线程池。
///
/// `deadline` 由调用方给出（整轮预算与单账户预算取小）；到点后不再开新的一批，
/// 游标只推进到已经取回的最大 UID，剩下的下一轮继续。
pub fn fetch_new(
    account: &EmailAccount,
    password: &str,
    cursor: Option<&MailCursor>,
    deadline: Instant,
) -> Result<FetchOutcome, EmailError> {
    let mut session = open_session(account, password)?;
    let mailbox = examine(&mut session, account)?;
    let uid_validity = mailbox.uid_validity.unwrap_or(0);
    let limit = account.config.max_messages_per_sync as usize;

    // UIDVALIDITY 变了说明服务器重排过 UID，旧游标作废，按首次窗口重扫。
    let resumed = cursor.filter(|cursor| cursor.uid_validity == uid_validity);
    let mut uids = match resumed {
        Some(cursor) => incremental_uids(&mut session, cursor.last_uid)?,
        None => first_window_uids(&mut session, account.config.initial_days)?,
    };
    uids.sort_unstable();
    let selected = select_batch(uids, limit, resumed.is_some());

    let mut high_water = resumed.map_or(0, |cursor| cursor.last_uid);
    let mut mails = Vec::with_capacity(selected.len());
    let mut skipped_count = 0usize;

    for chunk in selected.chunks(FETCH_CHUNK) {
        if Instant::now() >= deadline {
            break;
        }
        let set = chunk
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let fetches = session
            .uid_fetch(&set, FETCH_QUERY)
            .map_err(|error| map_error(&error))?;
        for fetch in fetches.iter() {
            let Some(uid) = fetch.uid else {
                continue;
            };
            // 服务器可能多返回（例如区间退化成最后一封），只认自己点名的 UID。
            if !chunk.contains(&uid) {
                continue;
            }
            let Some(raw) = fetch.body() else {
                skipped_count += 1;
                continue;
            };
            let size = fetch
                .size
                .unwrap_or_else(|| u32::try_from(raw.len()).unwrap_or(u32::MAX));
            let mut parsed = parse_message(raw);
            parsed.server = MailServerMeta {
                uid,
                uid_validity,
                size_bytes: size,
            };
            // `Date` 头缺失或非法时用 INTERNALDATE 兜底，保证每封信的时间取值稳定。
            if parsed.date_unix.is_none() {
                parsed.date_unix = fetch
                    .internal_date()
                    .map(|moment| moment.timestamp())
                    .filter(|seconds| *seconds > 0);
            }
            mails.push(parsed);
        }
        // 这一批已经全部取回，游标可以安全越过它。
        if let Some(last) = chunk.last() {
            high_water = high_water.max(*last);
        }
    }

    let _ = session.logout();
    Ok(FetchOutcome {
        mailbox_total: mailbox.exists,
        fetched_count: mails.len() + skipped_count,
        skipped_count,
        mails,
        next_cursor: MailCursor {
            uid_validity,
            last_uid: high_water,
        },
    })
}

/// 首次窗口：`SINCE <今天 - initial_days>`。
fn first_window_uids(session: &mut Session, days: u32) -> Result<Vec<u32>, EmailError> {
    let query = format!("SINCE {}", search_date(days));
    let found = session
        .uid_search(query)
        .map_err(|error| map_error(&error))?;
    Ok(found.into_iter().collect())
}

/// 增量：`UID <last>:*` 之后再按 `uid > last` 过滤。
///
/// 区间陷阱：`n:*` 在 `n` 大于当前最大 UID 时仍会匹配到最后一封，光看区间会重复入库。
fn incremental_uids(session: &mut Session, last_uid: u32) -> Result<Vec<u32>, EmailError> {
    let query = format!("UID {last_uid}:*");
    let found = session
        .uid_search(query)
        .map_err(|error| map_error(&error))?;
    Ok(found.into_iter().filter(|uid| *uid > last_uid).collect())
}

/// IMAP 的 `SINCE` 只接受 `<日>-<英文月>-<年>`，必须由代码生成，不能拼用户输入。
fn search_date(days: u32) -> String {
    let since = Utc::now() - ChronoDuration::days(i64::from(days));
    since.format("%d-%b-%Y").to_string()
}

/// 从排好序的候选 UID 里挑这一轮要处理的。
///
/// - 增量：取最旧的若干封，保证不跳号；
/// - 首次：取最新的若干封，首屏不出现一堆陈年旧信（更早的部分不再补拉，见方案 5.4）。
fn select_batch(mut uids: Vec<u32>, limit: usize, resuming: bool) -> Vec<u32> {
    if limit == 0 {
        return Vec::new();
    }
    if resuming {
        uids.truncate(limit);
        uids
    } else if uids.len() > limit {
        uids.split_off(uids.len() - limit)
    } else {
        uids
    }
}

fn open_session(account: &EmailAccount, password: &str) -> Result<Session, EmailError> {
    let stream = connect(account)?;
    // 问候已被自己消费，`login` 不检查 `greeting_read`，这里不需要再读一次。
    let client = imap::Client::new(stream);
    login(client, account.address(), password)
}

/// 建连并按需发 `ID`，返回已经握好手、还没登录的流。
fn connect(account: &EmailAccount) -> Result<Stream, EmailError> {
    let address = (account.host(), account.port())
        .to_socket_addrs()
        .map_err(|error| EmailError::Unreachable(format!("无法解析 {}：{error}", account.host())))?
        .next()
        .ok_or_else(|| EmailError::Unreachable(format!("{} 没有可用地址", account.host())))?;

    let tcp = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT).map_err(|error| {
        EmailError::Unreachable(format!(
            "无法连接 {}:{}（{error}）",
            account.host(),
            account.port()
        ))
    })?;
    tcp.set_read_timeout(Some(IO_TIMEOUT))
        .and_then(|()| tcp.set_write_timeout(Some(IO_TIMEOUT)))
        .map_err(io_error)?;
    let _ = tcp.set_nodelay(true);

    let mut stream = match account.tls() {
        EmailTls::Implicit => {
            let mut stream = tls_handshake(account.host(), tcp)?;
            read_greeting(&mut stream)?;
            stream
        }
        EmailTls::Starttls => upgrade_starttls(account.host(), tcp)?,
    };
    send_id(&mut stream)?;
    Ok(stream)
}

/// 143 明文建连 → `STARTTLS` → 在同一连接上升级 TLS。
fn upgrade_starttls(host: &str, mut tcp: TcpStream) -> Result<Stream, EmailError> {
    read_greeting(&mut tcp)?;
    write_all(&mut tcp, &format!("{STARTTLS_TAG} STARTTLS\r\n"))?;
    let (status, _) = read_until_tagged(&mut tcp, STARTTLS_TAG)?;
    if !is_ok_status(&status) {
        return Err(EmailError::Unreachable(format!(
            "{host} 拒绝 STARTTLS：{status}"
        )));
    }
    // 升级后服务器不会重发问候，客户端需要重新取能力；能力只影响可选命令，这里直接登录。
    tls_handshake(host, tcp)
}

fn tls_handshake(host: &str, tcp: TcpStream) -> Result<Stream, EmailError> {
    let connector = native_tls::TlsConnector::new()
        .map_err(|error| EmailError::Unreachable(format!("无法初始化 TLS 连接器：{error}")))?;
    connector.connect(host, tcp).map_err(|error| {
        EmailError::Unreachable(format!("TLS 握手失败（{error}），请确认端口与加密方式"))
    })
}

/// RFC 2971 的 `ID`，在 `LOGIN` 之前无条件发送。
///
/// 163 / 126 不发 `ID` 时 `EXAMINE` 会直接返回 `Unsafe Login`；Gmail、QQ、iCloud
/// 不认识这条命令，回 `BAD`，忽略即可。关键是把自己的状态行读干净，不能留给下一条命令。
fn send_id(stream: &mut Stream) -> Result<(), EmailError> {
    let command = format!(
        "{ID_TAG} ID (\"name\" \"TomatoDesk\" \"version\" \"{}\")\r\n",
        env!("CARGO_PKG_VERSION")
    );
    write_all(stream, &command)?;
    read_until_tagged(stream, ID_TAG)?;
    Ok(())
}

fn login(client: imap::Client<Stream>, user: &str, password: &str) -> Result<Session, EmailError> {
    let client = match client.login(user, password) {
        Ok(session) => return Ok(session),
        Err((error, client)) => {
            // 只在「服务器不认 LOGIN 这条命令」时回退 PLAIN；凭证被拒时重试没有意义，
            // 还可能触发服务商风控。
            if !matches!(error, imap::Error::Bad(_)) {
                return Err(login_error(&error));
            }
            client
        }
    };
    match client.authenticate("PLAIN", &PlainAuth { user, password }) {
        Ok(session) => Ok(session),
        Err((error, _)) => Err(login_error(&error)),
    }
}

/// `AUTHENTICATE PLAIN` 的应答：`\0user\0password`，由 `imap` 负责 base64。
struct PlainAuth<'a> {
    user: &'a str,
    password: &'a str,
}

impl imap::Authenticator for PlainAuth<'_> {
    type Response = Vec<u8>;

    fn process(&self, _challenge: &[u8]) -> Self::Response {
        let mut response = Vec::with_capacity(self.user.len() + self.password.len() + 2);
        response.push(0);
        response.extend_from_slice(self.user.as_bytes());
        response.push(0);
        response.extend_from_slice(self.password.as_bytes());
        response
    }
}

fn examine(
    session: &mut Session,
    account: &EmailAccount,
) -> Result<imap::types::Mailbox, EmailError> {
    session
        .examine(account.mailbox())
        .map_err(|error| mailbox_error(&error, account.mailbox()))
}

/// 登录阶段的失败基本都可以归到「认证」：文案已经提示检查授权码与 IMAP 开关。
fn login_error(error: &imap::Error) -> EmailError {
    match map_error(error) {
        EmailError::Protocol(detail) => EmailError::Auth(detail),
        other => other,
    }
}

/// `EXAMINE` 阶段的 `NO` / `BAD` 是文件夹问题，除了 163 的 `Unsafe Login`
/// 那种「服务器拒绝客户端」的情况。
fn mailbox_error(error: &imap::Error, mailbox: &str) -> EmailError {
    match error {
        imap::Error::No(detail) | imap::Error::Bad(detail)
            if !EmailError::is_unsafe_login(detail) =>
        {
            EmailError::Mailbox(format!("{mailbox}：{detail}"))
        }
        other => map_error(other),
    }
}

fn map_error(error: &imap::Error) -> EmailError {
    match error {
        imap::Error::Io(io) => io_error_ref(io),
        imap::Error::TlsHandshake(_) | imap::Error::Tls(_) => {
            EmailError::Unreachable("TLS 握手失败，请确认端口与加密方式".to_owned())
        }
        imap::Error::ConnectionLost => EmailError::Unreachable("服务器中断了连接".to_owned()),
        imap::Error::Bad(detail) | imap::Error::No(detail) => classify(detail),
        imap::Error::Parse(_) | imap::Error::Validate(_) | imap::Error::Append => {
            EmailError::Protocol(error.to_string())
        }
        _ => EmailError::Protocol(error.to_string()),
    }
}

/// 按服务器原文分类：能确定是凭证问题就归 `Auth`，其余算协议异常。
fn classify(detail: &str) -> EmailError {
    if EmailError::is_unsafe_login(detail) {
        return EmailError::Auth(format!("{detail}（服务器拒绝了本次客户端登录）"));
    }
    let lower = detail.to_lowercase();
    let rejected = [
        "authenticationfailed",
        "authentication failed",
        "invalid credentials",
        "invalid username or password",
        "login fail",
    ]
    .iter()
    .any(|marker| lower.contains(marker));
    if rejected {
        EmailError::Auth(detail.to_owned())
    } else {
        EmailError::Protocol(detail.to_owned())
    }
}

fn io_error(error: std::io::Error) -> EmailError {
    io_error_ref(&error)
}

fn io_error_ref(error: &std::io::Error) -> EmailError {
    EmailError::Unreachable(match error.kind() {
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => {
            "服务器响应超时".to_owned()
        }
        _ => error.to_string(),
    })
}

/// 逐字节读到 CRLF。不能套 `BufReader`：预读会把后续响应吃进缓冲区，交还给
/// `imap::Client` 时就丢了。
fn read_line(stream: &mut impl Read) -> Result<String, EmailError> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        stream.read_exact(&mut byte).map_err(io_error)?;
        if byte[0] == b'\n' {
            break;
        }
        if byte[0] != b'\r' {
            line.push(byte[0]);
        }
        if line.len() > MAX_LINE_BYTES {
            return Err(EmailError::Protocol("服务器返回了超长响应行".to_owned()));
        }
    }
    Ok(String::from_utf8_lossy(&line).into_owned())
}

/// 读到指定 tag 的状态行为止，返回（状态行，之前的未标记响应）。
fn read_until_tagged(
    stream: &mut impl Read,
    tag: &str,
) -> Result<(String, Vec<String>), EmailError> {
    let prefix = format!("{tag} ");
    let mut untagged = Vec::new();
    loop {
        let line = read_line(stream)?;
        if line.starts_with(&prefix) {
            return Ok((line, untagged));
        }
        if line.starts_with("* BYE") {
            return Err(EmailError::Unreachable(format!("服务器关闭了连接：{line}")));
        }
        untagged.push(line);
    }
}

fn read_greeting(stream: &mut impl Read) -> Result<(), EmailError> {
    let greeting = read_line(stream)?;
    let upper = greeting.to_uppercase();
    if upper.starts_with("* OK") || upper.starts_with("* PREAUTH") {
        return Ok(());
    }
    Err(EmailError::Unreachable(format!(
        "服务器没有返回 IMAP 问候（{greeting}），请确认端口与加密方式"
    )))
}

fn write_all(stream: &mut impl Write, text: &str) -> Result<(), EmailError> {
    stream.write_all(text.as_bytes()).map_err(io_error)?;
    stream.flush().map_err(io_error)
}

/// 状态行形如 `a0 OK ID completed`，第二个词是状态码。
fn is_ok_status(status: &str) -> bool {
    status
        .split_whitespace()
        .nth(1)
        .is_some_and(|code| code.eq_ignore_ascii_case("OK"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_round_trips_and_rejects_broken_json() {
        let cursor = MailCursor {
            uid_validity: 1234567890,
            last_uid: 4521,
        };
        assert_eq!(
            MailCursor::parse(Some(&cursor.to_json())),
            Some(cursor),
            "游标应能往返"
        );
        assert!(cursor.to_json().contains("\"uidValidity\":1234567890"));
        assert_eq!(MailCursor::parse(Some("not json")), None);
        assert_eq!(MailCursor::parse(Some("{}")), None);
        assert_eq!(MailCursor::parse(Some("   ")), None);
        assert_eq!(MailCursor::parse(None), None);
        // UID 0 不是合法 UID，按无游标处理，否则会从 0 开始重扫。
        assert_eq!(
            MailCursor::parse(Some(r#"{"uidValidity":1,"lastUid":0}"#)),
            None
        );
    }

    #[test]
    fn first_sync_keeps_the_newest_batch() {
        let uids: Vec<u32> = (1..=250).collect();
        let selected = select_batch(uids, 100, false);
        assert_eq!(selected.len(), 100);
        assert_eq!(*selected.first().unwrap(), 151);
        assert_eq!(*selected.last().unwrap(), 250);
    }

    #[test]
    fn incremental_sync_keeps_the_oldest_batch() {
        let uids: Vec<u32> = (101..=350).collect();
        let selected = select_batch(uids, 100, true);
        assert_eq!(selected.len(), 100);
        assert_eq!(selected.first().copied(), Some(101));
        assert_eq!(selected.last().copied(), Some(200));
    }

    #[test]
    fn batches_smaller_than_the_limit_are_untouched() {
        assert_eq!(select_batch(vec![7, 8], 100, false), vec![7, 8]);
        assert_eq!(select_batch(vec![7, 8], 100, true), vec![7, 8]);
        assert!(select_batch(vec![1, 2], 0, false).is_empty());
    }

    #[test]
    fn search_date_uses_the_imap_date_format() {
        let date = search_date(14);
        assert_eq!(date.len(), 11, "形如 13-Nov-2026：{date}");
        assert!(date.as_bytes()[2] == b'-' && date.as_bytes()[6] == b'-');
        assert!(date[3..6].chars().all(|ch| ch.is_ascii_alphabetic()));
        assert!(date[7..].chars().all(|ch| ch.is_ascii_digit()));
    }

    #[test]
    fn auth_failures_are_classified_as_credentials() {
        assert!(matches!(
            classify("AUTHENTICATIONFAILED Authentication failed"),
            EmailError::Auth(_)
        ));
        assert!(matches!(
            classify("No Response: EXAMINE Unsafe Login. Please contact kefu@188.com"),
            EmailError::Auth(_)
        ));
        assert!(matches!(
            classify("Mailbox does not exist"),
            EmailError::Protocol(_)
        ));
    }

    #[test]
    fn login_failures_never_surface_as_protocol_errors() {
        // 登录被拒时统一按认证问题呈现，文案已经提示检查授权码与 IMAP 开关。
        assert!(matches!(
            login_error(&imap::Error::Bad("LOGIN not supported".into())),
            EmailError::Auth(_)
        ));
        assert!(matches!(
            login_error(&imap::Error::ConnectionLost),
            EmailError::Unreachable(_)
        ));
    }

    #[test]
    fn mailbox_errors_keep_the_folder_name() {
        let error = mailbox_error(&imap::Error::No("Mailbox does not exist".into()), "INBOX");
        assert!(matches!(error, EmailError::Mailbox(_)));
        assert!(error.to_string().contains("INBOX"));

        // 163 的 Unsafe Login 是客户端被拒，不是文件夹不存在。
        let error = mailbox_error(&imap::Error::No("Unsafe Login".into()), "INBOX");
        assert!(matches!(error, EmailError::Auth(_)));
    }

    #[test]
    fn status_lines_are_read_case_insensitively() {
        assert!(is_ok_status("a0 OK ID completed"));
        assert!(is_ok_status("a0 ok"));
        assert!(!is_ok_status("a0 BAD Unknown command"));
        assert!(!is_ok_status(""));
    }
}
