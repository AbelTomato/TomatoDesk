use chrono::{SecondsFormat, Utc};
use serde::Serialize;

use crate::{
    domain::{GithubRepo, Item, ItemKind, ItemStatus, Tag},
    infrastructure::db::Database,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportItem {
    #[serde(flatten)]
    pub item: Item,
    pub tags: Vec<Tag>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRepository {
    pub owner: String,
    pub name: String,
    pub enabled: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportBundle {
    pub format_version: u32,
    pub exported_at: String,
    pub tags: Vec<Tag>,
    pub github_repositories: Vec<ExportRepository>,
    pub items: Vec<ExportItem>,
}

pub fn build_export_bundle(database: &Database) -> Result<ExportBundle, String> {
    let items = database.items().list().map_err(|error| error.to_string())?;
    let mut exported_items = Vec::with_capacity(items.len());
    for item in items {
        let tags = database
            .tags()
            .for_item(&item.id)
            .map_err(|error| error.to_string())?;
        exported_items.push(ExportItem { item, tags });
    }

    let tags = database.tags().list().map_err(|error| error.to_string())?;
    let github_repositories = database
        .github_repos()
        .list()
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|repo: GithubRepo| ExportRepository {
            owner: repo.owner,
            name: repo.name,
            enabled: repo.enabled,
        })
        .collect();

    Ok(ExportBundle {
        format_version: 1,
        exported_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        tags,
        github_repositories,
        items: exported_items,
    })
}

pub fn to_json(bundle: &ExportBundle) -> Result<String, String> {
    serde_json::to_string_pretty(bundle).map_err(|error| error.to_string())
}

pub fn to_markdown(bundle: &ExportBundle) -> String {
    let mut output = format!(
        "# TomatoDesk 导出\n\n- 导出时间：{}\n- 条目数：{}\n\n",
        bundle.exported_at,
        bundle.items.len()
    );

    if !bundle.tags.is_empty() {
        output.push_str("## 标签\n\n");
        for tag in &bundle.tags {
            output.push_str(&format!("- {}\n", markdown_text(&tag.name)));
        }
        output.push('\n');
    }

    if !bundle.github_repositories.is_empty() {
        output.push_str("## GitHub 仓库\n\n");
        for repo in &bundle.github_repositories {
            output.push_str(&format!(
                "- {}/{}（{}）\n",
                markdown_text(&repo.owner),
                markdown_text(&repo.name),
                if repo.enabled { "启用" } else { "已停用" }
            ));
        }
        output.push('\n');
    }

    output.push_str("## 收集内容\n\n");
    for exported in &bundle.items {
        let item = &exported.item;
        let title = item
            .title
            .as_deref()
            .or(item.summary.as_deref())
            .or(item.note.as_deref())
            .or(item.url.as_deref())
            .unwrap_or("未命名");
        output.push_str(&format!("### {}\n\n", markdown_heading(title)));
        output.push_str(&format!(
            "- 类型：{}\n- 状态：{}\n- 来源：{}\n- 创建时间：{}\n- 更新时间：{}\n",
            kind_label(item.kind),
            status_label(item.status),
            markdown_text(&item.source),
            item.created_at,
            item.updated_at
        ));
        if !exported.tags.is_empty() {
            output.push_str(&format!(
                "- 标签：{}\n",
                exported
                    .tags
                    .iter()
                    .map(|tag| markdown_text(&tag.name))
                    .collect::<Vec<_>>()
                    .join("、")
            ));
        }
        if let Some(url) = &item.url {
            output.push_str(&format!("- URL：{}\n", markdown_text(url)));
        }
        if let Some(summary) = &item.summary {
            output.push_str(&format!("\n摘要：\n{}\n", markdown_quote(summary)));
        }
        if let Some(note) = &item.note {
            output.push_str(&format!("\n备注：\n{}\n", markdown_quote(note)));
        }
        output.push('\n');
    }

    output
}

fn kind_label(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Note => "笔记",
        ItemKind::Link => "链接",
        ItemKind::Github => "GitHub",
    }
}

fn status_label(status: ItemStatus) -> &'static str {
    match status {
        ItemStatus::Pending => "待处理",
        ItemStatus::Processed => "已处理",
        ItemStatus::Archived => "已归档",
    }
}

fn markdown_heading(value: &str) -> String {
    value
        .replace(['\r', '\n', '\t'], " ")
}

fn markdown_text(value: &str) -> String {
    value.replace(['\r', '\n'], " ")
}

fn markdown_quote(value: &str) -> String {
    value
        .lines()
        .map(|line| format!("> {}", line))
        .collect::<Vec<_>>()
        .join("\n")
}
