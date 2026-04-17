use crate::error::PgsError;

use super::view::{CliErrorOutput, CommandOutput};

pub fn render(output: &CommandOutput) -> Result<String, PgsError> {
    match output {
        CommandOutput::Scan(scan) => Ok(serde_json::to_string_pretty(scan)?),
        CommandOutput::Operation(operation) => Ok(serde_json::to_string_pretty(operation)?),
        CommandOutput::Status(status) => Ok(serde_json::to_string_pretty(status)?),
        CommandOutput::Commit(commit) => Ok(serde_json::to_string_pretty(commit)?),
        CommandOutput::Log(log) => Ok(serde_json::to_string_pretty(log)?),
        CommandOutput::Overview(overview) => Ok(serde_json::to_string_pretty(overview)?),
        CommandOutput::SplitHunk(split) => Ok(serde_json::to_string_pretty(split)?),
        CommandOutput::PlanCheck(plan_check) => Ok(serde_json::to_string_pretty(plan_check)?),
        CommandOutput::PlanDiff(plan_diff) => Ok(serde_json::to_string_pretty(plan_diff)?),
    }
}

pub fn render_error(output: &CliErrorOutput) -> Result<String, PgsError> {
    Ok(serde_json::to_string_pretty(output)?)
}
