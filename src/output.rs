use crate::cli::OutputFormat;
use crate::error::Result;
use crate::rules::{Severity, Violation};
use colored::*;
use serde::{Deserialize, Serialize};
use std::io::{self, Write};

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
}

impl OutputFormatter {
    pub fn new(format: OutputFormat, quiet: bool) -> Self {
        Self { format, quiet }
    }

    pub fn output_results(&self, violations: &[Violation], files_checked: usize) -> Result<()> {
        let summary = self.create_summary(violations, files_checked);

        match self.format {
            OutputFormat::Human => self.output_human(violations, &summary),
            OutputFormat::Json => self.output_json(&summary),
            OutputFormat::Summary => self.output_summary(&summary),
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
                    path: v.path.display().to_string(),
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

    fn output_human(&self, violations: &[Violation], summary: &OutputSummary) -> Result<()> {
        let mut stdout = io::stdout();

        if !self.quiet {
            writeln!(stdout, "{}", "Sizelint Results".bold().blue())?;
            writeln!(stdout, "{}", "==================".blue())?;
            writeln!(stdout)?;
        }

        if !violations.is_empty() {
            for violation in violations {
                let severity_icon = match violation.severity {
                    Severity::Error => "✗".red().bold(),
                    Severity::Warning => "⚠".yellow().bold(),
                };
                let path_str = violation.path.display().to_string();
                let rule_info = format!(
                    "[{}:{}]",
                    violation.rule_name,
                    match violation.severity {
                        Severity::Error => "error",
                        Severity::Warning => "warning",
                    }
                )
                .dimmed();

                writeln!(
                    stdout,
                    "{} {}: {} {}",
                    severity_icon,
                    path_str.bold(),
                    violation.message,
                    rule_info
                )?;
            }
            writeln!(stdout)?;
        } else {
            writeln!(stdout, "{}", "✓ No violations found".green().bold())?;
        }

        if !self.quiet {
            writeln!(stdout, "{}", "Summary".bold())?;
            writeln!(stdout, "-------")?;
            writeln!(stdout, "Files checked: {}", summary.total_files_checked)?;
            writeln!(stdout, "Total violations: {}", summary.total_violations)?;

            if summary.error_count > 0 {
                writeln!(
                    stdout,
                    "Errors: {}",
                    summary.error_count.to_string().red().bold()
                )?;
            }
            if summary.warning_count > 0 {
                writeln!(
                    stdout,
                    "Warnings: {}",
                    summary.warning_count.to_string().yellow().bold()
                )?;
            }

            if summary.error_count > 0 {
                writeln!(stdout, "Status: {}", "✗ FAILED".red().bold())?;
            } else if summary.warning_count > 0 {
                writeln!(stdout, "Status: {}", "⚠ WARNINGS".yellow().bold())?;
            } else {
                writeln!(stdout, "Status: {}", "✓ PASSED".green().bold())?;
            }
        }

        Ok(())
    }

    fn output_json(&self, summary: &OutputSummary) -> Result<()> {
        let json = serde_json::to_string_pretty(summary)?;

        println!("{json}");
        Ok(())
    }

    fn output_summary(&self, summary: &OutputSummary) -> Result<()> {
        let mut stdout = io::stdout();

        writeln!(stdout, "Files: {}", summary.total_files_checked)?;
        writeln!(stdout, "Violations: {}", summary.total_violations)?;
        writeln!(
            stdout,
            "Errors: {}",
            summary.error_count.to_string().red().bold()
        )?;
        writeln!(
            stdout,
            "Warnings: {}",
            summary.warning_count.to_string().yellow().bold()
        )?;

        if summary.error_count > 0 {
            writeln!(stdout, "Status: {}", "✗ FAILED".red().bold())?;
        } else if summary.warning_count > 0 {
            writeln!(stdout, "Status: {}", "⚠ WARNINGS".yellow().bold())?;
        } else {
            writeln!(stdout, "Status: {}", "✓ PASSED".green().bold())?;
        }

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
