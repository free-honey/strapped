use color_eyre::eyre::Result;
use std::sync::OnceLock;
use tracing_appender::rolling;
use tracing_subscriber::{
    EnvFilter,
    fmt,
};

mod client;
mod ui;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    let use_fake_vrf = std::env::args().any(|arg| arg == "--fake-vrf");
    let vrf_mode = if use_fake_vrf {
        client::VrfMode::Fake
    } else {
        client::VrfMode::Pseudo
    };
    client::run_app(vrf_mode).await
}
