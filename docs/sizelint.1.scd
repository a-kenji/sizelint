sizelint(1)

# NAME

sizelint - Git-aware file size linter

# SYNOPSIS

*sizelint* [_OPTIONS_] _COMMAND_ [_ARGS_]

# DESCRIPTION

*sizelint* is a Git-aware file size linter designed to prevent large files from entering repositories. It checks files against configurable size limits and integrates with Git workflows to examine staged changes, working tree modifications, or entire repositories.

The tool is compatible with treefmt and supports parallel processing for performance on large codebases.

# COMMANDS

*check* [_PATHS_] (alias: *c*)
	Check files for size violations. If no paths are specified, checks the current directory.

	*--format* _FORMAT_, *-f* _FORMAT_
		Set output format. Options: *human* (default), *json*, *summary*.

	*--staged*
		Check only staged files (equivalent to *git diff --staged --name-only*).

	*--working-tree*
		Check only working tree files (equivalent to *git diff --name-only*).

	*--quiet*, *-q*
		Suppress informational output, show only violations.

*init* [*--force*] [*--stdout*] (alias: *i*)
	Initialize or update sizelint configuration file. Creates *sizelint.toml* in the current directory.

	*--force*, *-f*
		Overwrite existing configuration file.

	*--stdout*
		Print default configuration to stdout instead of writing to file.

*rules* _ACTION_ (alias: *r*)
	Rule management commands.

	*list* (alias: *l*)
		List available rules.

	*describe* _RULE_ (alias: *d*)
		Show detailed information about a rule.

*completions* _SHELL_
	Generate shell completion scripts. Supported shells: *bash*, *zsh*, *fish*, *powershell*, *elvish*.

# OPTIONS

*--config* _FILE_, *-c* _FILE_
	Specify configuration file path. Default searches for *sizelint.toml* or *.sizelint.toml* in current directory and parent directories.

*--verbose*, *-v*
	Enable verbose output with additional debugging information.

*--log-level* _LEVEL_
	Set logging level. Options: *trace*, *debug*, *info*, *warn*, *error*. Default: *info*.

# CONFIGURATION

*sizelint* uses TOML configuration files. The tool searches for *sizelint.toml* or *.sizelint.toml* starting from the current directory and moving up the directory tree.

## Main Configuration

*max_file_size*
	Maximum file size before error. Examples: "10MB", "1GB".

*warn_file_size*
	File size warning threshold. Examples: "5MB", "500KB".

*excludes*
	List of glob patterns to exclude from checking.

*check_staged*
	Check only staged files by default (default: false).

*check_working_tree*
	Check only working tree files by default (default: false).

*respect_gitignore*
	Respect .gitignore patterns when discovering files (default: true).

## Rules Configuration

Rules are defined in the *[rules]* section. Each rule has a unique name and configuration.

*[rules.rule_name]*
	Configuration for a specific rule.

*type*
	Rule type (e.g., "large_files").

*enabled*
	Enable or disable the rule (default: true).

*description*
	Human-readable description of the rule.

*includes*
	List of glob patterns to include for this rule.

*excludes*
	List of glob patterns to exclude for this rule.

*max_size*
	Maximum file size before error (for size-based rules).

*warn_size*
	File size warning threshold (for size-based rules).


# RULES

## large_files

Checks files against configured size limits. Supports both error and warning thresholds with configurable exclude patterns.

Size units use binary multipliers: B, KB (1024), MB (1024²), GB (1024³), TB (1024⁴).

# EXAMPLES

Check current directory for size violations:
	*sizelint check*

Check specific files:
	*sizelint check* _file1.txt_ _file2.txt_

Check only staged files:
	*sizelint check* *--staged*

Initialize configuration:
	*sizelint init*

List available rules:
	*sizelint rules list*

Show rule details:
	*sizelint rules describe* _large_files_

Generate bash completions:
	*sizelint completions* _bash_

# CONFIGURATION EXAMPLES

Basic configuration:
```
[sizelint]
max_file_size = "10MB"
warn_file_size = "5MB"
excludes = ["*.zip", "docs/assets/*"]

[rules.large_files]
type = "large_files"
enabled = true
description = "Checks for files that exceed configured size limits"
max_size = "10MB"
warn_size = "5MB"
```

Advanced rule configuration:
```
[rules.image_files]
type = "large_files"
enabled = true
description = "Size limits for image files"
includes = ["*.jpg", "*.png", "*.gif"]
max_size = "2MB"
warn_size = "1MB"

[rules.video_files]
type = "large_files"
enabled = true
description = "Size limits for video files"
includes = ["*.mp4", "*.avi", "*.mov"]
excludes = ["docs/demos/*"]
max_size = "50MB"
```

# EXIT STATUS

*0*
	No violations found.

*1*
	Size violations found or configuration error.

# FILES

*sizelint.toml*, *.sizelint.toml*
	Configuration files searched in current directory and parent directories.

*.gitignore*
	Git ignore patterns are respected when *respect_gitignore* is enabled.

# SEE ALSO

*git*(1), *treefmt*(1)

# AUTHORS

sizelint contributors

# REPORTING BUGS

Report bugs at: https://github.com/a-kenji/sizelint/issues
