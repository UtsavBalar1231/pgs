use crate::error::AgstageError;
use crate::git::{diff, repo};
use crate::output::view::{CommandOutput, StatusOutput};

pub fn execute(repo_path: Option<&str>, context: u32) -> Result<CommandOutput, AgstageError> {
    let repository = repo::open(repo_path)?;
    let d = diff::diff_head_to_index(&repository, context)?;
    let report = diff::build_status_report(&d)?;

    Ok(StatusOutput::from(report).into())
}
