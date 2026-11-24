use keyring::Entry;

const SERVICE: &str = "TomatoDesk";
const USER: &str = "github-token";

pub fn save_github_token(token: &str) -> Result<(), String> {
    let token = token.trim();
    if token.is_empty() {
        return Err("GitHub Token 不能为空".into());
    }
    Entry::new(SERVICE, USER)
        .map_err(|error| format!("无法打开 Windows 凭据存储：{error}"))?
        .set_password(token)
        .map_err(|error| format!("无法保存 GitHub Token：{error}"))
}

pub fn get_github_token() -> Result<Option<String>, String> {
    match Entry::new(SERVICE, USER)
        .map_err(|error| format!("无法打开 Windows 凭据存储：{error}"))?
        .get_password()
    {
        Ok(token) if !token.trim().is_empty() => Ok(Some(token)),
        Ok(_) => Ok(None),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(format!("无法读取 GitHub Token：{error}")),
    }
}

pub fn delete_github_token() -> Result<(), String> {
    match Entry::new(SERVICE, USER)
        .map_err(|error| format!("无法打开 Windows 凭据存储：{error}"))?
        .delete_credential()
    {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(format!("无法删除 GitHub Token：{error}")),
    }
}
