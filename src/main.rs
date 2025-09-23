use color_eyre::eyre::Result;

mod client;
mod ui;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    client::run_app().await
}
