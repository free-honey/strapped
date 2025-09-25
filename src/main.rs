use color_eyre::eyre::Result;
use std::sync::OnceLock;
use tracing_appender::rolling;
use tracing_subscriber::{
    EnvFilter,
    fmt,
};

mod client;
mod ui;

static TRACING_GUARD: OnceLock<tracing_appender::non_blocking::WorkerGuard> =
    OnceLock::new();

fn init_tracing() -> Result<()> {
    let log_dir = std::env::var("STRAPPED_LOG_DIR").unwrap_or_else(|_| String::from("."));
    let file_appender = rolling::never(log_dir, "trace.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    // Leak guard for program lifetime to ensure logs flush on exit
    let _ = TRACING_GUARD.set(guard);
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("error"));
    fmt()
        .with_env_filter(env_filter)
        .with_writer(non_blocking)
        .with_ansi(false)
        .init();
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    // init_tracing()?;
    client::run_app().await
}
