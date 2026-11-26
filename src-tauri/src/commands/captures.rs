use reqwest::Url;
use tauri::State;

use crate::{
    domain::{
        Capture, CaptureBody, CaptureCounts, CaptureKind, CapturePage, CaptureSummary, NewCapture,
    },
    infrastructure::{db::Database, web_metadata::fetch_web_text},
};

use super::{
    dto::{
        CountCapturesInput, CreateCaptureInput, IdInput, ListCapturesPageInput,
        SearchCapturesInput, UpdateCaptureInput, parse_input,
    },
    error::{CommandError, CommandResult},
};

pub(super) fn create_capture_in(
    database: &Database,
    input: CreateCaptureInput,
) -> CommandResult<Capture> {
    let input = input.validate()?;
    // 笔记类没有正文时，把 note 当作正文；这样仓库页等旧调用点不需要改语义。
    let (note, body) = match input.kind {
        CaptureKind::Note if input.body.is_none() => (None, input.note),
        _ => (input.note, input.body),
    };
    database
        .captures()
        .create(NewCapture {
            kind: input.kind,
            source: input.source,
            url: input.url,
            title: input.title,
            summary: input.summary,
            note,
            body,
            repo_id: input.repo_id,
        })
        .map_err(Into::into)
}

pub(super) fn get_capture_in(database: &Database, input: IdInput) -> CommandResult<Capture> {
    let input = input.validate()?;
    database
        .captures()
        .get(&input.id)
        .map_err(CommandError::from)?
        .ok_or_else(|| CommandError::not_found("capture"))
}

pub(super) fn list_captures_page_in(
    database: &Database,
    input: ListCapturesPageInput,
) -> CommandResult<CapturePage> {
    let query = input.into_query()?;
    database.captures().page(&query).map_err(Into::into)
}

pub(super) fn count_captures_in(
    database: &Database,
    input: CountCapturesInput,
) -> CommandResult<CaptureCounts> {
    database.captures().counts(input.kind).map_err(Into::into)
}

pub(super) fn update_capture_in(
    database: &Database,
    input: UpdateCaptureInput,
) -> CommandResult<Capture> {
    let input = input.validate()?;
    let mut capture = get_capture_in(
        database,
        IdInput {
            id: input.id.clone(),
        },
    )?;
    capture.kind = input.kind;
    capture.source = input.source;
    capture.url = input.url;
    capture.title = input.title;
    capture.summary = input.summary;
    capture.note = input.note;

    if !database
        .captures()
        .update(&capture)
        .map_err(CommandError::from)?
    {
        return Err(CommandError::not_found("capture"));
    }
    get_capture_in(database, IdInput { id: capture.id })
}

pub(super) fn archive_capture_in(database: &Database, input: IdInput) -> CommandResult<Capture> {
    let capture = get_capture_in(database, input)?;
    if !database
        .captures()
        .set_archived(&capture.id, true)
        .map_err(CommandError::from)?
    {
        return Err(CommandError::not_found("capture"));
    }
    get_capture_in(database, IdInput { id: capture.id })
}

pub(super) fn restore_capture_in(database: &Database, input: IdInput) -> CommandResult<Capture> {
    let capture = get_capture_in(database, input)?;
    if !database
        .captures()
        .set_archived(&capture.id, false)
        .map_err(CommandError::from)?
    {
        return Err(CommandError::not_found("capture"));
    }
    get_capture_in(database, IdInput { id: capture.id })
}

pub(super) fn search_captures_in(
    database: &Database,
    input: SearchCapturesInput,
) -> CommandResult<Vec<CaptureSummary>> {
    let input = input.validate()?;
    database.captures().search(&input.query).map_err(Into::into)
}

/// 详情正文按需加载：本地有就返回，链接类为空时回源抓一次并写回缓存，
/// 笔记类只返回本地正文，不触发网络请求。
pub(super) async fn get_capture_body_in(
    database: &Database,
    input: IdInput,
) -> CommandResult<CaptureBody> {
    let input = input.validate()?;
    let body = database
        .captures()
        .get_body(&input.id)
        .map_err(CommandError::from)?
        .ok_or_else(|| CommandError::not_found("capture"))?;

    if body
        .body
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return Ok(body);
    }
    if body.kind != CaptureKind::Link {
        return Ok(body);
    }
    let Some(url) = body
        .url
        .as_deref()
        .and_then(|value| Url::parse(value).ok())
        .filter(|url| matches!(url.scheme(), "http" | "https"))
    else {
        return Ok(body);
    };
    let Some(text) = fetch_web_text(&url).await else {
        return Ok(body);
    };
    database
        .captures()
        .set_body(&body.id, &text)
        .map_err(CommandError::from)?;
    Ok(CaptureBody {
        body: Some(text),
        ..body
    })
}

#[tauri::command]
pub fn create_capture(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<Capture> {
    create_capture_in(&database, parse_input(input)?)
}

#[tauri::command]
pub fn get_capture(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<Capture> {
    get_capture_in(&database, parse_input(input)?)
}

#[tauri::command]
pub fn list_captures_page(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<CapturePage> {
    list_captures_page_in(&database, parse_input(input)?)
}

#[tauri::command]
pub fn count_captures(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<CaptureCounts> {
    let input = match input {
        Some(value) => serde_json::from_value(value)
            .map_err(|_| CommandError::validation("input is invalid"))?,
        None => CountCapturesInput::default(),
    };
    count_captures_in(&database, input)
}

#[tauri::command]
pub async fn get_capture_body(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<CaptureBody> {
    get_capture_body_in(&database, parse_input(input)?).await
}

#[tauri::command]
pub fn update_capture(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<Capture> {
    update_capture_in(&database, parse_input(input)?)
}

#[tauri::command]
pub fn archive_capture(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<Capture> {
    archive_capture_in(&database, parse_input(input)?)
}

#[tauri::command]
pub fn restore_capture(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<Capture> {
    restore_capture_in(&database, parse_input(input)?)
}

#[tauri::command]
pub fn search_captures(
    database: State<'_, Database>,
    input: Option<serde_json::Value>,
) -> CommandResult<Vec<CaptureSummary>> {
    search_captures_in(&database, parse_input(input)?)
}
