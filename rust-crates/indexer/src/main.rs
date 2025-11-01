use anyhow::Context;
use clap::{
    ArgGroup,
    Parser,
};
use fuel_core::state::rocks_db::{
    ColumnsPolicy,
    DatabaseConfig,
};
use fuel_indexer::indexer::IndexerConfig;
use fuels::types::ContractId;
use indexer::app::{
    App,
    RunState,
    actix_query_api::ActixQueryApi,
    fuel_indexer_event_source::{
        FuelIndexerEventSource,
        parse_event_logs,
    },
    init_tracing,
    sled_storage::SledSnapshotStorage,
};
use std::{
    env::current_exe,
    fs,
    path::PathBuf,
    str::FromStr,
};
use url::Url;

#[derive(Parser, Debug)]
#[command(
    version,
    about,
    long_about = None,
    group(
        ArgGroup::new("network")
            .args(["local", "dev", "test"])
            .required(true)
    )
)]
struct Args {
    #[arg(short, long)]
    contract_id: String,

    #[arg(short, long)]
    starting_block_height: u32,

    #[arg(short, long)]
    graphql_url: Url,

    #[arg(short, long)]
    port: Option<u16>,

    #[arg(short, long, default_value = "false")]
    tracing: bool,

    #[arg(long)]
    snapshot_dir: Option<PathBuf>,

    #[arg(long)]
    local: bool,

    #[arg(long)]
    dev: bool,

    #[arg(long)]
    test: bool,
}

async fn handle_interupt() {
    let res = tokio::signal::ctrl_c().await;
    match res {
        Ok(_) => {
            tracing::info!("Received interrupt, exiting");
        }
        Err(_) => {
            tracing::warn!("Received interrupt error, exiting anyway");
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    if args.tracing {
        init_tracing();
    }
    let contract_id_str = args.contract_id;
    let contract_id = ContractId::from_str(&contract_id_str).unwrap();

    let network_label = if args.local {
        "local"
    } else if args.dev {
        "dev"
    } else {
        "test"
    };
    let binary_dir = current_exe()
        .context("determine indexer binary path")?
        .parent()
        .context("indexer binary has no parent directory")?
        .to_path_buf();
    let data_root = binary_dir.join("strapped_indexer_data").join(network_label);
    fs::create_dir_all(&data_root)?;
    let event_data_path = data_root.join("events");
    fs::create_dir_all(&event_data_path)?;
    tracing::info!(
        "Using persistent event directory: {}",
        event_data_path.display()
    );
    let storage_path = match &args.snapshot_dir {
        Some(path) => path.clone(),
        None => data_root.join("snapshots"),
    };
    fs::create_dir_all(&storage_path)?;
    tracing::info!("Using sled storage directory: {}", storage_path.display());
    let database_config = DatabaseConfig {
        cache_capacity: None,
        max_fds: 512,
        columns_policy: ColumnsPolicy::Lazy,
    };
    let indexer_config =
        IndexerConfig::new(args.starting_block_height.into(), args.graphql_url);
    let events = FuelIndexerEventSource::new(
        parse_event_logs,
        event_data_path.clone(),
        database_config,
        indexer_config,
    )
    .await
    .unwrap();
    let api = ActixQueryApi::new(args.port).await.unwrap();
    let (snapshots, metadata) = SledSnapshotStorage::open(&storage_path)?;
    let mut app = App::new(events, api, snapshots, metadata, contract_id);

    tracing::info!("Starting indexer service");
    loop {
        let interrupt = handle_interupt();
        match app.run(interrupt).await? {
            RunState::Continue => continue,
            RunState::Exit => {
                tracing::info!("Exiting indexer service");
                return Ok(());
            }
        }
    }
}
