use clap::Parser;
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
    in_memory_metadata_storage::InMemoryMetadataStorage,
    in_memory_snapshot_storage::InMemorySnapshotStorage,
    init_tracing,
};
use std::{
    env::temp_dir,
    str::FromStr,
};
use url::Url;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
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

    let tmp_dir_path = temp_dir().as_path().to_path_buf();
    tracing::info!(
        "Using temporary directory for indexer data: {}",
        tmp_dir_path.display()
    );
    let database_config = DatabaseConfig {
        cache_capacity: None,
        max_fds: 512,
        columns_policy: ColumnsPolicy::Lazy,
    };
    let indexer_config =
        IndexerConfig::new(args.starting_block_height.into(), args.graphql_url);
    let events = FuelIndexerEventSource::new(
        parse_event_logs,
        tmp_dir_path,
        database_config,
        indexer_config,
    )
    .await
    .unwrap();
    let api = ActixQueryApi::new(args.port).await.unwrap();
    let snapshots = InMemorySnapshotStorage::new();
    let metadata = InMemoryMetadataStorage::new();
    let mut app = App::new(events, api, snapshots, metadata, contract_id);

    tracing::info!("Starting indexer service");
    loop {
        let interrupt = handle_interupt();
        match app.run(interrupt).await.unwrap() {
            RunState::Continue => continue,
            RunState::Exit => {
                tracing::info!("Exiting indexer service");
                return Ok(());
            }
        }
    }
}
