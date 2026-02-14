use crate::cli::OutputFormat;
use crate::error::Result;
use crate::rules::{Severity, Violation};
use colored::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Serialize, Deserialize)]
pub struct OutputSummary {
    pub total_files_checked: usize,
    pub total_violations: usize,
    pub error_count: usize,
    pub warning_count: usize,
    pub rules_run: Vec<String>,
    pub violations: Vec<ViolationOutput>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ViolationOutput {
    pub path: String,
    pub rule_name: String,
    pub message: String,
    pub severity: String,
    pub actual_value: Option<String>,
    pub expected_value: Option<String>,
}

pub struct OutputFormatter {
    format: OutputFormat,
    quiet: bool,
    base_path: PathBuf,
}

impl OutputFormatter {
    pub fn new(format: OutputFormat, quiet: bool, base_path: PathBuf) -> Self {
        Self {
            format,
            quiet,
            base_path,
        }
    }

    fn relative_path(&self, path: &Path) -> String {
        path.strip_prefix(&self.base_path)
            .unwrap_or(path)
            .display()
            .to_string()
    }

    pub fn output_results(
        &self,
        violations: &[Violation],
        files_checked: usize,
        elapsed: Duration,
    ) -> Result<()> {
        let summary = self.create_summary(violations, files_checked);

        match self.format {
            OutputFormat::Human => self.output_human(violations, &summary, elapsed),
            OutputFormat::Json => self.output_json(&summary),
        }
    }

    fn create_summary(&self, violations: &[Violation], files_checked: usize) -> OutputSummary {
        let mut rules_run = std::collections::HashSet::new();
        let mut error_count = 0;
        let mut warning_count = 0;

        let violation_outputs: Vec<ViolationOutput> = violations
            .iter()
            .map(|v| {
                rules_run.insert(v.rule_name.clone());

                match v.severity {
                    Severity::Error => error_count += 1,
                    Severity::Warning => warning_count += 1,
                }

                ViolationOutput {
                    path: self.relative_path(&v.path),
                    rule_name: v.rule_name.clone(),
                    message: v.message.clone(),
                    severity: match v.severity {
                        Severity::Error => "error".to_string(),
                        Severity::Warning => "warning".to_string(),
                    },
                    actual_value: v.actual_value.clone(),
                    expected_value: v.expected_value.clone(),
                }
            })
            .collect();

        OutputSummary {
            total_files_checked: files_checked,
            total_violations: violations.len(),
            error_count,
            warning_count,
            rules_run: rules_run.into_iter().collect(),
            violations: violation_outputs,
        }
    }

    fn output_human(
        &self,
        violations: &[Violation],
        summary: &OutputSummary,
        elapsed: Duration,
    ) -> Result<()> {
        let mut stdout = io::stdout();
        let gutter = "┃".dimmed();

        let mut by_rule: BTreeMap<&str, Vec<&Violation>> = BTreeMap::new();
        for v in violations {
            by_rule.entry(&v.rule_name).or_default().push(v);
        }

        for (rule_name, rule_violations) in &by_rule {
            writeln!(stdout, "{}", rule_name.bold())?;
            writeln!(stdout, "{gutter}")?;

            let mut errors: Vec<&Violation> = Vec::new();
            let mut warnings: Vec<&Violation> = Vec::new();
            for v in rule_violations {
                match v.severity {
                    Severity::Error => errors.push(v),
                    Severity::Warning => warnings.push(v),
                }
            }
            errors.sort_by(|a, b| b.sort_key.cmp(&a.sort_key));
            warnings.sort_by(|a, b| b.sort_key.cmp(&a.sort_key));

            for (severity_group, marker, color_fn) in [
                (
                    &errors,
                    "[E]",
                    ColoredString::red as fn(ColoredString) -> ColoredString,
                ),
                (&warnings, "[W]", ColoredString::yellow),
            ] {
                if severity_group.is_empty() {
                    continue;
                }
                let message = &severity_group[0].message;
                writeln!(stdout, "{gutter} {} {}", color_fn(marker.bold()), message)?;
                for v in severity_group {
                    let path_str = self.relative_path(&v.path);
                    match &v.actual_value {
                        Some(actual) => {
                            writeln!(stdout, "{gutter}     {} ({})", path_str.bold(), actual)?;
                        }
                        None => {
                            writeln!(stdout, "{gutter}     {}", path_str.bold())?;
                        }
                    }
                }
            }
            writeln!(stdout)?;
        }

        if !self.quiet {
            writeln!(stdout)?;
            writeln!(
                stdout,
                "{}",
                format!("Analysis took {:.2}s", elapsed.as_secs_f64()).dimmed()
            )?;
            let mut parts = vec![format!("Checked {} files", summary.total_files_checked)];
            if summary.error_count > 0 {
                parts.push(format!(
                    "{} {}",
                    summary.error_count.to_string().red().bold(),
                    if summary.error_count == 1 {
                        "error"
                    } else {
                        "errors"
                    }
                ));
            }
            if summary.warning_count > 0 {
                parts.push(format!(
                    "{} {}",
                    summary.warning_count.to_string().yellow().bold(),
                    if summary.warning_count == 1 {
                        "warning"
                    } else {
                        "warnings"
                    }
                ));
            }

            let status = if summary.error_count > 0 {
                "FAILED".red().bold()
            } else if summary.warning_count > 0 {
                "WARNINGS".yellow().bold()
            } else {
                "PASSED".green().bold()
            };

            writeln!(stdout, "{}. [{}]", parts.join(", "), status)?;
        }

        Ok(())
    }

    fn output_json(&self, summary: &OutputSummary) -> Result<()> {
        let json = serde_json::to_string_pretty(summary)?;

        println!("{json}");
        Ok(())
    }
}

pub fn print_progress(message: &str) {
    if !cfg!(test) {
        eprintln!("{} {}", "→".dimmed(), message.dimmed());
    }
}

pub fn print_error(message: &str) {
    eprintln!("{} {}", "✗".red().bold(), message.red());
}

pub fn print_success(message: &str) {
    eprintln!("{} {}", "✓".green().bold(), message.green());
}

pub fn print_warning(message: &str) {
    eprintln!("{} {}", "⚠".yellow().bold(), message.yellow());
}
