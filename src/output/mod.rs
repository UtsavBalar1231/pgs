pub mod json;
pub mod text;
pub mod view;

use crate::cmd::OutputMode;
use crate::error::PgsError;

pub fn render(output: &view::CommandOutput, output_mode: OutputMode) -> Result<String, PgsError> {
    match output_mode {
        OutputMode::Json => json::render(output),
        OutputMode::Text => text::render(output),
    }
}
