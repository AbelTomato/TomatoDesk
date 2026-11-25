use std::time::Duration;

use reqwest::{Client, StatusCode, Url, header};
use serde::Deserialize;

const API_ROOT: &str = "https://api.github.com";
const USER_AGENT: &str = "TomatoDesk/0.1.0";

#[derive(Debug)]
pub enum GithubError {
    InvalidRepository(String),
    Request(String),
    RateLimited(Option<String>),
    Unauthorized,
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
            Self::NotFound => formatter.write_str("GitHub 仓库不存在或当前 Token 无权访问"),
            Self::Api(message) => write!(formatter, "GitHub API 返回错误：{message}"),
        }
    }
}

impl std::error::Error for GithubError {}

#[derive(Debug, Clone)]
pub struct GithubResponse<T> {
    pub value: T,
    pub etag: Option<String>,
    pub not_modified: bool,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct GithubRelease {
    pub id: u64,
    pub tag_name: String,
    pub name: Option<String>,
    pub body: Option<String>,
    pub html_url: String,
    pub published_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct GithubIssue {
    pub id: u64,
    pub number: u64,
    pub title: String,
    pub body: Option<String>,
    pub html_url: String,
    pub state: String,
    pub updated_at: String,
    pub created_at: String,
    pub pull_request: Option<serde_json::Value>,
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

    pub async fn releases(
        &self,
        owner: &str,
        name: &str,
        etag: Option<&str>,
    ) -> Result<GithubResponse<Vec<GithubRelease>>, GithubError> {
        let url = endpoint(&format!("/repos/{owner}/{name}/releases?per_page=100"))?;
        self.get_list(url, etag).await
    }

    /// `state` 取 `open` 或 `all`；`since` 只带增量同步，GitHub 按 `updated_at` 过滤。
    pub async fn issues(
        &self,
        owner: &str,
        name: &str,
        state: &str,
        since: Option<&str>,
        etag: Option<&str>,
    ) -> Result<GithubResponse<Vec<GithubIssue>>, GithubError> {
        let mut url = endpoint(&format!("/repos/{owner}/{name}/issues"))?;
        {
            let mut pairs = url.query_pairs_mut();
            pairs.append_pair("state", state);
            pairs.append_pair("sort", "updated");
            pairs.append_pair("direction", "desc");
            pairs.append_pair("per_page", "100");
            if let Some(since) = since {
                pairs.append_pair("since", since);
            }
        }
        self.get_list(url, etag).await
    }

    async fn get_list<T: for<'de> Deserialize<'de>>(
        &self,
        url: Url,
        etag: Option<&str>,
    ) -> Result<GithubResponse<Vec<T>>, GithubError> {
        let mut url = url;
        let mut values = Vec::new();
        let mut response_etag = None;
        let mut is_first_page = true;
        loop {
            let mut request = self
                .client
                .get(url.clone())
                .header(header::ACCEPT, "application/vnd.github+json")
                .header(header::USER_AGENT, USER_AGENT)
                .bearer_auth(&self.token);
            if is_first_page && let Some(etag) = etag {
                request = request.header(header::IF_NONE_MATCH, etag);
            }
            let response = request
                .send()
                .await
                .map_err(|error| GithubError::Request(error.to_string()))?;
            let status = response.status();
            if is_first_page {
                response_etag = response
                    .headers()
                    .get(header::ETAG)
                    .and_then(|value| value.to_str().ok())
                    .map(ToOwned::to_owned);
            }
            if status == StatusCode::NOT_MODIFIED && is_first_page {
                return Ok(GithubResponse {
                    value: Vec::new(),
                    etag: response_etag,
                    not_modified: true,
                });
            }
            if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
                if status == StatusCode::FORBIDDEN
                    && response
                        .headers()
                        .get("x-ratelimit-remaining")
                        .and_then(|value| value.to_str().ok())
                        == Some("0")
                {
                    return Err(GithubError::RateLimited(
                        response
                            .headers()
                            .get("x-ratelimit-reset")
                            .and_then(|value| value.to_str().ok())
                            .map(ToOwned::to_owned),
                    ));
                }
                return if status == StatusCode::UNAUTHORIZED {
                    Err(GithubError::Unauthorized)
                } else {
                    Err(GithubError::Api("Token 权限不足或仓库访问受限".into()))
                };
            }
            if status == StatusCode::NOT_FOUND {
                return Err(GithubError::NotFound);
            }
            if status == StatusCode::TOO_MANY_REQUESTS {
                return Err(GithubError::RateLimited(
                    response
                        .headers()
                        .get("retry-after")
                        .and_then(|value| value.to_str().ok())
                        .map(ToOwned::to_owned),
                ));
            }
            if !status.is_success() {
                let body = response.text().await.unwrap_or_default();
                return Err(GithubError::Api(format!("HTTP {status}: {body}")));
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
            values.extend(page);
            let Some(next) = next_url else { break };
            url = Url::parse(&next).map_err(|error| GithubError::Api(error.to_string()))?;
            if url.scheme() != "https" || url.host_str() != Some("api.github.com") {
                return Err(GithubError::Api("GitHub API 返回了无效的分页地址".into()));
            }
            is_first_page = false;
        }
        Ok(GithubResponse {
            value: values,
            etag: response_etag,
            not_modified: false,
        })
    }
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
