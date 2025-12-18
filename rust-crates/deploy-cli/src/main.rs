mod wallets;

use anyhow::{
    Context,
    Result,
};
use chrono::Utc;
use clap::{
    ArgGroup,
    Parser,
};
use deployments::{
    DeploymentEnv,
    DeploymentRecord,
    DeploymentStore,
};
use fuel_core_client::client::{
    FuelClient,
    types::TransactionStatus,
};
use fuels::{
    accounts::ViewOnlyAccount,
    prelude::{
        AssetId,
        CallParameters,
        Contract,
        LoadConfiguration,
        Provider,
        TxPolicies,
        VariableOutputPolicy,
    },
    programs::contract::{
        Contract as LoadedContract,
        Regular,
    },
    tx::TxId,
    types::{
        Address,
        Bits256,
        ContractId,
        Identity,
    },
};
use generated_abi::{
    pseudo_vrf_types,
    strapped_types,
};
use rand::Rng;
use std::{
    path::Path,
    str::FromStr,
};

use crate::wallets::{
    find_wallet,
    resolve_wallet_dir,
    unlock_wallet,
};

const DEFAULT_TESTNET_RPC_URL: &str = "https://testnet.fuel.network";
const DEFAULT_DEVNET_RPC_URL: &str = "https://devnet.fuel.network";
const DEFAULT_LOCAL_RPC_URL: &str = "http://localhost:4000/";
const DEFAULT_SAFE_SCRIPT_GAS_LIMIT: u64 = 29_000_000;
const FUNDING_AMOUNT: u64 = 100_000_000;
const STRAPPED_BIN_CANDIDATES: [&str; 1] =
    ["./sway-projects/strapped/out/release/strapped.bin"];
const VRF_BIN_CANDIDATES: [&str; 1] =
    ["./sway-projects/pseudo-vrf-contract/out/release/pseudo-vrf-contract.bin"];

#[derive(Parser, Debug)]
#[command(
    name = "strapped-deploy",
    about = "Deploy strapped or perform owner utilities (withdraw, balance)",
    version,
    group(
        ArgGroup::new("network")
            .args(["devnet", "testnet", "local"])
            .required(true)
    )
)]
struct Args {
    /// Deploy to Fuel devnet
    #[arg(long)]
    devnet: bool,

    /// Deploy to Fuel testnet
    #[arg(long)]
    testnet: bool,

    /// Deploy to a local Fuel node
    #[arg(long)]
    local: bool,

    /// Override RPC URL
    #[arg(long)]
    rpc_url: Option<String>,

    /// forc-wallet profile name
    #[arg(long)]
    wallet: String,

    /// Override forc-wallet directory (defaults to ~/.fuel/wallets)
    #[arg(long)]
    wallet_dir: Option<String>,

    /// Which action to perform (defaults to deploy)
    #[arg(short, long, value_enum, default_value = "deploy")]
    action: Action,

    /// Asset id to use for chips (defaults to the chain base asset, or stored deployment when not deploying)
    #[arg(long)]
    chip_asset_id: Option<String>,

    /// Amount of chips to fund the strapped contract with (deploy only)
    #[arg(long)]
    funding_amount: Option<u64>,

    /// Blocks between rolls (sets the initial next-roll height offset)
    #[arg(long)]
    roll_frequency: Option<u32>,

    /// Destination address for withdrawal (defaults to this wallet)
    #[arg(long)]
    withdraw_to: Option<String>,

    /// Withdrawal amount (required for withdraw action)
    #[arg(long)]
    withdraw: Option<u64>,
}

#[derive(Debug, Clone, clap::ValueEnum)]
enum Action {
    Deploy,
    Balance,
    Withdraw,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    deployments::ensure_structure().context("initializing deployment directories")?;

    let (env, default_url) = if args.devnet {
        (DeploymentEnv::Dev, DEFAULT_DEVNET_RPC_URL)
    } else if args.testnet {
        (DeploymentEnv::Test, DEFAULT_TESTNET_RPC_URL)
    } else {
        (DeploymentEnv::Local, DEFAULT_LOCAL_RPC_URL)
    };

    let rpc_url = args
        .rpc_url
        .clone()
        .unwrap_or_else(|| default_url.to_string());
    let provider = Provider::connect(&rpc_url)
        .await
        .context("failed to connect to provider")?;
    let client = FuelClient::new(rpc_url.clone())
        .map_err(|e| anyhow::anyhow!("failed creating Fuel client: {e}"))?;

    let wallet_dir = resolve_wallet_dir(args.wallet_dir.as_deref())
        .context("resolving wallet directory")?;
    let descriptor =
        find_wallet(&wallet_dir, &args.wallet).context("locating requested wallet")?;
    let wallet =
        unlock_wallet(&descriptor, &provider).context("unlocking forc-wallet profile")?;

    let consensus_parameters = provider
        .consensus_parameters()
        .await
        .context("fetching consensus parameters")?;
    let default_chip_asset = *consensus_parameters.base_asset_id();
    let max_gas_per_tx = consensus_parameters.tx_params().max_gas_per_tx();
    let safe_script_gas_limit = max_gas_per_tx
        .saturating_sub(1)
        .clamp(1, DEFAULT_SAFE_SCRIPT_GAS_LIMIT);
    let chip_asset_id = if let Some(arg) = args.chip_asset_id.as_deref() {
        parse_asset_id(arg)?
    } else {
        let encoded = hex::encode::<[u8; 32]>(default_chip_asset.into());
        println!(
            "No chip asset id specified, defaulting to the base asset id: 0x{}",
            encoded
        );
        default_chip_asset
    };

    let store = DeploymentStore::new(env).context("opening deployment store")?;

    let action = match args.action {
        Action::Deploy => Action::Deploy,
        Action::Balance => Action::Balance,
        Action::Withdraw => {
            args.withdraw.ok_or_else(|| {
                anyhow::anyhow!("--withdraw <amount> is required for withdraw action")
            })?;
            Action::Withdraw
        }
    };

    if matches!(action, Action::Balance | Action::Withdraw) {
        let record = latest_record(&store)?;
        let contract_id: ContractId = record
            .contract_id
            .parse()
            .map_err(|e| anyhow::anyhow!("parsing stored contract id: {e}"))?;
        let chip_asset_id = if let Some(arg) = args.chip_asset_id.as_deref() {
            parse_asset_id(arg)?
        } else if let Some(stored) = record.chip_asset_id.as_deref() {
            parse_asset_id(stored)?
        } else {
            default_chip_asset
        };

        if let Action::Balance = action {
            let balance = wallet
                .get_asset_balance(&chip_asset_id)
                .await
                .context("fetching wallet balance")?;
            println!("Wallet '{}'", args.wallet);
            println!(
                "  Chip balance (asset {}): {}",
                hex::encode::<[u8; 32]>(chip_asset_id.into()),
                balance
            );
            println!(
                "  Pot size: not available (contract lacks a pot getter; fetch via indexer)"
            );
        }

        if let Action::Withdraw = action {
            let amount = args
                .withdraw
                .expect("withdraw amount required for withdraw action");
            let to_identity = if let Some(raw) = args.withdraw_to.as_deref() {
                parse_identity(raw)?
            } else {
                Identity::Address(wallet.address())
            };
            let dest_display = args
                .withdraw_to
                .clone()
                .unwrap_or_else(|| wallet.address().to_string());
            let strap_instance =
                strapped_types::MyContract::new(contract_id, wallet.clone());
            strap_instance
                .methods()
                .request_house_withdrawal(amount, to_identity)
                .with_tx_policies(script_policies(safe_script_gas_limit))
                .with_variable_output_policy(VariableOutputPolicy::EstimateMinimum)
                .call()
                .await
                .context("submitting house withdrawal request")?;
            println!(
                "Requested withdrawal of {} chips to {} for contract {}",
                amount, dest_display, contract_id
            );
        }

        return Ok(());
    }

    let roll_frequency = args.roll_frequency.ok_or_else(|| {
        anyhow::anyhow!("--roll-frequency <blocks> is required when deploying")
    })?;
    if roll_frequency == 0 {
        return Err(anyhow::anyhow!("--roll-frequency must be greater than 0"));
    }

    let strap_path =
        choose_binary(&STRAPPED_BIN_CANDIDATES).context("locating strapped binary")?;
    let strap_hash = deployments::compute_bytecode_hash(strap_path)
        .context("hashing strapped binary")?;
    let strap_salt = rand::rng().random::<[u8; 32]>();
    let strap_contract = load_contract(&STRAPPED_BIN_CANDIDATES, strap_salt)
        .context("loading strapped contract binary")?;
    let strap_response = strap_contract
        .clone()
        .smart_deploy(&wallet, TxPolicies::default(), 4_096)
        .await
        .context("deploying strapped contract")?;
    let strap_contract_id = strap_response.contract_id;
    let strap_tx_id = strap_response
        .tx_id
        .context("deployment transaction missing")?;

    let strap_block_height = fetch_block_height(&client, &strap_tx_id)
        .await
        .context("fetching strapped deployment block height")?;

    println!(
        "Strapped contract deployed: {} (tx: {}) at block {}",
        strap_contract_id, strap_tx_id, strap_block_height
    );

    let vrf_salt = rand::rng().random::<[u8; 32]>();
    let vrf_contract = load_contract(&VRF_BIN_CANDIDATES, vrf_salt)
        .context("loading VRF contract binary")?;
    let vrf_response = vrf_contract
        .clone()
        .deploy(&wallet, TxPolicies::default())
        .await
        .context("deploying VRF contract")?;
    let vrf_contract_id = vrf_response.contract_id;
    let vrf_hash = deployments::compute_bytecode_hash(
        choose_binary(&VRF_BIN_CANDIDATES).context("locating VRF binary")?,
    )
    .context("hashing VRF binary")?;

    let vrf_instance =
        pseudo_vrf_types::PseudoVRFContract::new(vrf_contract_id, wallet.clone());
    let entropy = rand::rng().random();
    vrf_instance
        .methods()
        .set_entropy(entropy)
        .with_tx_policies(script_policies(safe_script_gas_limit))
        .call()
        .await
        .context("setting initial VRF entropy")?;

    let strap_instance =
        strapped_types::MyContract::new(strap_contract_id, wallet.clone());
    strap_instance
        .methods()
        .initialize(Bits256(*vrf_contract_id), chip_asset_id, roll_frequency)
        .with_tx_policies(script_policies(safe_script_gas_limit))
        .call()
        .await
        .context("initializing strapped contract")?;

    let funding_amount = args.funding_amount.unwrap_or(FUNDING_AMOUNT);
    let fund_call =
        CallParameters::new(funding_amount, chip_asset_id, safe_script_gas_limit);
    strap_instance
        .methods()
        .fund()
        .with_tx_policies(script_policies(safe_script_gas_limit))
        .with_variable_output_policy(VariableOutputPolicy::EstimateMinimum)
        .call_params(fund_call)?
        .call()
        .await
        .context("funding strapped contract with initial chips")?;

    let record = DeploymentRecord {
        deployed_at: Utc::now().to_rfc3339(),
        contract_id: strap_contract_id.to_string(),
        bytecode_hash: strap_hash,
        network_url: rpc_url.clone(),
        chip_asset_id: Some(format!(
            "0x{}",
            hex::encode::<[u8; 32]>(chip_asset_id.into())
        )),
        contract_salt: Some(format!("0x{}", hex::encode(strap_salt))),
        vrf_salt: Some(format!("0x{}", hex::encode(vrf_salt))),
        vrf_contract_id: Some(vrf_contract_id.to_string()),
        vrf_bytecode_hash: Some(format!("0x{}", vrf_hash)),
        deployment_block_height: Some(strap_block_height),
        roll_frequency: Some(roll_frequency),
    };

    store.append(record).context("recording deployment")?;
    println!("Deployment metadata written to {}", store.path().display());
    Ok(())
}

fn script_policies(limit: u64) -> TxPolicies {
    TxPolicies::default().with_script_gas_limit(limit)
}

fn choose_binary<'a>(paths: &'a [&str]) -> Result<&'a str> {
    paths
        .iter()
        .find(|p| Path::new(p).exists())
        .copied()
        .ok_or_else(|| anyhow::anyhow!("Contract binary not found. Tried {:?}", paths))
}

fn load_contract(paths: &[&str], salt: [u8; 32]) -> Result<LoadedContract<Regular>> {
    let path = choose_binary(paths)?;
    Contract::load_from(path, LoadConfiguration::default().with_salt(salt))
        .with_context(|| format!("Failed to load contract binary from {path}"))
}

fn parse_asset_id(raw: &str) -> Result<AssetId> {
    let cleaned = raw.strip_prefix("0x").unwrap_or(raw);
    let bytes = hex::decode(cleaned).context("decoding chip asset id")?;
    if bytes.len() != 32 {
        anyhow::bail!("chip asset id must be 32 bytes (64 hex chars)");
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(AssetId::from(arr))
}

async fn fetch_block_height(client: &FuelClient, tx_id: &TxId) -> Result<u64> {
    let status = client
        .transaction_status(tx_id)
        .await
        .context("querying transaction status")?;
    match status {
        TransactionStatus::Success { block_height, .. } => {
            let height: u32 = block_height.into();
            Ok(u64::from(height))
        }
        other => anyhow::bail!("transaction {tx_id} not successful: {:?}", other),
    }
}

fn parse_identity(raw: &str) -> Result<Identity> {
    let addr = raw
        .parse::<Address>()
        .or_else(|_| Address::from_str(raw))
        .map_err(|_| anyhow::anyhow!("unable to parse address/identity: {raw}"))?;
    Ok(Identity::Address(addr))
}

fn latest_record(store: &DeploymentStore) -> Result<DeploymentRecord> {
    let mut records = store.load().context("loading deployment records")?;
    records
        .pop()
        .ok_or_else(|| anyhow::anyhow!("no deployments found for this environment"))
}
