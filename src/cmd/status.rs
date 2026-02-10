use crate::error::AgstageError;
use crate::git::{diff, repo};

pub fn execute(repo_path: Option<&str>, context: u32) -> Result<(), AgstageError> {
    let repository = repo::open(repo_path)?;
    let d = diff::diff_head_to_index(&repository, context)?;
    let report = diff::build_status_report(&d)?;

    let json = serde_json::to_string_pretty(&report)?;
    println!("{json}");
    Ok(())
}
