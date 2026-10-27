use crate::error::{Result, SizelintError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{Level, debug, span};

const DEFAULT_CONFIG_TOML: &str = include_str!("assets/config.toml");

const CONFIG_FILENAMES: &[&str] = &["sizelint.toml", ".sizelint.toml"];

fn default_true() -> bool {
    true
}

/// Fully resolved configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    #[serde(flatten)]
    pub sizelint: SizelintConfig,
    pub rules: Option<RulesConfig>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SizelintConfig {
    /// Maximum file size (e.g., "10MB", "1GB")
    pub max_file_size: Option<String>,

    /// Warning threshold for file size
    pub warn_file_size: Option<String>,

    /// Patterns to exclude from checking
    pub excludes: Vec<String>,

    /// Check only staged files
    pub check_staged: bool,

    /// Check working tree files
    pub check_working_tree: bool,

    /// Default git revision range for file discovery
    pub git: Option<String>,

    /// Respect .gitignore patterns
    pub respect_gitignore: bool,

    /// Treat warnings as errors
    pub fail_on_warn: bool,
}

#[derive(Debug, Deserialize)]
struct UserConfig {
    max_file_size: Option<String>,
    warn_file_size: Option<String>,
    excludes: Option<Vec<String>>,
    check_staged: Option<bool>,
    check_working_tree: Option<bool>,
    git: Option<String>,
    respect_gitignore: Option<bool>,
    fail_on_warn: Option<bool>,
    rules: Option<RulesConfig>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RulesConfig {
    #[serde(flatten)]
    pub rules: HashMap<String, RuleDefinition>,
}

fn default_priority() -> i32 {
    100
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RuleDefinition {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub description: String,
    // Higher numbers = higher priority
    #[serde(default = "default_priority")]
    pub priority: i32,

    // File size rule parameters
    pub max_size: Option<String>,
    pub warn_size: Option<String>,
    #[serde(default)]
    pub includes: Vec<String>,
    #[serde(default)]
    pub excludes: Vec<String>,

    // Match-based violations
    #[serde(default)]
    pub warn_on_match: bool,
    #[serde(default)]
    pub error_on_match: bool,

    // Actionable suggestion shown to users when this rule triggers
    #[serde(default)]
    pub suggestion: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        toml::from_str(DEFAULT_CONFIG_TOML).expect("Embedded default config must be valid")
    }
}

impl RulesConfig {
    pub fn get_enabled_rules(&self) -> Vec<(&String, &RuleDefinition)> {
        self.rules.iter().filter(|(_, rule)| rule.enabled).collect()
    }
}

impl Config {
    /// Loads the config file at `path`.
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let _span = span!(Level::DEBUG, "Config::load", path = %path.display()).entered();

        let content = std::fs::read_to_string(path)
            .map_err(|e| SizelintError::config_read(path.to_path_buf(), e))?;
        let user: UserConfig =
            toml::from_str(&content).map_err(|e| SizelintError::config_parse(path, content, e))?;

        let mut config = Self::default();
        config.apply(user);

        debug!("Config loaded and merged with defaults");
        Ok(config)
    }

    fn apply(&mut self, user: UserConfig) {
        let s = &mut self.sizelint;
        if user.max_file_size.is_some() {
            s.max_file_size = user.max_file_size;
        }
        if user.warn_file_size.is_some() {
            s.warn_file_size = user.warn_file_size;
        }
        if let Some(excludes) = user.excludes {
            s.excludes = excludes;
        }
        if let Some(check_staged) = user.check_staged {
            s.check_staged = check_staged;
        }
        if let Some(check_working_tree) = user.check_working_tree {
            s.check_working_tree = check_working_tree;
        }
        if user.git.is_some() {
            s.git = user.git;
        }
        if let Some(respect_gitignore) = user.respect_gitignore {
            s.respect_gitignore = respect_gitignore;
        }
        if let Some(fail_on_warn) = user.fail_on_warn {
            s.fail_on_warn = fail_on_warn;
        }

        if let Some(user_rules) = user.rules {
            self.rules
                .get_or_insert_with(RulesConfig::default)
                .rules
                .extend(user_rules.rules);
        }
    }

    pub fn find_config_file<P: AsRef<Path>>(start_dir: P) -> Option<PathBuf> {
        let mut current_dir = start_dir.as_ref().to_path_buf();
        loop {
            for filename in CONFIG_FILENAMES {
                let config_path = current_dir.join(filename);
                if config_path.exists() {
                    return Some(config_path);
                }
            }
            if !current_dir.pop() {
                break;
            }
        }
        None
    }

    pub fn create_default_config() -> String {
        DEFAULT_CONFIG_TOML.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default_loads() {
        let config = Config::default();

        assert!(config.rules.is_some());
        let rules = config.rules.as_ref().unwrap();
        assert!(!rules.rules.is_empty());
        assert!(rules.rules.contains_key("medium_files"));
    }

    #[test]
    fn test_embedded_config_is_valid_toml() {
        let result = toml::from_str::<Config>(DEFAULT_CONFIG_TOML);
        assert!(result.is_ok(), "Embedded config must be valid TOML");
    }

    #[test]
    fn parse_error_labels_the_offending_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sizelint.toml");
        let content = "[sizelint]\nmax_file_size = \"10MB\"\nexcludes = [\n";
        std::fs::write(&path, content).unwrap();

        let err = Config::load(&path).unwrap_err();
        let SizelintError::ConfigParse { span, .. } = &err else {
            panic!("expected ConfigParse, got {err:?}");
        };
        assert!(
            span.offset() >= content.find("excludes").unwrap(),
            "span {span:?} should point into the broken array"
        );

        let mut rendered = String::new();
        miette::GraphicalReportHandler::new_themed(miette::GraphicalTheme::unicode_nocolor())
            .render_report(&mut rendered, &err)
            .unwrap();
        assert!(rendered.contains("excludes = ["), "{rendered}");
    }

    fn load_str(content: &str) -> Result<Config> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sizelint.toml");
        std::fs::write(&path, content).unwrap();
        Config::load(&path)
    }

    #[test]
    fn test_unset_user_keys_keep_default_values() {
        let config = load_str(r#"max_file_size = "3MB""#).unwrap();
        let defaults = Config::default();

        assert_eq!(config.sizelint.max_file_size.as_deref(), Some("3MB"));
        assert_eq!(config.sizelint.check_staged, defaults.sizelint.check_staged);
        assert_eq!(
            config.sizelint.respect_gitignore,
            defaults.sizelint.respect_gitignore
        );
        assert_eq!(config.sizelint.excludes, defaults.sizelint.excludes);
    }

    #[test]
    fn test_set_user_booleans_override_default_values() {
        let config = load_str("respect_gitignore = false\nfail_on_warn = true").unwrap();

        assert!(!config.sizelint.respect_gitignore);
        assert!(config.sizelint.fail_on_warn);
    }

    #[test]
    fn test_user_rule_replaces_default_rule_of_same_name() {
        let config = load_str(
            r#"
[rules.default]
max_size = "5MB"

[rules.images]
includes = ["*.png"]
max_size = "1MB"
"#,
        )
        .unwrap();
        let rules = &config.rules.as_ref().unwrap().rules;

        assert_eq!(rules["default"].max_size.as_deref(), Some("5MB"));
        assert_eq!(rules["default"].description, "");
        assert_eq!(rules["images"].max_size.as_deref(), Some("1MB"));
        assert!(rules.contains_key("medium_files"));
    }

    #[test]
    fn test_user_value_of_wrong_type_points_at_the_value() {
        let err = load_str(r#"fail_on_warn = "yes""#).unwrap_err();

        let SizelintError::ConfigParse { span, .. } = err else {
            panic!("expected ConfigParse, got {err:?}");
        };
        assert_eq!((span.offset(), span.len()), (15, 5));
    }

    #[test]
    fn test_init_output_loads_as_user_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sizelint.toml");
        std::fs::write(&path, Config::create_default_config()).unwrap();

        assert_eq!(Config::load(&path).unwrap(), Config::default());
    }

    #[test]
    fn test_load_keeps_shipped_default_rule() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("custom.toml");
        std::fs::write(&path, "fail_on_warn = true\n").unwrap();

        let config = Config::load(&path).unwrap();

        let default_rule = &config.rules.as_ref().unwrap().rules["default"];
        assert_eq!(default_rule.description, "Default file size check");
        assert!(default_rule.suggestion.is_some());
        assert_eq!(config.sizelint.max_file_size.as_deref(), Some("2MB"));
        assert!(config.sizelint.fail_on_warn);
    }
}
