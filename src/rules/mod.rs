use crate::config::RuleDefinition;
use crate::error::{Result, SizelintError};
use miette::Diagnostic;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;
use tracing::{Level, debug, span};

// Size constants using binary multipliers
const BYTES_PER_KB: u64 = 1_024;
const BYTES_PER_MB: u64 = BYTES_PER_KB * 1_024;
const BYTES_PER_GB: u64 = BYTES_PER_MB * 1_024;
const BYTES_PER_TB: u64 = BYTES_PER_GB * 1_024;

// Size formatting constants
const SIZE_THRESHOLD: f64 = 1024.0;
const SIZE_UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];

#[derive(Debug, Clone)]
pub struct RuleInfo {
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub priority: Option<i32>,
    pub max_size: Option<u64>,
    pub warn_size: Option<u64>,
    pub max_size_str: Option<String>,
    pub warn_size_str: Option<String>,
    pub includes: Vec<String>,
    pub excludes: Vec<String>,
    pub warn_on_match: bool,
    pub error_on_match: bool,
}

#[derive(Debug, Clone, Error)]
#[error("{message}")]
pub struct Violation {
    pub path: std::path::PathBuf,
    pub rule_name: String,
    pub message: String,
    pub severity: Severity,
    pub actual_value: Option<String>,
    pub expected_value: Option<String>,
}

impl Violation {
    pub fn new(
        path: std::path::PathBuf,
        rule_name: String,
        message: String,
        severity: Severity,
    ) -> Self {
        Self {
            path,
            rule_name,
            message,
            severity,
            actual_value: None,
            expected_value: None,
        }
    }

    pub fn with_actual_value(mut self, actual: String) -> Self {
        self.actual_value = Some(actual);
        self
    }

    pub fn with_expected_value(mut self, expected: String) -> Self {
        self.expected_value = Some(expected);
        self
    }

    pub fn diagnostic_code(&self) -> String {
        format!(
            "sizelint::{}::{}",
            self.rule_name,
            match self.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
            }
        )
    }
}

impl Diagnostic for Violation {
    fn code<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        Some(Box::new(self.diagnostic_code()))
    }

    fn help<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        if let (Some(actual), Some(expected)) = (&self.actual_value, &self.expected_value) {
            Some(Box::new(format!("Actual: {actual}, Expected: {expected}")))
        } else {
            None
        }
    }

    fn severity(&self) -> Option<miette::Severity> {
        match self.severity {
            Severity::Error => Some(miette::Severity::Error),
            Severity::Warning => Some(miette::Severity::Warning),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Severity {
    Warning,
    Error,
}

pub trait Rule: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn check(&self, path: &Path) -> Result<Vec<Violation>>;
    fn is_enabled(&self) -> bool;
    fn as_any(&self) -> &dyn std::any::Any;
}

pub struct RuleEngine {
    rules: Vec<Box<dyn Rule>>,
}

impl RuleEngine {
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    pub fn add_rule<R: Rule + 'static>(&mut self, rule: R) {
        self.rules.push(Box::new(rule));
    }

    pub fn check_file(&self, path: &Path) -> Result<Vec<Violation>> {
        let mut violations = Vec::new();
        let mut matching_rules = Vec::new();

        // Find all rules that would apply to this file
        for rule in &self.rules {
            if rule.is_enabled() {
                if let Some(configurable_rule) = rule.as_any().downcast_ref::<ConfigurableRule>() {
                    if !configurable_rule.should_skip_file(path) {
                        matching_rules.push((rule, configurable_rule.get_priority()));
                    }
                }
            }
        }

        if !matching_rules.is_empty() {
            // Sort by priority
            matching_rules.sort_by(|a, b| match (a.1, b.1) {
                (Some(p1), Some(p2)) => p2.cmp(&p1),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            });

            let rule_violations = matching_rules[0].0.check(path)?;
            violations.extend(rule_violations);
        }

        Ok(violations)
    }

    pub fn check_files(&self, paths: &[std::path::PathBuf]) -> Result<Vec<Violation>> {
        let _span = span!(Level::DEBUG, "check_files", file_count = paths.len()).entered();

        let violations: Result<Vec<_>> =
            paths.par_iter().map(|path| self.check_file(path)).collect();

        let all_violations: Vec<Violation> = violations?.into_iter().flatten().collect();

        debug!(
            "Found {} total violations across {} files",
            all_violations.len(),
            paths.len()
        );
        Ok(all_violations)
    }

    pub fn get_rules(&self) -> &[Box<dyn Rule>] {
        &self.rules
    }

    pub fn get_enabled_rules(&self) -> Vec<&dyn Rule> {
        self.rules
            .iter()
            .filter(|rule| rule.is_enabled())
            .map(|rule| rule.as_ref())
            .collect()
    }

    pub fn get_rule_info(&self) -> Vec<RuleInfo> {
        self.rules
            .iter()
            .map(|rule| {
                // Try to downcast to ConfigurableRule to get detailed info
                if let Some(configurable_rule) = rule.as_any().downcast_ref::<ConfigurableRule>() {
                    configurable_rule.get_rule_info()
                } else {
                    // Fallback for other rule types
                    RuleInfo {
                        name: rule.name().to_string(),
                        description: rule.description().to_string(),
                        enabled: rule.is_enabled(),
                        priority: None,
                        max_size: None,
                        warn_size: None,
                        max_size_str: None,
                        warn_size_str: None,
                        includes: vec![],
                        excludes: vec![],
                        warn_on_match: false,
                        error_on_match: false,
                    }
                }
            })
            .collect()
    }

    pub fn get_all_rule_info(&self, config: &crate::config::Config) -> Vec<RuleInfo> {
        let mut all_rules = Vec::new();

        all_rules.extend(self.get_rule_info());

        if let Some(rules_config) = &config.rules {
            for (name, rule_def) in &rules_config.rules {
                if !rule_def.enabled && !all_rules.iter().any(|r| r.name == *name) {
                    let max_size = rule_def
                        .max_size
                        .as_ref()
                        .and_then(|s| crate::rules::parse_size_string(s).ok());
                    let warn_size = rule_def
                        .warn_size
                        .as_ref()
                        .and_then(|s| crate::rules::parse_size_string(s).ok());

                    all_rules.push(RuleInfo {
                        name: name.clone(),
                        description: rule_def.description.clone(),
                        enabled: rule_def.enabled,
                        priority: Some(rule_def.priority),
                        max_size,
                        warn_size,
                        max_size_str: rule_def.max_size.clone(),
                        warn_size_str: rule_def.warn_size.clone(),
                        includes: rule_def.includes.clone(),
                        excludes: rule_def.excludes.clone(),
                        warn_on_match: rule_def.warn_on_match,
                        error_on_match: rule_def.error_on_match,
                    });
                }
            }
        }

        all_rules
    }
}

impl Default for RuleEngine {
    fn default() -> Self {
        Self::new()
    }
}

// Configurable rule that can be created from TOML configuration
pub struct ConfigurableRule {
    name: String,
    definition: RuleDefinition,
    max_size: Option<u64>,
    warn_size: Option<u64>,
    includes: globset::GlobSet,
    excludes: globset::GlobSet,
}

impl ConfigurableRule {
    pub fn new(name: String, definition: RuleDefinition) -> Result<Self> {
        let max_size = definition
            .max_size
            .as_ref()
            .map(|s| parse_size_string(s))
            .transpose()?;

        let warn_size = definition
            .warn_size
            .as_ref()
            .map(|s| parse_size_string(s))
            .transpose()?;

        // Build includes globset
        let mut includes_builder = globset::GlobSetBuilder::new();
        for pattern in &definition.includes {
            let expanded_pattern = expand_if_path(pattern);
            let glob = globset::Glob::new(&expanded_pattern)
                .map_err(|e| SizelintError::config_invalid_pattern(pattern.clone(), e))?;
            includes_builder.add(glob);
        }
        let includes = includes_builder.build().map_err(|e| {
            SizelintError::config_invalid(
                "include_patterns".to_string(),
                "globset_builder".to_string(),
                format!("Failed to build include patterns: {e}"),
            )
        })?;

        // Build excludes globset
        let mut excludes_builder = globset::GlobSetBuilder::new();
        for pattern in &definition.excludes {
            let expanded_pattern = expand_if_path(pattern);
            let glob = globset::Glob::new(&expanded_pattern)
                .map_err(|e| SizelintError::config_invalid_pattern(pattern.clone(), e))?;
            excludes_builder.add(glob);
        }
        let excludes = excludes_builder.build().map_err(|e| {
            SizelintError::config_invalid(
                "exclude_patterns".to_string(),
                "globset_builder".to_string(),
                format!("Failed to build exclude patterns: {e}"),
            )
        })?;

        Ok(Self {
            name,
            definition,
            max_size,
            warn_size,
            includes,
            excludes,
        })
    }

    pub fn should_skip_file(&self, path: &Path) -> bool {
        // If includes are specified, file must match at least one include pattern
        if !self.definition.includes.is_empty() && !self.includes.is_match(path) {
            return true;
        }

        // If any exclude pattern matches, skip the file
        if self.excludes.is_match(path) {
            return true;
        }

        false
    }

    fn get_file_size(&self, path: &Path) -> Result<u64> {
        let metadata = std::fs::metadata(path).map_err(|e| {
            SizelintError::filesystem("get file metadata".to_string(), path.to_path_buf(), e)
        })?;
        Ok(metadata.len())
    }

    pub fn get_priority(&self) -> Option<i32> {
        if self.name == "default" {
            None
        } else {
            Some(self.definition.priority)
        }
    }

    pub fn get_rule_info(&self) -> RuleInfo {
        RuleInfo {
            name: self.name.clone(),
            description: self.definition.description.clone(),
            enabled: self.definition.enabled,
            priority: self.get_priority(),
            max_size: self.max_size,
            warn_size: self.warn_size,
            max_size_str: self.definition.max_size.clone(),
            warn_size_str: self.definition.warn_size.clone(),
            includes: self.definition.includes.clone(),
            excludes: self.definition.excludes.clone(),
            warn_on_match: self.definition.warn_on_match,
            error_on_match: self.definition.error_on_match,
        }
    }
}

impl Rule for ConfigurableRule {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.definition.description
    }

    fn is_enabled(&self) -> bool {
        self.definition.enabled
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn check(&self, path: &Path) -> Result<Vec<Violation>> {
        let mut violations = Vec::new();

        if self.should_skip_file(path) {
            return Ok(violations);
        }

        // Check match-based violations first
        if self.definition.error_on_match {
            violations.push(
                Violation::new(
                    path.to_path_buf(),
                    self.name.clone(),
                    format!("File {} matches rule pattern", path.display()),
                    Severity::Error,
                )
                .with_actual_value("matched".to_string())
                .with_expected_value("not matched".to_string()),
            );
            return Ok(violations);
        }

        if self.definition.warn_on_match {
            violations.push(
                Violation::new(
                    path.to_path_buf(),
                    self.name.clone(),
                    format!("File {} matches rule pattern", path.display()),
                    Severity::Warning,
                )
                .with_actual_value("matched".to_string())
                .with_expected_value("not matched".to_string()),
            );
        }

        // If we already have a match-based warning, don't add size-based violations
        if !violations.is_empty() {
            return Ok(violations);
        }

        // Check size-based violations
        let file_size = self.get_file_size(path)?;

        // Check error threshold (max_size)
        if let Some(max_size) = self.max_size {
            if file_size > max_size {
                violations.push(
                    Violation::new(
                        path.to_path_buf(),
                        self.name.clone(),
                        format!(
                            "File size {} exceeds maximum allowed size {}",
                            format_size(file_size),
                            format_size(max_size)
                        ),
                        Severity::Error,
                    )
                    .with_actual_value(format_size(file_size))
                    .with_expected_value(format!("≤ {}", format_size(max_size))),
                );
                return Ok(violations);
            }
        }

        // Check warning threshold (warn_size)
        if let Some(warn_size) = self.warn_size {
            if file_size > warn_size {
                violations.push(
                    Violation::new(
                        path.to_path_buf(),
                        self.name.clone(),
                        format!(
                            "File size {} exceeds warning threshold {}",
                            format_size(file_size),
                            format_size(warn_size)
                        ),
                        Severity::Warning,
                    )
                    .with_actual_value(format_size(file_size))
                    .with_expected_value(format!("≤ {}", format_size(warn_size))),
                );
            }
        }

        Ok(violations)
    }
}

fn expand_if_path(pattern: &str) -> String {
    // If pattern contains slash, treat as path
    // Otherwise, treat as filename pattern and prepend with **/ for recursive matching
    if pattern.contains('/') {
        pattern.to_string()
    } else {
        format!("**/{pattern}")
    }
}

pub fn parse_size_string(size_str: &str) -> Result<u64> {
    let size_str = size_str.trim().to_uppercase();

    if size_str.is_empty() {
        return Err(SizelintError::invalid_size_format(
            size_str.to_string(),
            "Empty size string".to_string(),
        ));
    }

    let (number_part, unit_part) = if size_str.ends_with("TB") {
        (&size_str[..size_str.len() - 2], "TB")
    } else if size_str.ends_with("GB") {
        (&size_str[..size_str.len() - 2], "GB")
    } else if size_str.ends_with("MB") {
        (&size_str[..size_str.len() - 2], "MB")
    } else if size_str.ends_with("KB") {
        (&size_str[..size_str.len() - 2], "KB")
    } else if size_str.ends_with("B") {
        (&size_str[..size_str.len() - 1], "B")
    } else {
        (size_str.as_str(), "B")
    };

    let number: f64 = number_part.parse().map_err(|_| {
        SizelintError::invalid_size_format(
            size_str.to_string(),
            format!("Invalid size number: {number_part}"),
        )
    })?;

    if number < 0.0 {
        return Err(SizelintError::invalid_size_format(
            size_str.to_string(),
            "Size cannot be negative".to_string(),
        ));
    }

    let multiplier = match unit_part {
        "B" => 1,
        "KB" => BYTES_PER_KB,
        "MB" => BYTES_PER_MB,
        "GB" => BYTES_PER_GB,
        "TB" => BYTES_PER_TB,
        _ => {
            return Err(SizelintError::invalid_size_format(
                size_str.to_string(),
                format!("Unknown size unit: {unit_part}"),
            ));
        }
    };

    Ok((number * multiplier as f64) as u64)
}

pub fn format_size(size: u64) -> String {
    let mut size_f = size as f64;
    let mut unit_index = 0;

    while size_f >= SIZE_THRESHOLD && unit_index < SIZE_UNITS.len() - 1 {
        size_f /= SIZE_THRESHOLD;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", size, SIZE_UNITS[unit_index])
    } else {
        format!("{:.1} {}", size_f, SIZE_UNITS[unit_index])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_size_string() {
        assert_eq!(parse_size_string("100").unwrap(), 100);
        assert_eq!(parse_size_string("100B").unwrap(), 100);
        assert_eq!(parse_size_string("1KB").unwrap(), 1024);
        assert_eq!(parse_size_string("1MB").unwrap(), 1024 * 1024);
        assert_eq!(parse_size_string("1GB").unwrap(), 1024 * 1024 * 1024);
        assert_eq!(
            parse_size_string("1.5MB").unwrap(),
            (1.5 * 1024.0 * 1024.0) as u64
        );
        assert_eq!(parse_size_string("  2MB  ").unwrap(), 2 * 1024 * 1024);
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(100), "100 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1024 * 1024), "1.0 MB");
        assert_eq!(format_size(1536 * 1024), "1.5 MB");
        assert_eq!(format_size(1024 * 1024 * 1024), "1.0 GB");
    }
}
