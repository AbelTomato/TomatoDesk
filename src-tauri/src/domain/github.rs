use serde::{Deserialize, Serialize};
use std::str::FromStr;

use super::cursor::KeysetCursor;

/// README 原文的存储上限，避免把整篇文档塞进一行。
pub const README_TEXT_MAX_CHARS: usize = 20_000;
/// 概览里展示的纯文本摘要长度。
pub const README_SUMMARY_MAX_CHARS: usize = 400;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadKind {
    PullRequest,
    Issue,
}

impl ThreadKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PullRequest => "pull_request",
            Self::Issue => "issue",
        }
    }
}

impl FromStr for ThreadKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "pull_request" => Ok(Self::PullRequest),
            "issue" => Ok(Self::Issue),
            _ => Err(format!("unknown GitHub thread kind: {value}")),
        }
    }
}

/// 合并状态：草稿 PR 是「打开但未就绪」，已合并的 PR 单独一档。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadState {
    Open,
    Draft,
    Merged,
    Closed,
}

impl ThreadState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Draft => "draft",
            Self::Merged => "merged",
            Self::Closed => "closed",
        }
    }

    /// 打开中的状态，用于「未关闭」计数与筛选。
    pub fn is_open(self) -> bool {
        matches!(self, Self::Open | Self::Draft)
    }
}

impl FromStr for ThreadState {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "open" => Ok(Self::Open),
            "draft" => Ok(Self::Draft),
            "merged" => Ok(Self::Merged),
            "closed" => Ok(Self::Closed),
            _ => Err(format!("unknown GitHub thread state: {value}")),
        }
    }
}

/// 与当前 Token 用户的关系，四种可以同时成立，主标签按优先级取一个。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadRelationship {
    Author,
    Assigned,
    ReviewRequested,
    Reviewed,
    None,
}

impl ThreadRelationship {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Author => "author",
            Self::Assigned => "assigned",
            Self::ReviewRequested => "review_requested",
            Self::Reviewed => "reviewed",
            Self::None => "none",
        }
    }
}

impl FromStr for ThreadRelationship {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "author" => Ok(Self::Author),
            "assigned" => Ok(Self::Assigned),
            "review_requested" => Ok(Self::ReviewRequested),
            "reviewed" => Ok(Self::Reviewed),
            "none" => Ok(Self::None),
            _ => Err(format!("unknown GitHub relationship: {value}")),
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
    /// 阶段一之后不再写入，保留为后续可能复用的遗留字段。
    pub etag: Option<String>,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub language: Option<String>,
    pub stars: Option<i64>,
    pub forks: Option<i64>,
    pub open_issues: Option<i64>,
    pub default_branch: Option<String>,
    pub pushed_at: Option<String>,
    pub is_archived: bool,
    pub is_private: bool,
    /// README 原文（Markdown），只在详情页按需展示，不渲染 HTML。
    pub readme_text: Option<String>,
    pub readme_summary: Option<String>,
    pub readme_synced_at: Option<String>,
    pub custom_description: Option<String>,
    pub custom_note: Option<String>,
    pub meta_synced_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl GithubRepo {
    /// 展示用描述：自定义优先，其次取仓库 description。
    pub fn display_description(&self) -> Option<&str> {
        self.custom_description
            .as_deref()
            .or(self.description.as_deref())
    }
}

/// 仓库线程（PR / Issue）的完整行，写入与读取共用。
/// `id` 与 `synced_at` 在 upsert 命中已有行时不会覆盖。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubThread {
    pub id: String,
    pub repo_id: String,
    pub kind: ThreadKind,
    pub number: i64,
    pub title: String,
    pub state: ThreadState,
    pub author: Option<String>,
    pub url: String,
    pub body: Option<String>,
    pub summary: Option<String>,
    pub is_author: bool,
    pub is_assigned: bool,
    pub review_requested: bool,
    pub reviewed: bool,
    pub comment_count: i64,
    pub additions: Option<i64>,
    pub deletions: Option<i64>,
    pub changed_files: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
    pub closed_at: Option<String>,
    pub merged_at: Option<String>,
    pub synced_at: String,
}

impl GithubThread {
    /// 列表行上的关系主标签：作者 > 待我评审 > 我评审过 > 指派给我。
    pub fn relationship(&self) -> ThreadRelationship {
        if self.is_author {
            ThreadRelationship::Author
        } else if self.review_requested {
            ThreadRelationship::ReviewRequested
        } else if self.reviewed {
            ThreadRelationship::Reviewed
        } else if self.is_assigned {
            ThreadRelationship::Assigned
        } else {
            ThreadRelationship::None
        }
    }
}

/// 列表行：不含正文，够渲染一行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadSummary {
    pub id: String,
    pub repo_id: String,
    pub kind: ThreadKind,
    pub number: i64,
    pub title: String,
    pub state: ThreadState,
    pub author: Option<String>,
    pub url: String,
    pub summary: Option<String>,
    pub is_author: bool,
    pub is_assigned: bool,
    pub review_requested: bool,
    pub reviewed: bool,
    pub comment_count: i64,
    pub additions: Option<i64>,
    pub deletions: Option<i64>,
    pub changed_files: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
    pub closed_at: Option<String>,
    pub merged_at: Option<String>,
}

impl From<&GithubThread> for ThreadSummary {
    fn from(thread: &GithubThread) -> Self {
        Self {
            id: thread.id.clone(),
            repo_id: thread.repo_id.clone(),
            kind: thread.kind,
            number: thread.number,
            title: thread.title.clone(),
            state: thread.state,
            author: thread.author.clone(),
            url: thread.url.clone(),
            summary: thread.summary.clone(),
            is_author: thread.is_author,
            is_assigned: thread.is_assigned,
            review_requested: thread.review_requested,
            reviewed: thread.reviewed,
            comment_count: thread.comment_count,
            additions: thread.additions,
            deletions: thread.deletions,
            changed_files: thread.changed_files,
            created_at: thread.created_at.clone(),
            updated_at: thread.updated_at.clone(),
            closed_at: thread.closed_at.clone(),
            merged_at: thread.merged_at.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadDetail {
    #[serde(flatten)]
    pub summary: ThreadSummary,
    pub body: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ThreadQuery {
    pub repo_id: String,
    pub kind: Option<ThreadKind>,
    pub state: Option<ThreadState>,
    pub relationship: Option<ThreadRelationship>,
    pub cursor: Option<KeysetCursor>,
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadPage {
    pub threads: Vec<ThreadSummary>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubRelease {
    pub id: String,
    pub repo_id: String,
    pub tag_name: String,
    pub name: Option<String>,
    pub url: String,
    pub body: Option<String>,
    pub summary: Option<String>,
    pub is_draft: bool,
    pub is_prerelease: bool,
    pub published_at: Option<String>,
    pub created_at: String,
    pub synced_at: String,
}

/// 侧栏与标签页角标：全部来自 SQL 聚合，不为拿计数加载行。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoThreadCounts {
    pub open_pull_requests: i64,
    pub open_issues: i64,
    pub reviews: i64,
    pub releases: i64,
    pub items: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoSummary {
    pub id: String,
    pub owner: String,
    pub name: String,
    /// 展示用描述：自定义优先。
    pub description: Option<String>,
    pub language: Option<String>,
    pub stars: Option<i64>,
    pub open_pull_requests: i64,
    pub open_issues: i64,
    pub reviews: i64,
    pub releases: i64,
    pub items: i64,
    pub enabled: bool,
    pub last_synced_at: Option<String>,
    pub is_archived: bool,
    pub is_private: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoDetail {
    #[serde(flatten)]
    pub repo: GithubRepo,
    pub counts: RepoThreadCounts,
}

/// 一次同步写入的结果。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SyncWrites {
    pub threads_upserted: usize,
    pub releases_upserted: usize,
}

/// README 摘要：按行抽取纯文本，跳过代码块与徽标行，不渲染 HTML。
pub fn readme_summary(markdown: &str) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut in_fence = false;
    let mut length = 0;

    for raw_line in markdown.lines() {
        let line = raw_line.trim();
        if line.starts_with("```") || line.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence || line.is_empty() || is_badge_line(line) {
            continue;
        }
        let text = strip_markdown_inline(line);
        if text.is_empty() {
            continue;
        }
        length += text.chars().count() + 1;
        parts.push(text);
        if length >= README_SUMMARY_MAX_CHARS {
            break;
        }
    }

    if parts.is_empty() {
        return None;
    }
    let mut summary = parts.join(" ");
    if summary.chars().count() > README_SUMMARY_MAX_CHARS {
        summary = summary.chars().take(README_SUMMARY_MAX_CHARS).collect();
        summary.push('…');
    }
    Some(summary)
}

/// 徽标行（`[![CI](…)](…)`、纯图片行）抽出来只会剩一串 URL，直接跳过。
fn is_badge_line(line: &str) -> bool {
    let trimmed = line.trim_start_matches(['>', '-', '*', '+', ' ']);
    trimmed.starts_with("[![") || trimmed.starts_with("![")
}

fn strip_markdown_inline(line: &str) -> String {
    let without_html = strip_html_tags(line);
    let without_links = strip_links(&without_html);
    let without_marker =
        without_links.trim_start_matches(['#', '>', '-', '*', '+', '=', '|', ' ', '\t']);
    let without_ordinal = strip_ordered_marker(without_marker);

    without_ordinal
        .replace(['`', '*', '_', '~'], "")
        .split_whitespace()
        .filter(|token| !token.contains("://"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// 去掉 `<tag …>` 与 HTML 注释，保留标签之间的文字。
fn strip_html_tags(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut depth = 0;
    for character in input.chars() {
        match character {
            '<' => depth += 1,
            '>' if depth > 0 => depth -= 1,
            _ if depth == 0 => output.push(character),
            _ => {}
        }
    }
    output
}

/// `[文字](链接)` 保留文字、丢弃链接；`![alt](链接)` 整段丢弃。
fn strip_links(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find('[') {
        let is_image = rest[..start].ends_with('!');
        let text_start = start + 1;
        let Some(close_offset) = rest[text_start..].find(']') else {
            break;
        };
        let close = text_start + close_offset;
        let after_text = close + 1;
        let Some(paren_offset) = rest[after_text..].find('(') else {
            break;
        };
        let paren = after_text + paren_offset;
        let Some(end_offset) = rest[paren..].find(')') else {
            break;
        };
        let end = paren + end_offset + 1;

        let head_end = if is_image {
            start.saturating_sub(1)
        } else {
            start
        };
        output.push_str(&rest[..head_end]);
        if !is_image {
            output.push_str(&rest[text_start..close]);
        }
        rest = &rest[end..];
    }
    output.push_str(rest);
    output
}

/// 去掉 `1.` / `1)` 这类有序列表序号。
fn strip_ordered_marker(input: &str) -> &str {
    let digits = input
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .count();
    if digits == 0 {
        return input;
    }
    let rest = &input[digits..];
    rest.strip_prefix('.')
        .or_else(|| rest.strip_prefix(')'))
        .unwrap_or(input)
}

#[cfg(test)]
mod tests {
    use super::{ThreadRelationship, ThreadState, readme_summary};

    #[test]
    fn readme_summary_skips_code_fences_and_badge_lines() {
        let markdown = r#"
# TomatoDesk

[![CI](https://example.com/badge.svg)](https://example.com/ci)

本地优先的个人信息收件箱，把链接、笔记与 GitHub 更新收在一处。

```bash
pnpm install
```

- 支持 **键集分页**
- 支持 [自定义描述](https://example.com/desc)
"#;
        let summary = readme_summary(markdown).expect("summary");
        assert!(summary.starts_with("TomatoDesk"), "{summary}");
        assert!(summary.contains("本地优先的个人信息收件箱"));
        assert!(summary.contains("支持 键集分页"), "{summary}");
        assert!(summary.contains("自定义描述"), "{summary}");
        assert!(!summary.contains("pnpm install"), "code fence leaked");
        assert!(!summary.contains("badge.svg"), "badge line leaked");
        assert!(!summary.contains("https://"), "links should be dropped");
        assert!(!summary.contains('#'), "heading markers should be dropped");
    }

    #[test]
    fn readme_summary_is_bounded_and_none_when_empty() {
        assert_eq!(readme_summary("   \n\n"), None);
        let long = "word ".repeat(200);
        let summary = readme_summary(&long).expect("summary");
        assert_eq!(
            summary.chars().count(),
            super::README_SUMMARY_MAX_CHARS + 1,
            "bounded with an ellipsis"
        );
        assert!(summary.ends_with('…'));
    }

    #[test]
    fn thread_state_reports_open_variants() {
        assert!(ThreadState::Open.is_open());
        assert!(ThreadState::Draft.is_open());
        assert!(!ThreadState::Merged.is_open());
        assert!(!ThreadState::Closed.is_open());
    }

    #[test]
    fn relationships_round_trip_through_strings() {
        for relationship in [
            ThreadRelationship::Author,
            ThreadRelationship::Assigned,
            ThreadRelationship::ReviewRequested,
            ThreadRelationship::Reviewed,
            ThreadRelationship::None,
        ] {
            assert_eq!(
                relationship.as_str().parse::<ThreadRelationship>(),
                Ok(relationship)
            );
        }
    }
}
