use miette::{Diagnostic, NamedSource, SourceSpan};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug, Diagnostic)]
#[non_exhaustive]
pub enum SizelintError {
    #[error(transparent)]
    #[diagnostic(transparent)]
    Git(#[from] crate::git::GitError),

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

    #[error("Failed to parse config file {}", .src.name())]
    #[diagnostic(code(sizelint::config::parse_error))]
    ConfigParse {
        #[source_code]
        src: NamedSource<String>,
        #[label("{message}")]
        span: SourceSpan,
        message: String,
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

    #[error("Failed to start editor '{editor}'")]
    #[diagnostic(
        code(sizelint::editor::exec),
        help("Check that the editor is installed and on your PATH, or set $VISUAL / $EDITOR")
    )]
    EditorExec {
        editor: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Editor '{editor}' exited with status {exit_code}")]
    #[diagnostic(code(sizelint::editor::failed))]
    EditorFailed { editor: String, exit_code: i32 },

    #[error("Invalid size format '{input}': {reason}")]
    #[diagnostic(
        code(sizelint::rule::invalid_size_format),
        help("Use formats like: 10MB, 1GB, 500KB, 1024B")
    )]
    InvalidSizeFormat { input: String, reason: String },

    #[error("Failed to write results")]
    #[diagnostic(code(sizelint::output::write_failed))]
    OutputWrite {
        #[source]
        source: std::io::Error,
    },
}

pub type Result<T> = miette::Result<T, SizelintError>;

impl SizelintError {
    pub fn config_read(path: PathBuf, source: std::io::Error) -> Self {
        Self::ConfigRead { path, source }
    }

    pub fn config_parse(path: &Path, content: String, source: toml::de::Error) -> Self {
        let span = source.span().unwrap_or(0..0).into();
        Self::ConfigParse {
            src: NamedSource::new(path.display().to_string(), content),
            span,
            message: source.message().to_string(),
        }
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

    pub fn invalid_size_format(input: String, reason: String) -> Self {
        Self::InvalidSizeFormat { input, reason }
    }
}
