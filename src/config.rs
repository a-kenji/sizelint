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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(flatten)]
    pub sizelint: SizelintConfig,
    pub rules: Option<RulesConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SizelintConfig {
    /// Maximum file size (e.g., "10MB", "1GB")
    pub max_file_size: Option<String>,

    /// Warning threshold for file size
    pub warn_file_size: Option<String>,

    /// Patterns to exclude from checking
    #[serde(default)]
    pub excludes: Vec<String>,

    /// Check only staged files
    #[serde(default)]
    pub check_staged: bool,

    /// Check working tree files
    #[serde(default)]
    pub check_working_tree: bool,

    /// Default git revision range for file discovery
    #[serde(default)]
    pub git: Option<String>,

    /// Respect .gitignore patterns
    #[serde(default = "default_true")]
    pub respect_gitignore: bool,

    /// Treat warnings as errors
    #[serde(default)]
    pub fail_on_warn: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RulesConfig {
    #[serde(flatten)]
    pub rules: HashMap<String, RuleDefinition>,
}

fn default_priority() -> i32 {
    100
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuleDefinition {
    #[serde(default = "default_true")]
    pub enabled: bool,
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
}

impl Default for Config {
    fn default() -> Self {
        toml::from_str(DEFAULT_CONFIG_TOML).expect("Embedded default config must be valid")
    }
}

impl SizelintConfig {
    fn merge_from(&mut self, other: SizelintConfig) {
        if other.max_file_size.is_some() {
            self.max_file_size = other.max_file_size;
        }
        if other.warn_file_size.is_some() {
            self.warn_file_size = other.warn_file_size;
        }
        if !other.excludes.is_empty() {
            self.excludes = other.excludes;
        }
        self.check_staged = other.check_staged;
        self.check_working_tree = other.check_working_tree;
        if other.git.is_some() {
            self.git = other.git;
        }
        self.respect_gitignore = other.respect_gitignore;
        self.fail_on_warn = other.fail_on_warn;
    }
}

impl RulesConfig {
    fn merge_from(&mut self, other: RulesConfig) {
        for (name, rule_def) in other.rules {
            self.rules.insert(name, rule_def);
        }
    }

    pub fn get_rule(&self, name: &str) -> Option<&RuleDefinition> {
        self.rules.get(name)
    }

    pub fn get_enabled_rules(&self) -> Vec<(&String, &RuleDefinition)> {
        self.rules.iter().filter(|(_, rule)| rule.enabled).collect()
    }
}

impl Config {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let _span = span!(Level::DEBUG, "Config::load_from_file", path = %path.as_ref().display())
            .entered();

        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| SizelintError::config_read(path.as_ref().to_path_buf(), e))?;

        let config: Config = toml::from_str(&content)
            .map_err(|e| SizelintError::config_parse(path.as_ref().to_path_buf(), e))?;

        debug!("Config loaded successfully from file");
        Ok(config)
    }

    pub fn load_with_defaults<P: AsRef<Path>>(path: P) -> Result<Self> {
        let _span =
            span!(Level::DEBUG, "Config::load_with_defaults", path = %path.as_ref().display())
                .entered();

        let mut config = Self::default();

        let user_config = Self::load_from_file(path)?;
        config.merge_from_user_config(user_config);

        debug!("Config loaded and merged with defaults");
        Ok(config)
    }

    fn merge_from_user_config(&mut self, user_config: Config) {
        self.sizelint.merge_from(user_config.sizelint);

        if let Some(user_rules) = user_config.rules {
            if let Some(ref mut default_rules) = self.rules {
                default_rules.merge_from(user_rules);
            } else {
                self.rules = Some(user_rules);
            }
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

    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let content = toml::to_string_pretty(self).map_err(|e| {
            SizelintError::config_invalid(
                "serialization".to_string(),
                "config".to_string(),
                format!("Failed to serialize config: {e}"),
            )
        })?;

        std::fs::write(path.as_ref(), content).map_err(|e| {
            SizelintError::filesystem(
                "write config file".to_string(),
                path.as_ref().to_path_buf(),
                e,
            )
        })?;

        Ok(())
    }

    pub fn create_default_config() -> String {
        DEFAULT_CONFIG_TOML.to_string()
    }

    pub fn default_config_str() -> &'static str {
        DEFAULT_CONFIG_TOML
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
}
