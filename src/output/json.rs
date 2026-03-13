use crate::error::AgstageError;

use super::view::{CliErrorOutput, CommandOutput};

pub fn render(output: &CommandOutput) -> Result<String, AgstageError> {
    match output {
        CommandOutput::Scan(scan) => Ok(serde_json::to_string_pretty(scan)?),
        CommandOutput::Operation(operation) => Ok(serde_json::to_string_pretty(operation)?),
        CommandOutput::Status(status) => Ok(serde_json::to_string_pretty(status)?),
        CommandOutput::Commit(commit) => Ok(serde_json::to_string_pretty(commit)?),
    }
}

pub fn render_error(output: &CliErrorOutput) -> Result<String, AgstageError> {
    Ok(serde_json::to_string_pretty(output)?)
}
