# `$ sizelint` - lint your working tree based on file sizes

[![Built with Nix](https://img.shields.io/static/v1?label=built%20with&message=nix&color=5277C3&logo=nixos&style=flat-square&logoColor=ffffff)](https://builtwithnix.org)
[![Crates](https://img.shields.io/crates/v/sizelint?style=flat-square)](https://crates.io/crates/sizelint)

<!--toc:start-->
- [`$ sizelint` - lint your working tree based on file sizes](#sizelint-lint-your-working-tree-based-on-file-sizes)
  - [`$ sizelint` - usage](#-sizelint---usage)
    - [`$ sizelint check`](#-sizelint-check)
    - [`$ sizelint rules`](#-sizelint-rules)
    - [`$ sizelint init`](#-sizelint-init)
  - [Configuration](#configuration)
  - [Documentation](#documentation)
  - [Development](#development)
  - [License](#license)
<!--toc:end-->

## Overview

`sizelint` is a fast, configurable file size linter that helps prevent large files from entering your Git repository.
It can be used as a standalone tool, pre-commit hook, or as part of your CI/CD pipeline.

## `$ sizelint` - usage

<!-- `$ nix run . help` -->

```
A git-aware file size linter

Usage: sizelint [OPTIONS] <COMMAND>

Commands:
  check        Check files for size violations
  init         Initialize sizelint configuration
  rules        Rule management
  completions  Generate shell completions
  help         Print this message or the help of the given subcommand(s)

Options:
  -c, --config <FILE>
          Configuration file path

  -v, --verbose
          Verbose output

      --log-level <LOG_LEVEL>
          Log level
          
          [default: info]

          Possible values:
          - trace: Trace level logging
          - debug: Debug level logging
          - info:  Info level logging
          - warn:  Warning level logging
          - error: Error level logging

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```

## `$ sizelint check`

<!-- `$ nix run . help check` -->

```
Check files for size violations

Usage: sizelint check [OPTIONS] [PATHS]...

Arguments:
  [PATHS]...
          Paths to check

Options:
  -f, --format <FORMAT>
          Output format
          
          [default: human]

          Possible values:
          - human:   Human-readable output
          - json:    JSON output
          - summary: Summary only

      --staged
          Check only staged files (git diff --staged)

      --working-tree
          Check working tree files

  -q, --quiet
          Quiet mode (only show violations)

      --fail-on-warn
          Treat warnings as errors

  -h, --help
          Print help (see a summary with '-h')
```

## `$ sizelint rules`

<!-- `$ nix run . help rules` -->

```
Rule management

Usage: sizelint rules <COMMAND>

Commands:
  list      List available rules
  describe  Show rule documentation
  help      Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

## `$ sizelint init`

<!-- `$ nix run . help init` -->

```
Initialize sizelint configuration

Usage: sizelint init [OPTIONS]

Options:
  -f, --force   Force overwrite existing configuration
      --stdout  Print the default configuration to stdout
      --edit    Open configuration file in editor after creation
  -h, --help    Print help
```


## Quick Start

### Installation

```bash
cargo install sizelint --locked
```

### Basic Usage

```bash
# Initialize configuration
sizelint init

# Check all files
sizelint check

# Check specific files
sizelint check src/main.rs README.md
```

## Configuration

`sizelint` uses TOML configuration files.

Run `sizelint init` to create a default configuration:
<!-- `$ nix run . -- init --stdout` -->

```
[sizelint]
max_file_size = "10MB"
warn_file_size = "5MB"
excludes = []
check_staged = false
check_working_tree = false
respect_gitignore = true
fail_on_warn = false

[rules.medium_files]
enabled = true
description = "Base rule that fits many normal repos"
priority = 50
max_size = "5MB"
warn_size = "2MB"
includes = []
excludes = []

[rules.no_images]
enabled = false
description = "Warn about image files that might be better handled with LFS"
priority = 80
includes = ["*.png", "*.jpg", "*.jpeg", "*.gif", "*.bmp"]
excludes = []
warn_on_match = true

```

## Documentation

- [Manual Page](docs/sizelint.1.scd) - Reference in scdoc format

## Development

### Development Shell

```bash
nix develop
```

### Building

```bash
cargo build --release
```

### Testing

```bash
cargo test
cargo clippy
```

## License

MIT License - see [LICENSE](LICENSE) file for details.
