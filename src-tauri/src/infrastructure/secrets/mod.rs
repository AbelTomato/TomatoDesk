use keyring::Entry;

/// 凭据服务名随构建区分：开发进程读写的是自己的命名空间，
/// 不会覆盖或删除生产 Token。
#[cfg(debug_assertions)]
const SERVICE: &str = "TomatoDesk Dev";
#[cfg(not(debug_assertions))]
const SERVICE: &str = "TomatoDesk";
const GITHUB_TOKEN_USER: &str = "github-token";

/// 邮箱凭据按来源 id 一键一条，与 `inbox_sources.id` 对齐，多账户互不覆盖。
const EMAIL_PASSWORD_PREFIX: &str = "email-password:";

fn entry(user: &str) -> Result<Entry, String> {
    Entry::new(SERVICE, user).map_err(|error| format!("无法打开 Windows 凭据存储：{error}"))
}

fn email_password_user(source_id: &str) -> String {
    format!("{EMAIL_PASSWORD_PREFIX}{source_id}")
}

fn save_secret(user: &str, value: &str, label: &str) -> Result<(), String> {
    entry(user)?
        .set_password(value)
        .map_err(|error| format!("无法保存 {label}：{error}"))
}

fn get_secret(user: &str, label: &str) -> Result<Option<String>, String> {
    match entry(user)?.get_password() {
        Ok(value) if !value.trim().is_empty() => Ok(Some(value)),
        Ok(_) => Ok(None),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(format!("无法读取 {label}：{error}")),
    }
}

fn delete_secret(user: &str, label: &str) -> Result<(), String> {
    match entry(user)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(format!("无法删除 {label}：{error}")),
    }
}

pub fn save_github_token(token: &str) -> Result<(), String> {
    let token = token.trim();
    if token.is_empty() {
        return Err("GitHub Token 不能为空".into());
    }
    save_secret(GITHUB_TOKEN_USER, token, "GitHub Token")
}

pub fn get_github_token() -> Result<Option<String>, String> {
    get_secret(GITHUB_TOKEN_USER, "GitHub Token")
}

pub fn delete_github_token() -> Result<(), String> {
    delete_secret(GITHUB_TOKEN_USER, "GitHub Token")
}

/// 存邮箱授权码 / 应用专用密码。只去掉首尾空白：部分服务商的授权码内部含空格。
pub fn save_email_password(source_id: &str, password: &str) -> Result<(), String> {
    if source_id.trim().is_empty() {
        return Err("邮箱来源 id 不能为空".into());
    }
    let password = password.trim();
    if password.is_empty() {
        return Err("邮箱授权码不能为空".into());
    }
    save_secret(&email_password_user(source_id), password, "邮箱授权码")
}

pub fn get_email_password(source_id: &str) -> Result<Option<String>, String> {
    if source_id.trim().is_empty() {
        return Ok(None);
    }
    get_secret(&email_password_user(source_id), "邮箱授权码")
}

pub fn delete_email_password(source_id: &str) -> Result<(), String> {
    if source_id.trim().is_empty() {
        return Ok(());
    }
    delete_secret(&email_password_user(source_id), "邮箱授权码")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 只验证键名派生，不碰 Windows 凭据存储：真实读写会污染用户环境。
    #[test]
    fn email_password_key_is_namespaced_by_source() {
        assert_eq!(
            email_password_user("source-email-abc"),
            "email-password:source-email-abc"
        );
        assert_ne!(
            email_password_user("source-email-a"),
            email_password_user("source-email-b")
        );
    }
}
