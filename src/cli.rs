use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{Shell, generate};
use std::io;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "sizelint",
    about = env!("CARGO_PKG_DESCRIPTION"),
    version,
    author
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Configuration file path
    #[arg(short, long, value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// Enable debug output (or set SIZELINT_LOG for fine-grained control)
    #[arg(long)]
    pub debug: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Check files for size violations
    #[command(alias = "c")]
    Check(CheckArgs),

    /// Initialize sizelint configuration
    #[command(alias = "i")]
    Init {
        /// Force overwrite existing configuration
        #[arg(short, long)]
        force: bool,
        /// Print the default configuration to stdout
        #[arg(long)]
        stdout: bool,
        /// Open configuration file in editor after creation
        #[arg(long)]
        edit: bool,
    },

    /// Rule management
    #[command(alias = "r")]
    Rules {
        #[command(subcommand)]
        action: RuleAction,
    },

    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        shell: Shell,
    },
}

#[derive(Args, Debug)]
pub struct CheckArgs {
    /// Paths to check
    pub paths: Vec<PathBuf>,

    /// Configuration file path
    #[arg(short, long, value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// Output format
    #[arg(short = 'f', long, default_value = "human")]
    pub format: OutputFormat,

    /// Check only staged files (git diff --staged)
    #[arg(long)]
    pub staged: bool,

    /// Check working tree files
    #[arg(long, conflicts_with = "git")]
    pub working_tree: bool,

    /// Check files changed in a git revision range (e.g. "main", "main..HEAD", "main...feature")
    #[arg(long, value_name = "RANGE", conflicts_with_all = ["staged", "working_tree"])]
    pub git: Option<String>,

    /// Skip git history scanning for deleted blobs (only check files at HEAD)
    #[arg(long, requires = "git")]
    pub no_history: bool,

    /// Quiet mode (only show violations)
    #[arg(short, long)]
    pub quiet: bool,

    /// Treat warnings as errors
    #[arg(long)]
    pub fail_on_warn: bool,
}

#[derive(Subcommand, Debug)]
pub enum RuleAction {
    /// List available rules
    #[command(alias = "l")]
    List,
    /// Show rule documentation
    #[command(alias = "d")]
    Describe { rule: String },
}

#[derive(ValueEnum, Debug, Clone, Copy)]
pub enum OutputFormat {
    /// Human-readable output
    Human,
    /// JSON output
    Json,
}

impl Cli {
    pub fn generate_completion(shell: Shell) {
        generate(shell, &mut Self::command(), "sizelint", &mut io::stdout());
    }
}
