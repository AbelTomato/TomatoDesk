mod database;
mod error;
mod repositories;

pub use database::Database;
pub use error::{DbError, DbResult};
pub use repositories::{
    CaptureRepository, GITHUB_SOURCE_ID, GithubReleaseRepository, GithubRepoRepository,
    GithubThreadRepository, InboxMessageRepository, InboxReadBackRepository, InboxSourceRepository,
    MANUAL_SOURCE_ID, SyncRunRepository, TagRepository, TaskRepository,
};

#[cfg(test)]
mod tests;
