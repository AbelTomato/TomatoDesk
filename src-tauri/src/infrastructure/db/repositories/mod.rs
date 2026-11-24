mod github_event;
mod github_repo;
mod item;
mod sync_run;
mod tag;

pub use github_event::GithubEventRepository;
pub use github_repo::GithubRepoRepository;
pub use item::ItemRepository;
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
