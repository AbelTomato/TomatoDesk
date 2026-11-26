mod capture;
mod github_release;
mod github_repo;
mod github_thread;
mod sync_run;
mod tag;

pub use capture::CaptureRepository;
pub use github_release::GithubReleaseRepository;
pub use github_repo::GithubRepoRepository;
pub use github_thread::GithubThreadRepository;
pub use sync_run::SyncRunRepository;
pub use tag::TagRepository;

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
