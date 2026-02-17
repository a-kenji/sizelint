use tracing::Level;
use tracing_subscriber::{FmtSubscriber, filter::EnvFilter};

const LOG_ENV: &str = "SIZELINT_LOG";

pub fn init(debug: bool, quiet: bool) -> Result<(), Box<dyn std::error::Error>> {
    let default_level = if debug {
        Level::DEBUG
    } else if quiet {
        Level::WARN
    } else {
        Level::INFO
    };

    let subscriber = FmtSubscriber::builder()
        .with_max_level(default_level)
        .with_thread_ids(false)
        .with_ansi(true)
        .with_line_number(false)
        .with_target(false);

    if let Ok(env_filter) = EnvFilter::try_from_env(LOG_ENV) {
        let subscriber = subscriber.with_env_filter(env_filter).finish();
        tracing::subscriber::set_global_default(subscriber)?;
    } else {
        let filter = EnvFilter::new(format!("sizelint={}", default_level));
        let subscriber = subscriber.with_env_filter(filter).finish();
        tracing::subscriber::set_global_default(subscriber)?;
    }

    Ok(())
}
