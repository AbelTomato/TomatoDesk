use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GithubEventType {
    Release,
    Issue,
    PullRequest,
}

impl GithubEventType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Release => "release",
            Self::Issue => "issue",
            Self::PullRequest => "pull_request",
        }
    }
}

impl FromStr for GithubEventType {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "release" => Ok(Self::Release),
            "issue" => Ok(Self::Issue),
            "pull_request" => Ok(Self::PullRequest),
            _ => Err(format!("unknown GitHub event type: {value}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubRepo {
    pub id: String,
    pub owner: String,
    pub name: String,
    pub enabled: bool,
    pub last_synced_at: Option<String>,
    pub etag: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubEvent {
    pub id: String,
    pub repo_id: String,
    pub source_event_id: String,
    pub event_type: GithubEventType,
    pub payload_json: String,
    pub occurred_at: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewGithubEvent {
    pub repo_id: String,
    pub source_event_id: String,
    pub event_type: GithubEventType,
    pub payload_json: String,
    pub occurred_at: String,
}
