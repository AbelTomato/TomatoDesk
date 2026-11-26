mod capture;
mod github_release;
mod github_repo;
mod github_thread;
mod inbox_message;
mod inbox_source;
mod sync_run;
mod tag;
mod task;

pub use capture::CaptureRepository;
pub use github_release::GithubReleaseRepository;
pub use github_repo::GithubRepoRepository;
pub use github_thread::GithubThreadRepository;
pub use inbox_message::InboxMessageRepository;
pub use inbox_source::{GITHUB_SOURCE_ID, InboxSourceRepository, MANUAL_SOURCE_ID};
pub use sync_run::SyncRunRepository;
pub use tag::TagRepository;
pub use task::TaskRepository;

use std::str::FromStr;

use crate::infrastructure::db::{DbError, DbResult};

fn parse_value<T>(value: String, field: &str) -> DbResult<T>
where
    T: FromStr<Err = String>,
{
    value
        .parse()
        .map_err(|error| DbError::InvalidData(format!("{field}: {error}")))
}
