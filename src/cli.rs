use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{Shell, generate};
use std::io;
use std::path::PathBuf;

const SUPPORTED_SHELLS: &[(&str, Shell)] = &[
    ("bash", Shell::Bash),
    ("zsh", Shell::Zsh),
    ("fish", Shell::Fish),
    ("powershell", Shell::PowerShell),
    ("elvish", Shell::Elvish),
];

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

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Check files for size violations
    #[command(alias = "c")]
    Check {
        /// Paths to check
        paths: Vec<PathBuf>,

        /// Configuration file path
        #[arg(short, long, value_name = "FILE")]
        config: Option<PathBuf>,

        /// Output format
        #[arg(short = 'f', long, default_value = "human")]
        format: OutputFormat,

        /// Check only staged files (git diff --staged)
        #[arg(long)]
        staged: bool,

        /// Check working tree files
        #[arg(long, conflicts_with = "git")]
        working_tree: bool,

        /// Check files changed in a git revision range (e.g. "main", "main..HEAD", "main...feature")
        #[arg(long, value_name = "RANGE", conflicts_with_all = ["staged", "working_tree"])]
        git: Option<String>,

        /// Skip git history scanning for deleted blobs (only check files at HEAD)
        #[arg(long, requires = "git")]
        no_history: bool,

        /// Quiet mode (only show violations)
        #[arg(short, long)]
        quiet: bool,

        /// Treat warnings as errors
        #[arg(long)]
        fail_on_warn: bool,
    },

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
        shell: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum RuleAction {
    /// List available rules
    #[command(alias = "l")]
    List,
    /// Show rule documentation
    #[command(alias = "d")]
    Describe { rule: String },
}

#[derive(ValueEnum, Debug, Clone)]
pub enum OutputFormat {
    /// Human-readable output
    Human,
    /// JSON output
    Json,
}

impl Cli {
    pub fn get_command(&self) -> Commands {
        self.command.clone()
    }

    pub fn get_format(&self) -> OutputFormat {
        match &self.command {
            Commands::Check { format, .. } => format.clone(),
            _ => OutputFormat::Human,
        }
    }

    pub fn get_quiet(&self) -> bool {
        match &self.command {
            Commands::Check { quiet, .. } => *quiet,
            _ => false,
        }
    }

    pub fn get_staged(&self) -> bool {
        match &self.command {
            Commands::Check { staged, .. } => *staged,
            _ => false,
        }
    }

    pub fn get_working_tree(&self) -> bool {
        match &self.command {
            Commands::Check { working_tree, .. } => *working_tree,
            _ => false,
        }
    }

    pub fn get_git(&self) -> Option<String> {
        match &self.command {
            Commands::Check { git, .. } => git.clone(),
            _ => None,
        }
    }

    pub fn get_no_history(&self) -> bool {
        match &self.command {
            Commands::Check { no_history, .. } => *no_history,
            _ => false,
        }
    }

    pub fn get_fail_on_warn(&self) -> bool {
        match &self.command {
            Commands::Check { fail_on_warn, .. } => *fail_on_warn,
            _ => false,
        }
    }

    pub fn get_check_config(&self) -> Option<PathBuf> {
        match &self.command {
            Commands::Check { config, .. } => config.clone(),
            _ => None,
        }
    }

    pub fn parse_shell(shell_str: &str) -> std::result::Result<Shell, String> {
        let shell_lower = shell_str.to_lowercase();
        SUPPORTED_SHELLS
            .iter()
            .find(|(name, _)| *name == shell_lower)
            .map(|(_, shell)| *shell)
            .ok_or_else(|| {
                let supported: Vec<&str> = SUPPORTED_SHELLS.iter().map(|(name, _)| *name).collect();
                format!(
                    "Unsupported shell: {}. Supported shells: {}",
                    shell_str,
                    supported.join(", ")
                )
            })
    }

    pub fn generate_completion(shell_str: &str) -> std::result::Result<(), String> {
        let shell = Self::parse_shell(shell_str)?;
        let mut cmd = Self::command();
        generate(shell, &mut cmd, "sizelint", &mut io::stdout());
        Ok(())
    }
}
