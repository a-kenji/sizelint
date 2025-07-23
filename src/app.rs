use crate::cli::{Cli, Commands, RuleAction};
use crate::config::Config;
use crate::discovery::FileDiscovery;
use crate::error::{Result, SizelintError};
use crate::output::{OutputFormatter, print_error, print_progress, print_success};
use crate::rules::{ConfigurableRule, RuleEngine};
use colored::*;
use std::path::PathBuf;
use std::process;
use tracing::{Level, debug, span};

pub struct App {
    cli: Cli,
    config: Config,
}

impl App {
    pub fn new(cli: Cli) -> Result<Self> {
        let _span = span!(Level::DEBUG, "App::new").entered();
        debug!("Creating new App instance");

        let config = Self::load_config(&cli)?;

        debug!("App initialized successfully");
        Ok(Self { cli, config })
    }

    fn load_config(cli: &Cli) -> Result<Config> {
        let _span = span!(Level::DEBUG, "load_config").entered();

        // Priority order: 1) subcommand config, 2) global config, 3) auto-discover, 4) default
        let config = if let Some(config_path) = cli.get_check_config() {
            debug!(
                "Loading config from subcommand-specified path: {}",
                config_path.display()
            );
            Config::load_from_file(&config_path)?
        } else if let Some(config_path) = &cli.config {
            debug!(
                "Loading config from global config path: {}",
                config_path.display()
            );
            Config::load_from_file(config_path)?
        } else {
            let current_dir = std::env::current_dir()
                .map_err(|e| SizelintError::CurrentDirectory { source: e })?;

            debug!(
                "Searching for config file starting from: {}",
                current_dir.display()
            );

            if let Some(config_path) = Config::find_config_file(&current_dir)? {
                debug!("Found config file: {}", config_path.display());
                print_progress(&format!("Found config file: {}", config_path.display()));
                Config::load_with_defaults(config_path)?
            } else {
                debug!("No config file found, using defaults");
                print_progress("No config file found, using defaults");
                Config::default()
            }
        };

        debug!("Config loaded successfully");
        Ok(config)
    }

    pub async fn run(&self) -> Result<()> {
        match self.cli.get_command() {
            Commands::Check { paths, .. } => self.run_check(paths).await,
            Commands::Init {
                force,
                stdout,
                edit,
            } => self.run_init(force, stdout, edit).await,
            Commands::Rules { action } => self.run_rules(action).await,
            Commands::Completions { shell } => Cli::generate_completion(&shell).map_err(|e| {
                SizelintError::config_invalid("shell".to_string(), shell.to_string(), e)
            }),
        }
    }

    async fn run_check(&self, paths: Vec<PathBuf>) -> Result<()> {
        let check_paths = self.determine_check_paths(paths);
        let discovery = self.setup_file_discovery(&check_paths)?;
        let files = self.discover_files(&discovery, &check_paths)?;
        self.validate_and_check_files(files).await
    }

    fn determine_check_paths(&self, paths: Vec<PathBuf>) -> Vec<PathBuf> {
        if paths.is_empty() {
            self.cli.get_paths()
        } else {
            paths
        }
    }

    fn setup_file_discovery(&self, check_paths: &[PathBuf]) -> Result<FileDiscovery> {
        debug!("Initializing file discovery...");
        FileDiscovery::new(
            check_paths.first().unwrap_or(&PathBuf::from(".")),
            &self.config.sizelint.excludes,
        )
    }

    fn discover_files(
        &self,
        discovery: &FileDiscovery,
        check_paths: &[PathBuf],
    ) -> Result<Vec<PathBuf>> {
        debug!("Discovering files...");

        if self.cli.get_staged()
            || (self.config.sizelint.check_staged && discovery.is_in_git_repo())
        {
            discovery.discover_staged_files()
        } else if self.cli.get_working_tree()
            || (self.config.sizelint.check_working_tree && discovery.is_in_git_repo())
        {
            discovery.discover_working_tree_files()
        } else if check_paths.len() == 1 && check_paths[0] == PathBuf::from(".") {
            discovery.discover_files(self.config.sizelint.respect_gitignore)
        } else {
            discovery.discover_specific_paths(check_paths)
        }
    }

    async fn validate_and_check_files(&self, files: Vec<PathBuf>) -> Result<()> {
        if files.is_empty() {
            print_success("No files to check");
            return Ok(());
        }

        print_progress(&format!("Found {} files to check", files.len()));

        debug!("Setting up rules...");
        let rule_engine = self.create_rule_engine()?;

        debug!("Running checks...");
        let violations = rule_engine.check_files(&files)?;

        self.output_results_and_exit(&violations, files.len())
    }

    fn output_results_and_exit(
        &self,
        violations: &[crate::rules::Violation],
        file_count: usize,
    ) -> Result<()> {
        let formatter = OutputFormatter::new(self.cli.get_format(), self.cli.get_quiet());
        formatter.output_results(violations, file_count)?;

        if !violations.is_empty() {
            let has_errors = violations
                .iter()
                .any(|v| matches!(v.severity, crate::rules::Severity::Error));

            let fail_on_warn = self.cli.get_fail_on_warn() || self.config.sizelint.fail_on_warn;
            let has_warnings = violations
                .iter()
                .any(|v| matches!(v.severity, crate::rules::Severity::Warning));

            if has_errors || (fail_on_warn && has_warnings) {
                process::exit(1);
            }
        }

        Ok(())
    }

    async fn run_init(&self, force: bool, stdout: bool, edit: bool) -> Result<()> {
        let default_config = Config::create_default_config();

        if stdout {
            println!("{default_config}");
            return Ok(());
        }

        let config_file = PathBuf::from("sizelint.toml");

        if config_file.exists() && !force {
            if edit {
                print_progress(&format!("Opening existing {}", config_file.display()));
                return self.open_editor(&config_file);
            } else {
                print_error(
                    "sizelint.toml already exists. Use --force to overwrite or --edit to open existing file.",
                );
                process::exit(1);
            }
        }

        std::fs::write(&config_file, default_config).map_err(|e| {
            SizelintError::filesystem("write config file".to_string(), config_file.clone(), e)
        })?;

        print_success(&format!("Created {}", config_file.display()));

        if edit {
            self.open_editor(&config_file)?;
        } else {
            println!(
                "You can now customize the configuration and run 'sizelint check' to start linting."
            );
        }

        Ok(())
    }

    fn open_editor(&self, file_path: &PathBuf) -> Result<()> {
        use std::process::Command;

        let editor = std::env::var("VISUAL")
            .or_else(|_| std::env::var("EDITOR"))
            .unwrap_or_else(|_| "vi".to_string());

        print_progress(&format!("Opening {} in {}...", file_path.display(), editor));

        let status = Command::new(&editor).arg(file_path).status().map_err(|e| {
            SizelintError::config_invalid(
                "editor".to_string(),
                editor.clone(),
                format!("Failed to start editor: {e}"),
            )
        })?;

        if !status.success() {
            return Err(SizelintError::config_invalid(
                "editor".to_string(),
                editor,
                "Editor exited with error".to_string(),
            ));
        }

        print_success("Configuration saved");
        Ok(())
    }

    async fn run_rules(&self, action: RuleAction) -> Result<()> {
        match action {
            RuleAction::List => {
                let rule_engine = self.create_rule_engine()?;
                let rule_info = rule_engine.get_all_rule_info(&self.config);

                if rule_info.is_empty() {
                    println!("No rules configured or available.");
                    return Ok(());
                }

                println!("{}", "Configured Rules:".bold().blue());
                println!();

                for info in &rule_info {
                    let status = if info.enabled {
                        "✓ enabled".green()
                    } else {
                        "✗ disabled".red()
                    };

                    println!("  {} - {} [{}]", info.name.bold(), info.description, status);
                }

                println!();
                let enabled_count = rule_info.iter().filter(|r| r.enabled).count();
                let disabled_count = rule_info.iter().filter(|r| !r.enabled).count();
                println!(
                    "{}",
                    format!("Runtime: {enabled_count} active, {disabled_count} inactive rules")
                        .bold()
                );

                if enabled_count > 0 {
                    println!("\n{}", "Active Rules:".bold().green());
                }
                for info in rule_info.iter().filter(|r| r.enabled) {
                    let mut details = Vec::new();

                    if let Some(priority) = info.priority {
                        details.push(format!("priority={priority}"));
                    } else {
                        details.push("priority=default".to_string());
                    }
                    if let Some(max_str) = &info.max_size_str {
                        details.push(format!("max={max_str}"));
                    }
                    if let Some(warn_str) = &info.warn_size_str {
                        details.push(format!("warn={warn_str}"));
                    }
                    if !info.includes.is_empty() {
                        details.push(format!("includes={}", info.includes.len()));
                    } else {
                        details.push("includes=[]".to_string());
                    }
                    if !info.excludes.is_empty() {
                        details.push(format!("excludes={}", info.excludes.len()));
                    } else {
                        details.push("excludes=[]".to_string());
                    }
                    if info.warn_on_match {
                        details.push("warn_on_match=true".to_string());
                    }
                    if info.error_on_match {
                        details.push("error_on_match=true".to_string());
                    }

                    println!("  ✓ {}: {}", info.name, details.join(", "));
                }

                if disabled_count > 0 {
                    println!("\n{}", "Inactive Rules:".bold().red());
                    for info in rule_info.iter().filter(|r| !r.enabled) {
                        let mut details = Vec::new();

                        if let Some(priority) = info.priority {
                            details.push(format!("priority={priority}"));
                        } else {
                            details.push("priority=default".to_string());
                        }
                        if let Some(max_str) = &info.max_size_str {
                            details.push(format!("max={max_str}"));
                        }
                        if let Some(warn_str) = &info.warn_size_str {
                            details.push(format!("warn={warn_str}"));
                        }
                        if !info.includes.is_empty() {
                            details.push(format!("includes={}", info.includes.len()));
                        } else {
                            details.push("includes=[]".to_string());
                        }
                        if !info.excludes.is_empty() {
                            details.push(format!("excludes={}", info.excludes.len()));
                        } else {
                            details.push("excludes=[]".to_string());
                        }
                        if info.warn_on_match {
                            details.push("warn_on_match=true".to_string());
                        }
                        if info.error_on_match {
                            details.push("error_on_match=true".to_string());
                        }

                        println!("  ✗ {}: {}", info.name, details.join(", "));
                    }
                }
            }
            RuleAction::Describe { rule } => {
                let rule_engine = self.create_rule_engine()?;
                let rule_info = rule_engine.get_all_rule_info(&self.config);

                if let Some(info) = rule_info.iter().find(|r| r.name == rule) {
                    println!("{}", format!("Rule: {}", info.name).bold().blue());
                    println!("{}", "━".repeat(50).blue());
                    println!();
                    println!("Description: {}", info.description);
                    println!(
                        "Status: {}",
                        if info.enabled {
                            "✓ enabled".green()
                        } else {
                            "✗ disabled".red()
                        }
                    );

                    let mut severities = Vec::new();
                    if info.max_size.is_some() {
                        severities.push("Error".red().to_string());
                    }
                    if info.warn_size.is_some() {
                        severities.push("Warning".yellow().to_string());
                    }
                    if !severities.is_empty() {
                        println!("Can generate: {}", severities.join(", "));
                    }
                    println!();

                    println!("{}", "Configuration:".bold());
                    if let Some(priority) = info.priority {
                        println!("  Priority: {priority}");
                    } else {
                        println!("  Priority: default (lowest)");
                    }
                    if let Some(max_str) = &info.max_size_str {
                        let bytes_info = if let Some(bytes) = info.max_size {
                            format!(" ({bytes} bytes)")
                        } else {
                            String::new()
                        };
                        println!("  Max size: {max_str}{bytes_info}");
                    }
                    if let Some(warn_str) = &info.warn_size_str {
                        let bytes_info = if let Some(bytes) = info.warn_size {
                            format!(" ({bytes} bytes)")
                        } else {
                            String::new()
                        };
                        println!("  Warning size: {warn_str}{bytes_info}");
                    }
                    if !info.includes.is_empty() {
                        println!("  Includes: {:?}", info.includes);
                    } else {
                        println!("  Includes: all files");
                    }
                    if !info.excludes.is_empty() {
                        println!("  Excludes: {:?}", info.excludes);
                    } else {
                        println!("  Excludes: none");
                    }
                    if info.warn_on_match {
                        println!("  Warn on match: enabled");
                    }
                    if info.error_on_match {
                        println!("  Error on match: enabled");
                    }
                } else {
                    print_error(&format!("Unknown rule: {rule}"));
                    process::exit(1);
                }
            }
        }

        Ok(())
    }

    fn create_rule_engine(&self) -> Result<RuleEngine> {
        let mut engine = RuleEngine::new();

        // Always add a default rule that catches all files not matched by specific rules
        self.add_default_rule(&mut engine)?;

        // Add any specific rules from configuration
        if let Some(rules_config) = &self.config.rules {
            let enabled_rules = rules_config.get_enabled_rules();
            for (rule_name, rule_def) in enabled_rules {
                let mut rule_definition = rule_def.clone();

                if rule_definition.max_size.is_none() {
                    rule_definition.max_size = self.config.sizelint.max_file_size.clone();
                }
                if rule_definition.warn_size.is_none() {
                    rule_definition.warn_size = self.config.sizelint.warn_file_size.clone();
                }

                let rule = ConfigurableRule::new(rule_name.clone(), rule_definition)?;
                engine.add_rule(rule);
            }
        }

        Ok(engine)
    }

    fn add_default_rule(&self, engine: &mut RuleEngine) -> Result<()> {
        use crate::config::RuleDefinition;

        let default_rule = RuleDefinition {
            enabled: true,
            description: "Default file size check".to_string(),
            priority: 1000,
            max_size: self.config.sizelint.max_file_size.clone(),
            warn_size: self.config.sizelint.warn_file_size.clone(),
            includes: vec![],
            excludes: vec![],
            ..Default::default()
        };

        let rule = ConfigurableRule::new("default".to_string(), default_rule)?;
        engine.add_rule(rule);
        Ok(())
    }
}
