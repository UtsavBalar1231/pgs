use std::ffi::OsString;

mod commit;
pub mod log;
pub mod mcp_adapter;
mod overview;
pub mod plan_apply;
pub mod plan_check;
pub mod plan_diff;
pub(crate) mod plan_io;
mod scan;
pub mod split;
mod stage;
mod status;
mod unstage;
mod work_plan;

use clap::{CommandFactory, Parser, Subcommand, ValueEnum, error::ErrorKind};

use crate::error::PgsError;
use crate::output;
use crate::output::view::OutputCommand;

pub use crate::output::view::{CliErrorOutput, CommandOutput};

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

/// Exit code for a successful run whose envelope still reports an outcome the
/// caller must act on: `plan-check` issues and `plan-diff` drift exit 1 while the
/// full envelope is still printed.
#[must_use]
pub const fn exit_override(output: &CommandOutput) -> Option<i32> {
    match output {
        CommandOutput::PlanCheck(plan_check) if plan_check.has_issues() => Some(1),
        CommandOutput::PlanDiff(plan_diff) if plan_diff.has_drift() => Some(1),
        CommandOutput::PlanApply(plan_apply) => plan_apply.failure_exit_code(),
        _ => None,
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
            Command::Status(_) => OutputCommand::Status,
            Command::Commit(_) => OutputCommand::Commit,
            Command::Log(_) => OutputCommand::Log,
            Command::Overview => OutputCommand::Overview,
            Command::SplitHunk(_) => OutputCommand::SplitHunk,
            Command::PlanCheck(_) => OutputCommand::PlanCheck,
            Command::PlanDiff(_) => OutputCommand::PlanDiff,
            Command::PlanApply(_) => OutputCommand::PlanApply,
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
    Status(status::StatusArgs),
    /// Create a git commit from staged changes.
    Commit(commit::CommitArgs),
    /// Show recent commit history.
    Log(log::LogArgs),
    /// Unified view of both unstaged (scan) and staged (status) changes.
    Overview,
    /// Classify a hunk's contiguous runs (addition, deletion, mixed).
    #[command(name = "split-hunk")]
    SplitHunk(split::SplitArgs),
    /// Validate an agent-supplied `CommitPlan` against a fresh scan.
    #[command(name = "plan-check")]
    PlanCheck(plan_check::PlanCheckArgs),
    /// Reconcile a saved `CommitPlan` against a fresh scan: `still_valid` / shifted / gone.
    #[command(name = "plan-diff")]
    PlanDiff(plan_diff::PlanDiffArgs),
    /// Apply a `CommitPlan`: stage and commit each planned commit in order.
    #[command(name = "plan-apply")]
    PlanApply(plan_apply::PlanApplyArgs),
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
pub fn run(parsed: ParsedCli) -> Result<CommandOutput, PgsError> {
    let Cli {
        repo,
        output: _,
        json: _,
        context,
        command,
    } = parsed.cli;
    let context = context.max(1);

    match command {
        Command::Scan(args) => scan::execute(repo.as_deref(), context, args),
        Command::Stage(args) => stage::execute(repo.as_deref(), context, args),
        Command::Unstage(args) => unstage::execute(repo.as_deref(), context, args),
        Command::Status(args) => status::execute(repo.as_deref(), context, args.full),
        Command::Commit(args) => commit::execute(repo.as_deref(), context, args, true),
        Command::Log(args) => log::execute(repo.as_deref(), args),
        Command::Overview => overview::execute(repo.as_deref(), context),
        Command::SplitHunk(args) => split::execute(repo.as_deref(), context, args),
        Command::PlanCheck(args) => plan_check::execute(repo.as_deref(), context, args),
        Command::PlanDiff(args) => plan_diff::execute(repo.as_deref(), context, args),
        Command::PlanApply(args) => plan_apply::execute(repo.as_deref(), context, args),
    }
}

/// Render a successful command result in the requested output mode.
///
/// # Errors
///
/// Returns an error if the output cannot be serialized.
pub fn render(output: &CommandOutput, output_mode: OutputMode) -> Result<String, PgsError> {
    output::render(output, output_mode)
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

pub fn parse_failure(error: &clap::Error) -> CliErrorOutput {
    let message = error.to_string().trim_end().to_owned();
    CliErrorOutput::parse(parse_error_code(error.kind()), message, error.exit_code())
}

pub fn runtime_failure(command: OutputCommand, error: &PgsError) -> CliErrorOutput {
    CliErrorOutput::runtime(command, error.code(), error.to_string(), error.exit_code())
}

pub fn render_error(
    renderable: &CliErrorOutput,
    output_mode: OutputMode,
) -> Result<String, PgsError> {
    match output_mode {
        OutputMode::Json => output::json::render_error(renderable),
        OutputMode::Text => output::text::render_error(renderable),
    }
}
