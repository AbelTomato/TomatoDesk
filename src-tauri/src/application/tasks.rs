use crate::{
    domain::{NewTask, Task, TaskStatus},
    infrastructure::db::{Database, DbResult},
};

/// 手动创建的待办：来源固定为 `manual`，阶段二不接收件箱。
pub(crate) fn create_task(
    database: &Database,
    title: String,
    note: Option<String>,
) -> DbResult<Task> {
    database.tasks().create(NewTask {
        title,
        note,
        origin_kind: Some("manual".into()),
        origin_id: None,
    })
}

pub(crate) fn update_task(
    database: &Database,
    id: &str,
    title: String,
    note: Option<String>,
) -> DbResult<Option<Task>> {
    database.tasks().update(id, &title, note.as_deref())
}

/// 完成 / 取消 / 重开共用一条路径，状态语义只在 `TaskStatus` 里定义。
pub(crate) fn set_status(
    database: &Database,
    id: &str,
    status: TaskStatus,
) -> DbResult<Option<Task>> {
    database.tasks().set_status(id, status)
}
