//! Shared I/O for loading a [`CommitPlan`] from a file path or stdin.
//!
//! Both `plan-check` and `plan-diff` accept a `CommitPlan` via `--plan <path>`
//! or stdin when no path is given. This module provides the single shared
//! loader so the logic lives in exactly one place.

use std::io::{self, Read};

use crate::error::PgsError;
use crate::models::CommitPlan;

/// Load a [`CommitPlan`] from `plan_path` when `Some`, or from stdin when `None`.
///
/// # Errors
/// Returns [`PgsError::Io`] when the file cannot be read (including stdin
/// failures), or [`PgsError::InvalidSelection`] when the JSON is malformed.
pub fn load_commit_plan(plan_path: Option<&str>) -> Result<CommitPlan, PgsError> {
    let raw = if let Some(path) = plan_path {
        std::fs::read_to_string(path).map_err(|e| PgsError::Io {
            path: path.into(),
            source: e,
        })?
    } else {
        let mut buf = String::new();
        io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| PgsError::Io {
                path: "<stdin>".into(),
                source: e,
            })?;
        buf
    };

    serde_json::from_str(&raw).map_err(|e| PgsError::InvalidSelection {
        detail: format!("malformed CommitPlan JSON: {e}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    #[test]
    fn load_commit_plan_from_path_parses() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        write!(
            tmp,
            r#"{{"version":"v1","commits":[{{"selections":["f.rs"]}}]}}"#
        )
        .unwrap();
        let plan = load_commit_plan(Some(tmp.path().to_str().unwrap()))
            .expect("should parse valid plan from file");
        assert_eq!(plan.version, "v1");
        assert_eq!(plan.commits.len(), 1);
    }

    #[test]
    fn load_commit_plan_malformed_returns_user_error() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        write!(tmp, "not valid json").unwrap();
        let err = load_commit_plan(Some(tmp.path().to_str().unwrap()))
            .expect_err("malformed JSON must fail");
        match err {
            PgsError::InvalidSelection { detail } => {
                assert!(
                    detail.starts_with("malformed CommitPlan JSON:"),
                    "unexpected detail: {detail}"
                );
            }
            other => panic!("expected InvalidSelection, got: {other:?}"),
        }
    }

    #[test]
    fn load_commit_plan_missing_path_returns_io_error() {
        let err = load_commit_plan(Some("/tmp/does-not-exist-pgs-plan-io-test.json"))
            .expect_err("missing file must fail");
        assert!(
            matches!(err, PgsError::Io { .. }),
            "expected Io error, got: {err:?}"
        );
    }
}
