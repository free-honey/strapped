use anyhow::{
    Context,
    anyhow,
};
use clap::{
    ArgGroup,
    Parser,
};
use deployments::{
    DeploymentEnv,
    DeploymentStore,
};
use fuel_core::{
    state::rocks_db::{
        ColumnsPolicy,
        DatabaseConfig,
    },
    types::fuel_types::BlockHeight,
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
    snapshot_storage::SnapshotStorage,
};
use std::{
    convert::TryFrom,
    env::current_dir,
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
    contract_id: Option<String>,

    #[arg(long = "start-height")]
    start_height: Option<u32>,

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

fn parse_contract_id_str(raw: &str) -> anyhow::Result<ContractId> {
    let trimmed = raw.trim();
    let cleaned = trimmed.trim_start_matches("fuel");
    ContractId::from_str(cleaned)
        .map_err(|e| anyhow!("Failed to parse contract id '{raw}': {e:?}"))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    if args.tracing {
        init_tracing();
    }
    let (deployment_env, network_label) = if args.local {
        (DeploymentEnv::Local, "local")
    } else if args.dev {
        (DeploymentEnv::Dev, "dev")
    } else {
        (DeploymentEnv::Test, "test")
    };
    let store =
        DeploymentStore::new(deployment_env).context("opening deployments store")?;
    let stored_record = store.load().context("loading deployment")?;
    let user_contract_id = args
        .contract_id
        .as_ref()
        .map(|raw| parse_contract_id_str(raw).context("parsing --contract-id"))
        .transpose()?;
    let matches_cli_contract = |record: &deployments::DeploymentRecord,
                                cid: &ContractId| {
        parse_contract_id_str(&record.contract_id)
            .map(|parsed| parsed == *cid)
            .unwrap_or(false)
    };
    let selected_record = match (&user_contract_id, stored_record.clone()) {
        (Some(cid), Some(record)) if matches_cli_contract(&record, cid) => Some(record),
        (None, record) => record,
        _ => None,
    };
    let (contract_id, record_used) = match selected_record {
        Some(record) => {
            let contract_id =
                parse_contract_id_str(&record.contract_id).with_context(|| {
                    format!(
                        "parsing contract id from deployment record {}",
                        record.contract_id
                    )
                })?;
            (contract_id, Some(record))
        }
        None => {
            let cid = user_contract_id
                .ok_or_else(|| anyhow!(
                    "No deployment record found for {network_label}; provide --contract-id"
                ))?;
            (cid, None)
        }
    };
    let override_start_height = args.start_height;
    let mut start_height = if let Some(ref record) = record_used {
        match record.deployment_block_height {
            Some(height) => {
                u32::try_from(height).context("deployment block height exceeds u32")?
            }
            None => {
                tracing::warn!(
                    "Deployment record {} missing deployment_block_height; defaulting to 0",
                    record.contract_id
                );
                0
            }
        }
    } else {
        0
    };
    if record_used.is_none() && override_start_height.is_none() {
        return Err(anyhow!(
            "No deployment metadata available for contract {}; supply --start-height",
            contract_id
        ));
    }
    if let Some(custom_height) = override_start_height {
        start_height = custom_height;
    }
    let requested_start_height = start_height;
    if let Some(ref record) = record_used {
        tracing::info!(
            "Using deployment record {} (network {}) deployed at {} (block height {:?})",
            record.contract_id,
            record.network_url,
            record.deployed_at,
            record.deployment_block_height
        );
    } else {
        tracing::info!(
            "Using contract {} provided via CLI override with start height {}",
            contract_id,
            start_height
        );
    }
    let contract_dir_name = record_used
        .as_ref()
        .map(|record| record.contract_id.clone())
        .unwrap_or_else(|| contract_id.to_string());
    let execution_dir = current_dir().context("determine process working directory")?;
    let data_root = execution_dir
        .join("strapped_indexer_data")
        .join(network_label)
        .join(&contract_dir_name);
    fs::create_dir_all(&data_root)?;
    let event_data_path = data_root.join("events");
    fs::create_dir_all(&event_data_path)?;
    tracing::info!(
        "Using persistent event directory for {}: {}",
        contract_dir_name,
        event_data_path.display()
    );
    let storage_path = match &args.snapshot_dir {
        Some(path) => path.clone(),
        None => data_root.join("snapshots"),
    };
    fs::create_dir_all(&storage_path)?;
    tracing::info!(
        "Using sled storage directory for {}: {}",
        contract_dir_name,
        storage_path.display()
    );

    let (mut snapshots, metadata) = SledSnapshotStorage::open(&storage_path)?;
    let last_indexed_height = snapshots.latest_snapshot().map(|(_, height)| height).ok();
    if let Some(existing_height) = last_indexed_height {
        if existing_height > start_height {
            tracing::info!(
                "Found indexed state up to block height {}; overriding requested start {}",
                existing_height,
                requested_start_height
            );
        }
        start_height = start_height.max(existing_height);
    }
    snapshots.prune_from(start_height)?;
    if start_height != requested_start_height {
        tracing::info!(
            "Indexer will resume from block height {} (requested {})",
            start_height,
            requested_start_height
        );
    } else {
        tracing::info!("Indexer will start from block height {}", start_height);
    }
    let should_backfill_deployment_block = record_used.is_some()
        && last_indexed_height.is_none()
        && override_start_height.is_none();
    let event_start_height = if should_backfill_deployment_block {
        start_height.saturating_sub(1)
    } else {
        start_height
    };
    if should_backfill_deployment_block && event_start_height != start_height {
        tracing::info!(
            "Fuel receipts stream will backfill from block height {} to include the deployment block {}",
            event_start_height,
            start_height,
        );
    }
    let start_block_height: BlockHeight = event_start_height.into();

    let database_config = DatabaseConfig {
        cache_capacity: None,
        max_fds: 512,
        columns_policy: ColumnsPolicy::Lazy,
    };
    let indexer_config = IndexerConfig::new(start_block_height, args.graphql_url);
    let events = FuelIndexerEventSource::new(
        parse_event_logs,
        event_data_path.clone(),
        database_config,
        indexer_config,
        start_block_height,
    )
    .await?;
    let api = ActixQueryApi::new(args.port).await?;
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
