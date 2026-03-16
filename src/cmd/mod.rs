use std::ffi::OsString;

mod commit;
pub mod mcp_adapter;
mod scan;
mod stage;
mod status;
mod unstage;

use clap::{CommandFactory, Parser, Subcommand, ValueEnum, error::ErrorKind};

use crate::error::PgsError;
use crate::output;
use crate::output::view::{CliErrorOutput, OutputCommand};

/// Output format requested at the CLI boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum OutputMode {
    /// Human-readable output.
    Text,
    /// Machine-readable JSON output.
    Json,
}

/// Parsed CLI arguments with the resolved output mode.
pub struct ParsedCli {
    /// Parsed clap arguments.
    pub cli: Cli,
    /// Output mode selected from `--output` and `--json`.
    pub output_mode: OutputMode,
}

pub struct RenderableOutput(output::view::CommandOutput);

pub struct RenderableError(CliErrorOutput);

impl RenderableOutput {
    const fn new(output: output::view::CommandOutput) -> Self {
        Self(output)
    }
}

impl RenderableError {
    const fn new(output: CliErrorOutput) -> Self {
        Self(output)
    }
}

/// Non-interactive git staging at file, hunk, and line granularity.
#[derive(Parser)]
#[command(name = "pgs", version, about)]
pub struct Cli {
    /// Repository path (auto-discover via .git if omitted).
    #[arg(long, global = true)]
    pub repo: Option<String>,

    /// Output mode (`text` by default, `json` when explicitly requested).
    #[arg(long, global = true, value_enum)]
    pub output: Option<OutputMode>,

    /// Convenience alias for `--output json`.
    #[arg(long, global = true)]
    pub json: bool,

    /// Context lines for diff generation (affects scan hunk boundaries, min: 1).
    #[arg(long, global = true, default_value = "3")]
    pub context: u32,

    #[command(subcommand)]
    pub command: Command,
}

impl ParsedCli {
    pub const fn command(&self) -> OutputCommand {
        match &self.cli.command {
            Command::Scan(_) => OutputCommand::Scan,
            Command::Stage(_) => OutputCommand::Stage,
            Command::Unstage(_) => OutputCommand::Unstage,
            Command::Status => OutputCommand::Status,
            Command::Commit(_) => OutputCommand::Commit,
        }
    }
}

#[derive(Subcommand)]
pub enum Command {
    /// Discover all unstaged changes in the working tree.
    Scan(scan::ScanArgs),
    /// Stage selected changes into the index.
    Stage(stage::StageArgs),
    /// Remove selected changes from the index.
    Unstage(unstage::UnstageArgs),
    /// Show currently staged changes (HEAD vs index).
    Status,
    /// Create a git commit from staged changes.
    Commit(commit::CommitArgs),
}

impl Cli {
    fn resolve_output_mode(&self) -> Result<OutputMode, clap::Error> {
        match (self.json, self.output) {
            (true, Some(OutputMode::Text)) => Err(Self::command().error(
                ErrorKind::ArgumentConflict,
                "--json conflicts with --output text",
            )),
            (true, Some(OutputMode::Json) | None) => Ok(OutputMode::Json),
            (false, Some(mode)) => Ok(mode),
            (false, None) => Ok(OutputMode::Text),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DetectedOutputMode {
    Unspecified,
    Resolved(OutputMode),
    Ambiguous,
}

impl DetectedOutputMode {
    fn merge(self, next: Option<OutputMode>) -> Self {
        match (self, next) {
            (Self::Ambiguous, _) => Self::Ambiguous,
            (state, None) => state,
            (Self::Unspecified, Some(mode)) => Self::Resolved(mode),
            (Self::Resolved(current), Some(next_mode)) if current == next_mode => {
                Self::Resolved(current)
            }
            (Self::Resolved(_), Some(_)) => Self::Ambiguous,
        }
    }

    const fn into_option(self) -> Option<OutputMode> {
        match self {
            Self::Resolved(mode) => Some(mode),
            Self::Unspecified | Self::Ambiguous => None,
        }
    }
}

fn parse_output_mode_token(token: &str) -> Option<OutputMode> {
    match token {
        "text" => Some(OutputMode::Text),
        "json" => Some(OutputMode::Json),
        _ => None,
    }
}

/// Parse CLI arguments without exiting the process.
///
/// # Errors
///
/// Returns a clap error when argument parsing fails or output flags conflict.
pub fn parse_args<I, T>(args: I) -> Result<ParsedCli, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = Cli::try_parse_from(args)?;
    let output_mode = cli.resolve_output_mode()?;

    Ok(ParsedCli { cli, output_mode })
}

/// Best-effort output mode detection from raw argv.
///
/// # Errors
///
/// This function does not return errors. It yields `None` when the mode is not
/// requested or conflicting raw flags make the requested mode ambiguous.
pub fn detect_output_mode(args: &[OsString]) -> Option<OutputMode> {
    let mut detected = DetectedOutputMode::Unspecified;
    let mut iter = args.iter().skip(1);

    while let Some(arg) = iter.next() {
        let token = arg.to_string_lossy();

        if token == "--" {
            break;
        }

        if token == "--json" {
            detected = detected.merge(Some(OutputMode::Json));
            continue;
        }

        if let Some(value) = token.strip_prefix("--output=") {
            detected = detected.merge(parse_output_mode_token(value));
            continue;
        }

        if token == "--output" {
            let Some(value) = iter.next() else {
                break;
            };
            detected = detected.merge(parse_output_mode_token(&value.to_string_lossy()));
        }
    }

    detected.into_option()
}

/// Run the CLI after parsing and validating global arguments.
///
/// # Errors
///
/// Returns any runtime error produced by the selected command handler.
pub fn run(parsed: ParsedCli) -> Result<Option<RenderableOutput>, PgsError> {
    let Cli {
        repo,
        output: _,
        json: _,
        context,
        command,
    } = parsed.cli;
    let context = context.max(1);

    match command {
        Command::Scan(args) => Ok(Some(RenderableOutput::new(scan::execute(
            repo.as_deref(),
            context,
            args,
        )?))),
        Command::Stage(args) => Ok(Some(RenderableOutput::new(stage::execute(
            repo.as_deref(),
            context,
            args,
        )?))),
        Command::Unstage(args) => Ok(Some(RenderableOutput::new(unstage::execute(
            repo.as_deref(),
            context,
            args,
        )?))),
        Command::Status => Ok(Some(RenderableOutput::new(status::execute(
            repo.as_deref(),
            context,
        )?))),
        Command::Commit(args) => Ok(Some(RenderableOutput::new(commit::execute(
            repo.as_deref(),
            args,
        )?))),
    }
}

pub fn render(renderable: &RenderableOutput, output_mode: OutputMode) -> Result<String, PgsError> {
    output::render(&renderable.0, output_mode)
}

const fn parse_error_code(kind: ErrorKind) -> &'static str {
    match kind {
        ErrorKind::ArgumentConflict => "argument_conflict",
        ErrorKind::UnknownArgument => "unknown_argument",
        ErrorKind::InvalidValue => "invalid_value",
        ErrorKind::InvalidSubcommand => "invalid_subcommand",
        ErrorKind::MissingRequiredArgument => "missing_required_argument",
        _ => "parse_error",
    }
}

pub fn parse_failure(error: &clap::Error) -> RenderableError {
    let message = error.to_string().trim_end().to_owned();
    RenderableError::new(CliErrorOutput::parse(
        parse_error_code(error.kind()),
        message,
        error.exit_code(),
    ))
}

pub fn runtime_failure(command: OutputCommand, error: &PgsError) -> RenderableError {
    RenderableError::new(CliErrorOutput::runtime(
        command,
        error.code(),
        error.to_string(),
        error.exit_code(),
    ))
}

pub fn render_error(
    renderable: &RenderableError,
    output_mode: OutputMode,
) -> Result<String, PgsError> {
    match output_mode {
        OutputMode::Json => output::json::render_error(&renderable.0),
        OutputMode::Text => output::text::render_error(&renderable.0),
    }
}
