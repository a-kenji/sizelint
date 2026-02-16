use clap::Parser;
use sizelint::{App, Cli};
use std::process::ExitCode;

fn main() -> ExitCode {
    let cli = Cli::parse();

    if let Err(e) = sizelint::log::init(Some(cli.log_level.as_str()), cli.verbose, cli.get_quiet())
    {
        eprintln!("Failed to initialize logging: {e}");
        return ExitCode::FAILURE;
    }

    tracing::debug!("Starting sizelint");

    let app = match App::new(cli) {
        Ok(app) => app,
        Err(e) => {
            eprintln!("{:?}", miette::Report::new(e));
            return ExitCode::FAILURE;
        }
    };

    match app.run() {
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
