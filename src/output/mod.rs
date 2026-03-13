pub mod json;
pub mod text;
pub mod view;

use crate::cmd::OutputMode;
use crate::error::AgstageError;

pub fn render(
    output: &view::CommandOutput,
    output_mode: OutputMode,
) -> Result<String, AgstageError> {
    match output_mode {
        OutputMode::Json => json::render(output),
        OutputMode::Text => text::render(output),
    }
}
