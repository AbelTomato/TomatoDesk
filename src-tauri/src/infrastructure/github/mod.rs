use std::time::Duration;

use reqwest::{Client, Response, StatusCode, Url, header};
use serde::de::DeserializeOwned;

const API_ROOT: &str = "https://api.github.com";
const PER_PAGE: &str = "100";

#[derive(Debug)]
pub enum GithubError {
    InvalidRepository(String),
    Request(String),
    RateLimited(Option<String>),
    Unauthorized,
    MissingNotificationsScope,
    NotFound,
    Api(String),
}

impl std::fmt::Display for GithubError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRepository(message) => write!(formatter, "{message}"),
            Self::Request(message) => write!(formatter, "GitHub 网络请求失败：{message}"),
            Self::RateLimited(reset) => match reset {
                Some(reset) => {
                    if let Ok(value) = reset.parse::<i64>() {
                        if value >= 1_000_000_000 {
                            if let Some(timestamp) = chrono::DateTime::from_timestamp(value, 0) {
                                return write!(
                                    formatter,
                                    "GitHub API 已限流，请在 {} 后重试",
                                    timestamp.with_timezone(&chrono::Local).format("%H:%M:%S")
                                );
                            }
                        } else {
                            return write!(formatter, "GitHub API 已限流，请在 {value} 秒后重试");
                        }
                    }
                    write!(formatter, "GitHub API 已限流，请稍后重试（{reset}）")
                }
                None => formatter.write_str("GitHub API 已限流，请稍后重试"),
            },
            Self::Unauthorized => formatter.write_str("GitHub Token 无效或权限不足"),
            Self::MissingNotificationsScope => formatter.write_str(
                "GitHub Token 缺少 notifications 权限：经典 Token 需勾选 notifications，\
                 Fine-grained Token 需授予 Notifications 的只读权限",
            ),
            Self::NotFound => formatter.write_str("GitHub 仓库不存在或当前 Token 无权访问"),
            Self::Api(message) => write!(formatter, "GitHub API 返回错误：{message}"),
        }
    }
}

impl std::error::Error for GithubError {}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct GithubUser {
    pub login: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct GithubViewer {
    pub login: String,
}

/// `/repos/{owner}/{name}` 里展示用得上的字段。
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct GithubRepositoryMeta {
    pub name: String,
    pub full_name: String,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub language: Option<String>,
    pub stargazers_count: Option<i64>,
    pub forks_count: Option<i64>,
    pub open_issues_count: Option<i64>,
    pub default_branch: Option<String>,
    pub pushed_at: Option<String>,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub private: bool,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct GithubRelease {
    pub id: u64,
    pub tag_name: String,
    pub name: Option<String>,
    pub body: Option<String>,
    pub html_url: String,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
    pub published_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct GithubIssue {
    pub id: u64,
    pub number: u64,
    pub title: String,
    pub body: Option<String>,
    pub html_url: String,
    pub state: String,
    pub state_reason: Option<String>,
    pub comments: Option<u64>,
    pub user: Option<GithubUser>,
    pub assignees: Option<Vec<GithubUser>>,
    pub updated_at: String,
    pub created_at: String,
    pub closed_at: Option<String>,
    pub pull_request: Option<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct GithubPull {
    pub id: u64,
    pub number: u64,
    pub title: String,
    pub body: Option<String>,
    pub html_url: String,
    pub state: String,
    #[serde(default)]
    pub draft: bool,
    pub comments: Option<u64>,
    pub additions: Option<i64>,
    pub deletions: Option<i64>,
    pub changed_files: Option<i64>,
    pub user: Option<GithubUser>,
    pub assignees: Option<Vec<GithubUser>>,
    pub requested_reviewers: Option<Vec<GithubUser>>,
    pub created_at: String,
    pub updated_at: String,
    pub closed_at: Option<String>,
    pub merged_at: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct GithubReview {
    pub id: u64,
    pub user: Option<GithubUser>,
    pub state: Option<String>,
    pub submitted_at: Option<String>,
}

/// 对话评论。PR 与 Issue 共用 `/issues/{number}/comments`。
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct GithubComment {
    pub id: u64,
    pub body: Option<String>,
    pub html_url: String,
    pub user: Option<GithubUser>,
    pub created_at: String,
    pub updated_at: String,
}

/// `/notifications` 只返回通知摘要：标题、类型、仓库与最后更新时间。
/// PR / Issue 的正文与评论仍由仓库模块负责，这里不重复抓取。
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct GithubNotification {
    /// 通知线程 id，同时用于 `PATCH /notifications/threads/{id}` 回写已读。
    pub id: serde_json::Value,
    #[serde(default)]
    pub unread: bool,
    pub reason: String,
    pub updated_at: String,
    pub subject: GithubNotificationSubject,
    pub repository: GithubNotificationRepository,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct GithubNotificationSubject {
    pub title: String,
    /// 指向 api.github.com 的资源地址，展示前需要换成 github.com。
    pub url: Option<String>,
    pub latest_comment_url: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct GithubNotificationRepository {
    pub full_name: String,
    pub html_url: String,
}

#[derive(Clone)]
pub struct GithubClient {
    client: Client,
    token: String,
}

impl GithubClient {
    pub fn new(token: String, timeout: Duration) -> Result<Self, GithubError> {
        let client = Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|error| GithubError::Request(error.to_string()))?;
        Ok(Self { client, token })
    }

    /// 当前 Token 所属用户，用于推导 PR / Issue 与我的关系。
    pub async fn viewer(&self) -> Result<GithubViewer, GithubError> {
        self.get_json(endpoint("/user")?).await
    }

    pub async fn repository(
        &self,
        owner: &str,
        name: &str,
    ) -> Result<GithubRepositoryMeta, GithubError> {
        self.get_json(endpoint(&format!("/repos/{owner}/{name}"))?)
            .await
    }

    /// README 原文（Markdown）。仓库没有 README 时返回 `None`。
    pub async fn readme(&self, owner: &str, name: &str) -> Result<Option<String>, GithubError> {
        self.get_raw(endpoint(&format!("/repos/{owner}/{name}/readme"))?)
            .await
    }

    pub async fn releases(
        &self,
        owner: &str,
        name: &str,
    ) -> Result<Vec<GithubRelease>, GithubError> {
        let url = endpoint(&format!(
            "/repos/{owner}/{name}/releases?per_page={PER_PAGE}"
        ))?;
        self.get_pages(url, |_| false).await
    }

    /// `state` 取 `open` / `closed` / `all`；`since` 只带增量同步，GitHub 按 `updated_at` 过滤。
    pub async fn issues(
        &self,
        owner: &str,
        name: &str,
        state: &str,
        since: Option<&str>,
    ) -> Result<Vec<GithubIssue>, GithubError> {
        let mut url = endpoint(&format!("/repos/{owner}/{name}/issues"))?;
        {
            let mut pairs = url.query_pairs_mut();
            pairs.append_pair("state", state);
            pairs.append_pair("sort", "updated");
            pairs.append_pair("direction", "desc");
            pairs.append_pair("per_page", PER_PAGE);
            if let Some(since) = since {
                pairs.append_pair("since", since);
            }
        }
        self.get_pages(url, |_| false).await
    }

    /// PR 列表。接口不支持 `since`，按 `updated` 倒序翻页，遇到早于 `since` 的条目即停止，
    /// 这样增量同步只会读最近改动过的那几页。
    pub async fn pulls(
        &self,
        owner: &str,
        name: &str,
        state: &str,
        since: Option<&str>,
    ) -> Result<Vec<GithubPull>, GithubError> {
        let mut url = endpoint(&format!("/repos/{owner}/{name}/pulls"))?;
        {
            let mut pairs = url.query_pairs_mut();
            pairs.append_pair("state", state);
            pairs.append_pair("sort", "updated");
            pairs.append_pair("direction", "desc");
            pairs.append_pair("per_page", PER_PAGE);
        }
        self.get_pages(url, |pull: &GithubPull| {
            since.is_some_and(|since| pull.updated_at.as_str() < since)
        })
        .await
    }

    /// 单个 PR 的评审记录，只用来判断「我评审过」。
    pub async fn reviews(
        &self,
        owner: &str,
        name: &str,
        number: u64,
    ) -> Result<Vec<GithubReview>, GithubError> {
        let url = endpoint(&format!(
            "/repos/{owner}/{name}/pulls/{number}/reviews?per_page={PER_PAGE}"
        ))?;
        self.get_pages(url, |_| false).await
    }

    /// 对话评论：PR 与 Issue 同一个端点，按创建时间升序返回。
    pub async fn issue_comments(
        &self,
        owner: &str,
        name: &str,
        number: i64,
    ) -> Result<Vec<GithubComment>, GithubError> {
        let url = endpoint(&format!(
            "/repos/{owner}/{name}/issues/{number}/comments?per_page={PER_PAGE}"
        ))?;
        self.get_pages(url, |_| false).await
    }

    /// 当前用户的通知，只取未读：已读通知不会重新进入收件箱。
    /// `since` 是上次同步时间，调用方会往前留一点重叠，避免秒级边界漏消息。
    pub async fn notifications(
        &self,
        since: Option<&str>,
    ) -> Result<Vec<GithubNotification>, GithubError> {
        let mut url = endpoint("/notifications")?;
        {
            let mut pairs = url.query_pairs_mut();
            pairs.append_pair("all", "false");
            pairs.append_pair("participating", "false");
            pairs.append_pair("per_page", PER_PAGE);
            if let Some(since) = since {
                pairs.append_pair("since", since);
            }
        }

        let mut values = Vec::new();
        loop {
            let response = self.send(&url).await?;
            let status = response.status();
            if !status.is_success() {
                return Err(notification_error(status, response).await);
            }
            let next_url = response
                .headers()
                .get(header::LINK)
                .and_then(|value| value.to_str().ok())
                .and_then(next_page_url)
                .map(ToOwned::to_owned);
            let page = response
                .json::<Vec<GithubNotification>>()
                .await
                .map_err(|error| GithubError::Api(error.to_string()))?;
            values.extend(page);
            let Some(next) = next_url else { break };
            url = Url::parse(&next).map_err(|error| GithubError::Api(error.to_string()))?;
            if url.scheme() != "https" || url.host_str() != Some("api.github.com") {
                return Err(GithubError::Api("GitHub API 返回了无效的分页地址".into()));
            }
        }
        Ok(values)
    }

    /// 把本地「已读」回写到 GitHub：`PATCH /notifications/threads/{id}`。
    /// 通知 id 必须是数字，避免把任意字符串拼进路径。
    pub async fn mark_notification_read(&self, thread_id: &str) -> Result<(), GithubError> {
        if thread_id.is_empty() || !thread_id.chars().all(|value| value.is_ascii_digit()) {
            return Err(GithubError::Api("通知 id 无效".into()));
        }
        let url = endpoint(&format!("/notifications/threads/{thread_id}"))?;
        let response = self
            .client
            .patch(url)
            .header(header::ACCEPT, "application/vnd.github+json")
            .header(
                header::USER_AGENT,
                concat!("TomatoDesk/", env!("CARGO_PKG_VERSION")),
            )
            .header(header::CONTENT_LENGTH, 0)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|error| GithubError::Request(error.to_string()))?;
        if !response.status().is_success() {
            let status = response.status();
            return Err(notification_error(status, response).await);
        }
        Ok(())
    }

    fn request(&self, url: &Url) -> reqwest::RequestBuilder {
        self.client
            .get(url.clone())
            .header(header::ACCEPT, "application/vnd.github+json")
            .header(
                header::USER_AGENT,
                concat!("TomatoDesk/", env!("CARGO_PKG_VERSION")),
            )
            .bearer_auth(&self.token)
    }

    async fn get_json<T: DeserializeOwned>(&self, url: Url) -> Result<T, GithubError> {
        let response = self.send(&url).await?;
        if !response.status().is_success() {
            return Err(response_error(response).await);
        }
        response
            .json::<T>()
            .await
            .map_err(|error| GithubError::Api(error.to_string()))
    }

    /// 原样返回响应体，用于 README 这类非 JSON 资源。
    async fn get_raw(&self, url: Url) -> Result<Option<String>, GithubError> {
        let response = self
            .client
            .get(url)
            .header(header::ACCEPT, "application/vnd.github.raw")
            .header(
                header::USER_AGENT,
                concat!("TomatoDesk/", env!("CARGO_PKG_VERSION")),
            )
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|error| GithubError::Request(error.to_string()))?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(response_error(response).await);
        }
        response
            .text()
            .await
            .map(Some)
            .map_err(|error| GithubError::Request(error.to_string()))
    }

    async fn send(&self, url: &Url) -> Result<Response, GithubError> {
        self.request(url)
            .send()
            .await
            .map_err(|error| GithubError::Request(error.to_string()))
    }

    /// 顺着 `Link: rel="next"` 翻页，`should_stop` 命中后不再请求下一页。
    async fn get_pages<T, F>(&self, mut url: Url, mut should_stop: F) -> Result<Vec<T>, GithubError>
    where
        T: DeserializeOwned,
        F: FnMut(&T) -> bool,
    {
        let mut values = Vec::new();
        loop {
            let response = self.send(&url).await?;
            if !response.status().is_success() {
                return Err(response_error(response).await);
            }
            let next_url = response
                .headers()
                .get(header::LINK)
                .and_then(|value| value.to_str().ok())
                .and_then(next_page_url)
                .map(ToOwned::to_owned);
            let page = response
                .json::<Vec<T>>()
                .await
                .map_err(|error| GithubError::Api(error.to_string()))?;
            let stop = page.iter().any(&mut should_stop);
            values.extend(page);
            if stop {
                break;
            }
            let Some(next) = next_url else { break };
            url = Url::parse(&next).map_err(|error| GithubError::Api(error.to_string()))?;
            if url.scheme() != "https" || url.host_str() != Some("api.github.com") {
                return Err(GithubError::Api("GitHub API 返回了无效的分页地址".into()));
            }
        }
        Ok(values)
    }
}

/// `/notifications` 的 403 / 404 基本都是 Token 没有 `notifications` 权限。
/// 限流仍然按限流处理，其余情况给出可操作的权限提示，而不是笼统的「API 返回错误」。
async fn notification_error(status: StatusCode, response: Response) -> GithubError {
    if status == StatusCode::FORBIDDEN {
        let remaining = response
            .headers()
            .get("x-ratelimit-remaining")
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned);
        if remaining.as_deref() == Some("0") {
            return response_error(response).await;
        }
        let body = response.text().await.unwrap_or_default();
        if body.to_ascii_lowercase().contains("rate limit") {
            return GithubError::RateLimited(None);
        }
        return GithubError::MissingNotificationsScope;
    }
    if status == StatusCode::NOT_FOUND {
        return GithubError::MissingNotificationsScope;
    }
    response_error(response).await
}

/// 把非 2xx 响应翻译成可读错误，限流与鉴权分开提示。
async fn response_error(response: Response) -> GithubError {
    let status = response.status();
    let headers = response.headers().clone();
    if status == StatusCode::UNAUTHORIZED {
        return GithubError::Unauthorized;
    }
    if status == StatusCode::FORBIDDEN {
        if headers
            .get("x-ratelimit-remaining")
            .and_then(|value| value.to_str().ok())
            == Some("0")
        {
            return GithubError::RateLimited(
                headers
                    .get("x-ratelimit-reset")
                    .and_then(|value| value.to_str().ok())
                    .map(ToOwned::to_owned),
            );
        }
        return GithubError::Api("Token 权限不足或仓库访问受限".into());
    }
    if status == StatusCode::NOT_FOUND {
        return GithubError::NotFound;
    }
    if status == StatusCode::TOO_MANY_REQUESTS {
        return GithubError::RateLimited(
            headers
                .get("retry-after")
                .and_then(|value| value.to_str().ok())
                .map(ToOwned::to_owned),
        );
    }
    let body = response.text().await.unwrap_or_default();
    GithubError::Api(format!("HTTP {status}: {body}"))
}

fn endpoint(path: &str) -> Result<Url, GithubError> {
    Url::parse(&format!("{API_ROOT}{path}"))
        .map_err(|error| GithubError::Request(error.to_string()))
}

fn next_page_url(link_header: &str) -> Option<&str> {
    link_header.split(',').find_map(|link| {
        let (target, relation) = link.trim().split_once(';')?;
        if relation
            .split(';')
            .any(|value| value.trim() == "rel=\"next\"")
        {
            target.trim().strip_prefix('<')?.strip_suffix('>')
        } else {
            None
        }
    })
}

pub fn parse_repository_url(value: &str) -> Result<(String, String), GithubError> {
    let value = value.trim();
    let url = Url::parse(value)
        .map_err(|_| GithubError::InvalidRepository("请输入 GitHub 仓库 URL".into()))?;
    if url.scheme() != "https"
        || url.host_str().map(|host| host.to_ascii_lowercase()) != Some("github.com".into())
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return Err(GithubError::InvalidRepository(
            "仓库 URL 必须是 https://github.com/owner/name".into(),
        ));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(GithubError::InvalidRepository(
            "仓库 URL 不能包含查询参数或片段".into(),
        ));
    }
    let path = url.path().trim_start_matches('/').trim_end_matches('/');
    let parts = path.split('/').collect::<Vec<_>>();
    if parts.len() != 2 || parts.iter().any(|part| part.is_empty()) {
        return Err(GithubError::InvalidRepository(
            "仓库 URL 必须包含 owner 和 name".into(),
        ));
    }
    let name = parts[1].strip_suffix(".git").unwrap_or(parts[1]);
    if parts[0].is_empty() || name.is_empty() {
        return Err(GithubError::InvalidRepository(
            "owner 和 name 不能为空".into(),
        ));
    }
    Ok((parts[0].to_owned(), name.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::{next_page_url, parse_repository_url};

    #[test]
    fn link_header_finds_only_the_next_page() {
        let header = r#"<https://api.github.com/repos/o/n/pulls?page=2>; rel="next", <https://api.github.com/repos/o/n/pulls?page=5>; rel="last""#;
        assert_eq!(
            next_page_url(header),
            Some("https://api.github.com/repos/o/n/pulls?page=2")
        );
        assert_eq!(
            next_page_url(r#"<https://api.github.com/x?page=5>; rel="last""#),
            None
        );
    }

    #[test]
    fn repository_urls_are_validated_strictly() {
        let parsed = parse_repository_url("https://github.com/AbelTomato/TomatoDesk.git")
            .expect("valid repository url");
        assert_eq!(parsed, ("AbelTomato".to_owned(), "TomatoDesk".to_owned()));
        for invalid in [
            "http://github.com/owner/name",
            "https://github.com/owner",
            "https://github.com/owner/name?tab=readme",
            "https://gitlab.com/owner/name",
        ] {
            assert!(
                parse_repository_url(invalid).is_err(),
                "{invalid} should be rejected"
            );
        }
    }
}
