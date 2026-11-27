mod database;
mod error;
mod repositories;

pub use database::Database;
pub use error::{DbError, DbResult};
pub use repositories::{
    CaptureRepository, EmailSourceRow, GITHUB_SOURCE_ID, GithubReleaseRepository,
    GithubRepoRepository, GithubThreadRepository, InboxMessageRepository, InboxReadBackRepository,
    InboxRuleRepository, InboxSourceRepository, MANUAL_SOURCE_ID, SyncRunRepository, TagRepository,
    TaskRepository,
};

#[cfg(test)]
mod tests;
