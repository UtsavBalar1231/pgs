mod commit;
mod scan;
mod stage;
mod status;
mod unstage;

use clap::{Parser, Subcommand};

use crate::error::AgstageError;

/// Programmatic git staging CLI for AI agents.
#[derive(Parser)]
#[command(name = "agstage", version, about)]
pub struct Cli {
    /// Repository path (auto-discover via .git if omitted).
    #[arg(long, global = true)]
    pub repo: Option<String>,

    /// Context lines for diff generation (affects scan hunk boundaries, min: 1).
    #[arg(long, global = true, default_value = "3")]
    pub context: u32,

    #[command(subcommand)]
    pub command: Command,
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

/// Run the CLI — parse args, dispatch to command handler, print JSON.
pub fn run() -> Result<(), AgstageError> {
    let cli = Cli::parse();
    let context = cli.context.max(1);

    match cli.command {
        Command::Scan(args) => scan::execute(cli.repo.as_deref(), context, args),
        Command::Stage(args) => stage::execute(cli.repo.as_deref(), context, args),
        Command::Unstage(args) => unstage::execute(cli.repo.as_deref(), context, args),
        Command::Status => status::execute(cli.repo.as_deref(), context),
        Command::Commit(args) => commit::execute(cli.repo.as_deref(), args),
    }
}
