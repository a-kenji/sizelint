use clap::Parser;
use miette::Result;
use sizelint::{App, Cli};
use std::process;

fn main() -> Result<()> {
    let cli = Cli::parse();

    if let Err(e) = sizelint::log::init(Some(cli.log_level.as_str()), cli.verbose, cli.get_quiet())
    {
        eprintln!("Failed to initialize logging: {e}");
        process::exit(1);
    }

    tracing::debug!("Starting sizelint");

    let app = App::new(cli)?;
    app.run()?;

    tracing::debug!("Sizelint completed successfully");
    Ok(())
}
