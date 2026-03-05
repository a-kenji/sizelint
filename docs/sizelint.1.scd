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
		Set output format. Options: *human* (default), *json*.

	*--staged*
		Check only staged files (equivalent to *git diff --staged --name-only*).

	*--working-tree*
		Check only working tree files (equivalent to *git diff --name-only*).

	*--git* _RANGE_
		Check files changed in a git revision range. A bare ref such as *main*
		is expanded to the merge-base with HEAD. Two-dot (*main..HEAD*) and
		three-dot (*main...feature*) ranges are passed through as-is.
		Both live files and historical blobs in the range are scanned.

	*--no-history*
		Skip git history scanning for deleted blobs (only check files at HEAD).
		Requires *--git*.

	*--fail-on-warn*
		Treat warnings as errors (exit 1 on warnings).

	*--quiet*, *-q*
		Suppress informational output, show only violations.

*init* [*--force*] [*--stdout*] (alias: *i*)
	Initialize or update sizelint configuration file. Creates *sizelint.toml* in the current directory.

	*--force*, *-f*
		Overwrite existing configuration file.

	*--stdout*
		Print default configuration to stdout instead of writing to file.

	*--edit*
		Open configuration file in editor after creation (uses *$VISUAL*, *$EDITOR*, or *vi*).

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

*--debug*
	Enable debug output. For fine-grained control, set the *SIZELINT_LOG* environment variable (uses tracing EnvFilter syntax, e.g. *SIZELINT_LOG=sizelint::config=trace*).

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

*git*
	Default git revision range for file discovery. Same semantics as *--git*.

*respect_gitignore*
	Respect .gitignore patterns when discovering files (default: true).

*fail_on_warn*
	Treat warnings as errors (default: false).

## Rules Configuration

Rules are defined as *[rules.NAME]* tables. Each rule has a unique name and the
following fields:

*enabled*
	Enable or disable the rule (default: true).

*description*
	Human-readable description of the rule.

*priority*
	Higher numbers are evaluated first. The first matching rule wins (default: 100).

*max_size*
	Maximum file size before error. Examples: "10MB", "1GB".

*warn_size*
	File size warning threshold. Examples: "5MB", "500KB".

*includes*
	List of glob patterns — only matching files are checked by this rule.

*excludes*
	List of glob patterns — matching files are skipped by this rule.

*warn_on_match*
	Emit a warning for any file matching the include patterns (default: false).

*error_on_match*
	Emit an error for any file matching the include patterns (default: false).

*suggestion*
	Actionable hint shown to users when this rule triggers.

## Size Units

Size units use binary multipliers: B, KB (1024), MB (1024²), GB (1024³), TB (1024⁴).

# EXAMPLES

Check current directory for size violations:
	*sizelint check*

Check specific files:
	*sizelint check* _file1.txt_ _file2.txt_

Check only staged files:
	*sizelint check* *--staged*

Check files changed since diverging from main:
	*sizelint check* *--git* _main_

Check a specific range without history scanning:
	*sizelint check* *--git* _main..HEAD_ *--no-history*

Scan the entire history of a branch for oversized blobs:
	*sizelint check* *--git* _"$(git hash-object -t tree /dev/null)..master"_

Initialize configuration:
	*sizelint init*

List available rules:
	*sizelint rules list*

Show rule details:
	*sizelint rules describe* _default_

Generate bash completions:
	*sizelint completions* _bash_

# CONFIGURATION EXAMPLES

Basic configuration:
```
max_file_size = "10MB"
warn_file_size = "5MB"
excludes = ["*.zip", "docs/assets/*"]
```

Per-rule overrides:
```
[rules.image_files]
enabled = true
description = "Size limits for image files"
priority = 80
includes = ["*.jpg", "*.png", "*.gif"]
max_size = "2MB"
warn_size = "1MB"

[rules.video_files]
enabled = true
description = "Size limits for video files"
priority = 80
includes = ["*.mp4", "*.avi", "*.mov"]
excludes = ["docs/demos/*"]
max_size = "50MB"
```

Pattern-based rule:
```
[rules.no_images]
enabled = true
description = "Warn about image files that might be better handled with LFS"
priority = 80
includes = ["*.png", "*.jpg", "*.jpeg", "*.gif", "*.bmp"]
warn_on_match = true
suggestion = "Consider using Git LFS: git lfs track '*.png'"
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
