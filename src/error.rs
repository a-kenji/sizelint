use miette::Diagnostic;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug, Diagnostic)]
pub enum SizelintError {
    #[error(transparent)]
    #[diagnostic(transparent)]
    Git(#[from] crate::git::GitError),

    // Configuration errors
    #[error("Config file not found")]
    #[diagnostic(
        code(sizelint::config::not_found),
        help("Create a sizelint.toml file or use --config to specify a custom path")
    )]
    ConfigNotFound { paths: Vec<PathBuf> },

    #[error("Failed to read config file {path}")]
    #[diagnostic(
        code(sizelint::config::read_error),
        help("Check that the file exists and you have read permissions")
    )]
    ConfigRead {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to parse config file {path}")]
    #[diagnostic(
        code(sizelint::config::parse_error),
        help("Check your TOML syntax - visit https://toml.io for format documentation")
    )]
    ConfigParse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("Invalid configuration: {field} = '{value}'")]
    #[diagnostic(code(sizelint::config::invalid_value), help("Expected: {expected}"))]
    ConfigInvalid {
        field: String,
        value: String,
        expected: String,
    },

    #[error("Invalid exclude pattern '{pattern}'")]
    #[diagnostic(
        code(sizelint::config::invalid_pattern),
        help("Check your glob pattern syntax - use wildcards like *.txt or **/*.rs")
    )]
    ConfigInvalidPattern {
        pattern: String,
        #[source]
        source: globset::Error,
    },

    // File system errors
    #[error("Failed to {operation} {path}")]
    #[diagnostic(code(sizelint::filesystem::operation_failed))]
    FileSystem {
        operation: String,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to get current directory")]
    #[diagnostic(
        code(sizelint::filesystem::current_dir_error),
        help("Check your working directory permissions")
    )]
    CurrentDirectory {
        #[source]
        source: std::io::Error,
    },

    // Rule execution errors
    #[error("Rule '{rule}' failed on {path}: {message}")]
    #[diagnostic(code(sizelint::rule::execution_failed))]
    RuleExecution {
        rule: String,
        path: PathBuf,
        message: String,
    },

    #[error("Invalid size format '{input}': {reason}")]
    #[diagnostic(
        code(sizelint::rule::invalid_size_format),
        help("Use formats like: 10MB, 1GB, 500KB, 1024B")
    )]
    InvalidSizeFormat { input: String, reason: String },

    // File discovery errors
    #[error("File discovery failed in {path}: {message}")]
    #[diagnostic(code(sizelint::discovery::failed))]
    FileDiscovery { path: PathBuf, message: String },

    // Auto-converted errors for external types
    #[error("JSON serialization error: {0}")]
    #[diagnostic(code(sizelint::json::serialize_error))]
    Serialize(#[from] serde_json::Error),

    #[error("I/O error: {0}")]
    #[diagnostic(code(sizelint::io::error))]
    Io(#[from] std::io::Error),
}

pub type Result<T> = miette::Result<T, SizelintError>;

impl SizelintError {
    pub fn config_read(path: PathBuf, source: std::io::Error) -> Self {
        Self::ConfigRead { path, source }
    }

    pub fn config_parse(path: PathBuf, source: toml::de::Error) -> Self {
        Self::ConfigParse { path, source }
    }

    pub fn config_invalid(field: String, value: String, expected: String) -> Self {
        Self::ConfigInvalid {
            field,
            value,
            expected,
        }
    }

    pub fn config_invalid_pattern(pattern: String, source: globset::Error) -> Self {
        Self::ConfigInvalidPattern { pattern, source }
    }

    pub fn filesystem(operation: String, path: PathBuf, source: std::io::Error) -> Self {
        Self::FileSystem {
            operation,
            path,
            source,
        }
    }

    pub fn rule_execution(rule: String, path: PathBuf, message: String) -> Self {
        Self::RuleExecution {
            rule,
            path,
            message,
        }
    }

    pub fn invalid_size_format(input: String, reason: String) -> Self {
        Self::InvalidSizeFormat { input, reason }
    }

    pub fn file_discovery(path: PathBuf, message: String) -> Self {
        Self::FileDiscovery { path, message }
    }
}
