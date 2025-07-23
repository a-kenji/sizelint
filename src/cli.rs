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

    /// Verbose output
    #[arg(short, long)]
    pub verbose: bool,

    /// Log level
    #[arg(long, default_value = "info")]
    pub log_level: LogLevel,
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
        #[arg(long)]
        working_tree: bool,

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
    /// Summary only
    Summary,
}

#[derive(ValueEnum, Debug, Clone)]
pub enum LogLevel {
    /// Trace level logging
    Trace,
    /// Debug level logging
    Debug,
    /// Info level logging
    Info,
    /// Warning level logging
    Warn,
    /// Error level logging
    Error,
}

impl LogLevel {
    pub fn as_str(&self) -> &str {
        match self {
            LogLevel::Trace => "trace",
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        }
    }
}

impl Cli {
    pub fn get_command(&self) -> Commands {
        self.command.clone()
    }

    pub fn get_paths(&self) -> Vec<PathBuf> {
        match &self.command {
            Commands::Check { paths, .. } if !paths.is_empty() => paths.clone(),
            Commands::Check { .. } => vec![PathBuf::from(".")],
            _ => vec![],
        }
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
