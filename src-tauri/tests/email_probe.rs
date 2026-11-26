//! IMAP 连通性探针（S0）。
//!
//! 不是常规测试，默认 `#[ignore]`，只在手工联调时运行。它验证四件事：
//! 1. native-tls 能完成握手并通过证书校验；
//! 2. 账号能登录；
//! 3. 163 系是否需要 `ID`（RFC 2971）才能 `EXAMINE`；
//! 4. `EXAMINE` 能只读打开邮箱，并且能取到一封邮件头供 `mail-parser` 解析。
//!
//! 之所以要分两个场景：`imap 2.4.1` 依赖的 `imap-proto 0.10.2` 不认识 `* ID (...)`
//! 这种未标记响应，`run_command_and_read_response` 解析失败后不会消费掉随后的
//! `aN OK`，导致下一条命令的 tag 断言 panic。因此 `ID` 只能在客户端自己读原始行时发送，
//! 发完再交给 `imap::Client::new` 接管。
//!
//! 用法（在仓库根目录）：
//!
//! ```powershell
//! $env:TOMATODESK_IMAP_HOST = "imap.163.com"
//! $env:TOMATODESK_IMAP_USER = "you@163.com"
//! $env:TOMATODESK_IMAP_PASS = "授权码"
//! cargo test --manifest-path src-tauri\Cargo.toml probe_imap -- --ignored --nocapture
//! ```
//!
//! 不带 `USER` / `PASS` 时只验证 TLS 握手与服务器问候，用于确认网络可达。
//! 密码只从环境变量读取，不打印、不落盘。

use std::{
    env,
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    time::Duration,
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
const IO_TIMEOUT: Duration = Duration::from_secs(15);

type TlsStream = native_tls::TlsStream<TcpStream>;

fn env_or(name: &str, default: &str) -> String {
    env::var(name).unwrap_or_else(|_| default.to_owned())
}

/// 建一条带超时的 TLS 连接。超时是必须的：没有读超时时，服务器不回话会把测试挂死。
fn connect_tls(host: &str, port: u16) -> Option<TlsStream> {
    let address = (host, port)
        .to_socket_addrs()
        .ok()?
        .next()
        .expect("没有可用的服务器地址");
    let tcp = match TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) {
        Ok(stream) => stream,
        Err(error) => {
            println!("TCP 不可达（{error}），跳过；确认网络或代理后重跑");
            return None;
        }
    };
    tcp.set_read_timeout(Some(IO_TIMEOUT)).expect("设置读超时");
    tcp.set_write_timeout(Some(IO_TIMEOUT)).expect("设置写超时");
    println!("TCP 可达");

    let tls = native_tls::TlsConnector::new().expect("构建 TLS 连接器");
    let stream = tls
        .connect(host, tcp)
        .unwrap_or_else(|error| panic!("TLS 握手失败：{error}"));
    println!("TLS 握手正常");
    Some(stream)
}

/// 逐字节读到 CRLF 为止。必须逐字节：套一层 `BufReader` 会把后面的响应也预读走，
/// 交还给 `imap::Client` 时就丢了。
fn read_line(stream: &mut TlsStream) -> std::io::Result<String> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        stream.read_exact(&mut byte)?;
        if byte[0] == b'\n' {
            break;
        }
        if byte[0] != b'\r' {
            line.push(byte[0]);
        }
        // 服务器不该发超长行；这是防御性上限，避免坏数据把循环拖死。
        if line.len() > 64 * 1024 {
            break;
        }
    }
    Ok(String::from_utf8_lossy(&line).into_owned())
}

/// 读到指定 tag 的状态行为止，返回（状态行，之前的未标记响应）。
fn read_until_tagged(stream: &mut TlsStream, tag: &str) -> std::io::Result<(String, Vec<String>)> {
    let prefix = format!("{tag} ");
    let mut untagged = Vec::new();
    loop {
        let line = read_line(stream)?;
        if line.starts_with(&prefix) {
            return Ok((line, untagged));
        }
        untagged.push(line);
    }
}

/// 场景 B 的握手：自己读问候、自己发 `ID`，再交给 `imap::Client` 接管后续命令。
///
/// `Client::new` 的 tag 从 `a1` 开始，所以这里用 `a0`，不会撞号。
fn open_with_raw_id(host: &str, port: u16) -> Option<imap::Client<TlsStream>> {
    let mut stream = connect_tls(host, port)?;

    let greeting = read_line(&mut stream).expect("读取服务器问候");
    println!("问候：{greeting}");

    stream
        .write_all(b"a0 ID (\"name\" \"TomatoDesk\" \"version\" \"probe\")\r\n")
        .expect("发送 ID");
    stream.flush().expect("刷新缓冲");

    let (status, untagged) = read_until_tagged(&mut stream, "a0").expect("读取 ID 响应");
    for line in &untagged {
        println!("ID 未标记响应：{line}");
    }
    println!("ID 状态行：{status}");

    // 问候已消费，这里不能再调 read_greeting（它 assert 只能读一次）。
    Some(imap::Client::new(stream))
}

/// 登录后打印能力、EXAMINE 并解析首封邮件头。
fn inspect(session: &mut imap::Session<TlsStream>, mailbox: &str) {
    match session.capabilities() {
        Ok(capabilities) => {
            let mut names: Vec<String> = capabilities
                .iter()
                .map(|capability| format!("{capability:?}"))
                .collect();
            names.sort();
            println!("能力：{}", names.join(" "));
        }
        Err(error) => println!("读取能力失败（不中断）：{error}"),
    }

    let info = match session.examine(mailbox) {
        Ok(info) => info,
        Err(error) => {
            println!("EXAMINE {mailbox} 失败：{error}");
            return;
        }
    };
    println!(
        "EXAMINE {mailbox}：{} 封，UIDVALIDITY={:?}，UIDNEXT={:?}",
        info.exists, info.uid_validity, info.uid_next
    );

    if info.exists == 0 {
        println!("邮箱为空，跳过取信");
        return;
    }

    // 只取邮件头，且用 PEEK，避免把邮件在服务器上标成已读。
    let fetches = match session.fetch("1", "(UID BODY.PEEK[HEADER])") {
        Ok(fetches) => fetches,
        Err(error) => {
            println!("取邮件头失败：{error}");
            return;
        }
    };
    match fetches.iter().find_map(|fetch| fetch.header()) {
        Some(header) => {
            let parsed = mail_parser::MessageParser::new()
                .parse(header)
                .expect("mail-parser 解析邮件头");
            println!(
                "首封主题：{:?}，Message-ID：{:?}",
                parsed.subject(),
                parsed.message_id()
            );
        }
        None => println!("服务器没有返回邮件头"),
    }
}

#[test]
#[ignore = "手工联调：需要真实邮箱与环境变量，默认不跑"]
fn probe_imap() {
    let host = env_or("TOMATODESK_IMAP_HOST", "imap.qq.com");
    let port: u16 = env_or("TOMATODESK_IMAP_PORT", "993")
        .parse()
        .expect("TOMATODESK_IMAP_PORT 必须是数字");
    let mailbox = env_or("TOMATODESK_IMAP_MAILBOX", "INBOX");
    let user = env::var("TOMATODESK_IMAP_USER").ok();
    let password = env::var("TOMATODESK_IMAP_PASS").ok();

    println!("目标：{host}:{port}");

    println!("\n=== 场景 B：先发 ID，再登录 ===");
    if let Some(client) = open_with_raw_id(&host, port) {
        match (user.as_deref(), password.as_deref()) {
            (Some(user), Some(password)) => match client.login(user, password) {
                Ok(mut session) => {
                    println!("登录成功");
                    inspect(&mut session, &mailbox);
                    let _ = session.logout();
                }
                Err((error, _)) => println!("登录失败：{error}"),
            },
            _ => println!("未提供账号，跳过登录"),
        }
    }

    println!("\n=== 场景 A：不发 ID，直接登录并 EXAMINE ===");
    if let Some(stream) = connect_tls(&host, port) {
        let mut client = imap::Client::new(stream);
        println!(
            "问候：{}",
            String::from_utf8_lossy(&client.read_greeting().expect("读取服务器问候")).trim()
        );
        match (user.as_deref(), password.as_deref()) {
            (Some(user), Some(password)) => match client.login(user, password) {
                Ok(mut session) => {
                    println!("登录成功");
                    inspect(&mut session, &mailbox);
                    let _ = session.logout();
                }
                Err((error, _)) => println!("登录失败：{error}"),
            },
            _ => println!("未提供账号，跳过登录"),
        }
    }

    println!("\n探针完成");
}

/// 不依赖网络的依赖验证：`full_encoding` 必须能解 GBK 编码的 RFC 2047 主题。
/// GBK 下「测试」是 B2 E2 CA D4，Base64 为 suLK1A==；UTF-8 下为 5rWL6K+V。
#[test]
fn mail_parser_decodes_gbk_and_utf8_subjects() {
    let gbk = b"Subject: =?GBK?B?suLK1A==?=\r\nMessage-ID: <gbk@example.com>\r\n\r\n";
    let message = mail_parser::MessageParser::new()
        .parse(gbk)
        .expect("解析 GBK 主题");
    assert_eq!(message.subject(), Some("测试"));
    assert_eq!(message.message_id(), Some("gbk@example.com"));

    let utf8 = b"Subject: =?UTF-8?B?5rWL6K+V?=\r\n\r\n";
    let message = mail_parser::MessageParser::new()
        .parse(utf8)
        .expect("解析 UTF-8 主题");
    assert_eq!(message.subject(), Some("测试"));
}
