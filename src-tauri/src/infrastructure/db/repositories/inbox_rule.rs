use rusqlite::{Connection, OptionalExtension, Row, params, params_from_iter, types::Value};
use uuid::Uuid;

use crate::{
    domain::{InboxRule, InboxRuleAction, InboxRuleFilter, InboxRuleForm, InboxRulePreview},
    infrastructure::db::{Database, DbError, DbResult},
};

use super::parse_value;

const RULE_FIELDS: &str = "id, source_id, category, actor_contains, title_contains, action, \
                           enabled, hit_count, last_hit_at, created_at, updated_at";

type RawRule = (
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
    bool,
    i64,
    Option<String>,
    String,
    String,
);

/// 试跑只回前几条标题，够确认「命中的是不是我以为的那批」。
const PREVIEW_SAMPLES: i64 = 5;

pub struct InboxRuleRepository {
    database: Database,
}

impl InboxRuleRepository {
    pub(crate) fn new(database: Database) -> Self {
        Self { database }
    }

    /// 设置页列表：按创建顺序排，规则之间谁先命中一目了然。
    /// 同一毫秒创建的两条用 `rowid` 兜底，顺序才是稳定的。
    pub fn list(&self) -> DbResult<Vec<InboxRule>> {
        self.database.with_connection(|connection| {
            let mut statement = connection.prepare(&format!(
                "SELECT {RULE_FIELDS} FROM inbox_rules ORDER BY created_at, rowid"
            ))?;
            let rows = statement.query_map([], raw_rule)?;
            rows.map(|row| decode_rule(row?))
                .collect::<DbResult<Vec<_>>>()
        })
    }

    /// 入库时按这个顺序取第一条命中的规则：先创建的优先，同一毫秒用 `rowid` 兜底。
    pub fn enabled(&self) -> DbResult<Vec<InboxRule>> {
        self.database.with_connection(|connection| {
            let mut statement = connection.prepare(&format!(
                "SELECT {RULE_FIELDS} FROM inbox_rules
                 WHERE enabled = 1 ORDER BY created_at, rowid"
            ))?;
            let rows = statement.query_map([], raw_rule)?;
            rows.map(|row| decode_rule(row?))
                .collect::<DbResult<Vec<_>>>()
        })
    }

    pub fn get(&self, id: &str) -> DbResult<Option<InboxRule>> {
        self.database
            .with_connection(|connection| get_rule(connection, id))
    }

    /// 新增或更新。空条件在这里就被拒绝：迁移里的 CHECK 也会挡，
    /// 但那是「数据库错误」，这里给调用方一句能读懂的话。
    pub fn upsert(&self, form: InboxRuleForm) -> DbResult<InboxRule> {
        if form.filter.is_empty() {
            return Err(DbError::InvalidData(
                "inbox rule needs at least one condition".into(),
            ));
        }
        let InboxRuleForm {
            id,
            filter,
            action,
            enabled,
        } = form;
        let InboxRuleFilter {
            source_id,
            category,
            actor_contains,
            title_contains,
        } = filter;

        self.database.with_transaction(|transaction| match id {
            Some(id) => {
                let changed = transaction.execute(
                    "UPDATE inbox_rules
                     SET source_id = ?2, category = ?3, actor_contains = ?4,
                         title_contains = ?5, action = ?6, enabled = ?7,
                         updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                     WHERE id = ?1",
                    params![
                        id,
                        source_id,
                        category,
                        actor_contains,
                        title_contains,
                        action.as_str(),
                        enabled
                    ],
                )?;
                if changed == 0 {
                    return Err(DbError::NotFound("inbox rule"));
                }
                get_rule(transaction, &id)?.ok_or(DbError::NotFound("inbox rule"))
            }
            None => {
                let id = Uuid::new_v4().to_string();
                transaction.execute(
                    "INSERT INTO inbox_rules(
                         id, source_id, category, actor_contains, title_contains, action, enabled
                     )
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        id,
                        source_id,
                        category,
                        actor_contains,
                        title_contains,
                        action.as_str(),
                        enabled
                    ],
                )?;
                get_rule(transaction, &id)?.ok_or(DbError::NotFound("inbox rule"))
            }
        })
    }

    pub fn delete(&self, id: &str) -> DbResult<bool> {
        self.database.with_connection(|connection| {
            connection
                .execute("DELETE FROM inbox_rules WHERE id = ?1", [id])
                .map(|changed| changed > 0)
                .map_err(Into::into)
        })
    }

    /// 试跑：最近 14 天里符合条件的有多少条，其中还有多少条没处理。
    ///
    /// `match_count` 含已处理的行（规则不会回头改它们），`pending_count` 才是
    /// 真的会被规则动到的条数；两个数一起给出，用户才知道「上线后会怎样」。
    pub fn preview(&self, filter: &InboxRuleFilter) -> DbResult<InboxRulePreview> {
        if filter.is_empty() {
            return Ok(InboxRulePreview::default());
        }

        let mut counts_sql = String::from(
            "SELECT COUNT(*),
                    COALESCE(SUM(CASE WHEN triaged_at IS NULL THEN 1 ELSE 0 END), 0)
             FROM inbox_messages WHERE occurred_at >= ",
        );
        counts_sql.push_str(RECENT_WINDOW);
        let mut values: Vec<Value> = Vec::new();
        push_filter(&mut counts_sql, &mut values, filter);

        let mut samples_sql =
            String::from("SELECT title FROM inbox_messages WHERE occurred_at >= ");
        samples_sql.push_str(RECENT_WINDOW);
        let mut sample_values: Vec<Value> = Vec::new();
        push_filter(&mut samples_sql, &mut sample_values, filter);
        samples_sql.push_str(" AND triaged_at IS NULL ORDER BY occurred_at DESC, id DESC LIMIT ?");
        sample_values.push(Value::Integer(PREVIEW_SAMPLES));

        self.database.with_connection(|connection| {
            let (match_count, pending_count) =
                connection.query_row(&counts_sql, params_from_iter(values.iter()), |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })?;
            let mut statement = connection.prepare(&samples_sql)?;
            let rows =
                statement.query_map(params_from_iter(sample_values.iter()), |row| row.get(0))?;
            let sample_titles = rows.collect::<rusqlite::Result<Vec<String>>>()?;
            Ok(InboxRulePreview {
                match_count,
                pending_count,
                sample_titles,
            })
        })
    }
}

/// 最近 14 天的下界：与邮件同步窗口一致，避免历史积压把试跑数字放大。
/// 时间戳都是 `YYYY-MM-DDTHH:MM:SS.mmmZ`，可以直接按字符串比。
const RECENT_WINDOW: &str = "strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-14 days')";

/// 规则条件的 SQL 版本，必须与 `InboxRuleFilter::matches` 同口径：
/// 条件之间是「且」，文本用 `instr` 做子串判断，大小写按 `lower` 折叠。
fn push_filter(sql: &mut String, values: &mut Vec<Value>, filter: &InboxRuleFilter) {
    if let Some(source_id) = &filter.source_id {
        sql.push_str(" AND source_id = ?");
        values.push(Value::Text(source_id.clone()));
    }
    if let Some(category) = &filter.category {
        sql.push_str(" AND category = ?");
        values.push(Value::Text(category.clone()));
    }
    if let Some(actor) = &filter.actor_contains {
        sql.push_str(" AND instr(lower(COALESCE(actor, '')), lower(?)) > 0");
        values.push(Value::Text(actor.clone()));
    }
    if let Some(title) = &filter.title_contains {
        sql.push_str(" AND instr(lower(title), lower(?)) > 0");
        values.push(Value::Text(title.clone()));
    }
}

fn get_rule(connection: &Connection, id: &str) -> DbResult<Option<InboxRule>> {
    connection
        .query_row(
            &format!("SELECT {RULE_FIELDS} FROM inbox_rules WHERE id = ?1"),
            [id],
            raw_rule,
        )
        .optional()?
        .map(decode_rule)
        .transpose()
}

fn raw_rule(row: &Row<'_>) -> rusqlite::Result<RawRule> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
        row.get(10)?,
    ))
}

fn decode_rule(row: RawRule) -> DbResult<InboxRule> {
    Ok(InboxRule {
        id: row.0,
        source_id: row.1,
        category: row.2,
        actor_contains: row.3,
        title_contains: row.4,
        action: parse_value::<InboxRuleAction>(row.5, "inbox rule action")?,
        enabled: row.6,
        hit_count: row.7,
        last_hit_at: row.8,
        created_at: row.9,
        updated_at: row.10,
    })
}
