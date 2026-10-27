use clap::Parser;
use sizelint::cli::Commands;
use sizelint::{App, Cli};
use std::process::ExitCode;

fn main() -> ExitCode {
    let cli = Cli::parse();

    let quiet = matches!(&cli.command, Commands::Check(args) if args.quiet);
    if let Err(e) = sizelint::log::init(cli.debug, quiet) {
        eprintln!("Failed to initialize logging: {e}");
        return ExitCode::FAILURE;
    }

    tracing::debug!("Starting sizelint");

    match App::run(cli) {
        Ok(code) => {
            tracing::debug!("Sizelint completed");
            code
        }
        Err(e) => {
            eprintln!("{:?}", miette::Report::new(e));
            ExitCode::FAILURE
        }
    }
}
