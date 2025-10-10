use crate::{
    deployment,
    ui,
    wallets,
};
use chrono::Utc;
use color_eyre::eyre::{
    Result,
    WrapErr,
    eyre,
};
use fuels::{
    accounts::ViewOnlyAccount,
    prelude::{
        AssetConfig,
        AssetId,
        Bech32ContractId,
        CallParameters,
        Contract,
        ContractId,
        Execution,
        LoadConfiguration,
        Provider,
        TxPolicies,
        VariableOutputPolicy,
        WalletUnlocked,
        WalletsConfig,
        launch_custom_provider_and_get_wallets,
    },
    programs::contract::{
        Contract as LoadedContract,
        Regular,
    },
    tx::ContractIdExt,
    types::Bits256,
};
use futures::future::try_join_all;
use rand::Rng;
use std::{
    collections::{
        HashMap,
        HashSet,
    },
    io::{
        self,
        Write,
    },
    path::{
        Path,
        PathBuf,
    },
    str::FromStr,
    time::{
        Duration,
        Instant,
    },
};
use strapped_contract::{
    pseudo_vrf_types as pseudo_vrf,
    strapped_types as strapped,
    vrf_types as fake_vrf,
};
use tokio::time;
use tracing::error;

pub const DEFAULT_TESTNET_RPC_URL: &str = "https://testnet.fuel.network";
pub const DEFAULT_DEVNET_RPC_URL: &str = "https://devnet.fuel.network";
pub const DEFAULT_LOCAL_RPC_URL: &str = "http://localhost:4000/";
const STRAPPED_BIN_CANDIDATES: [&str; 2] = [
    "strapped/out/release/strapped.bin",
    "strapped/out/debug/strapped.bin",
];
const VRF_BIN_CANDIDATES: [&str; 2] = [
    "pseudo-vrf-contract/out/release/pseudo-vrf-contract.bin",
    "pseudo-vrf-contract/out/debug/pseudo-vrf-contract.bin",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VrfMode {
    Fake,
    Pseudo,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkKind {
    InMemory,
    Remote,
}

#[derive(Clone)]
pub enum VrfClient {
    Fake(fake_vrf::FakeVRFContract<WalletUnlocked>),
    Pseudo(pseudo_vrf::PseudoVRFContract<WalletUnlocked>),
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum WalletKind {
    Owner,
    Alice,
}

#[derive(Clone, Debug, Default)]
pub struct BetsSummary {
    pub _by_roll: Vec<(strapped::Roll, u64 /* total amount */)>,
}

#[derive(Clone, Debug)]
pub struct AppSnapshot {
    pub wallet: WalletKind,
    pub current_game_id: u64,
    pub roll_history: Vec<strapped::Roll>,
    pub modifier_triggers:
        Vec<(strapped::Roll, strapped::Roll, strapped::Modifier, bool)>,
    pub active_modifiers: Vec<(
        strapped::Roll,
        strapped::Modifier,
        u64, // roll_index
    )>,
    pub owned_straps: Vec<(strapped::Strap, u64)>,
    pub pot_balance: u64,
    pub chip_balance: u64,
    pub selected_roll: strapped::Roll,
    pub vrf_number: u64,
    pub vrf_mode: VrfMode,
    pub current_block_height: u32,
    pub next_roll_height: Option<u32>,
    pub status: String,
    pub cells: Vec<RollCell>,
    pub previous_games: Vec<PreviousGameSummary>,
    pub errors: Vec<String>,
}

pub struct Clients {
    pub owner: strapped::MyContract<WalletUnlocked>,
    pub alice: strapped::MyContract<WalletUnlocked>,
    pub vrf: Option<VrfClient>,
    pub vrf_mode: VrfMode,
    pub contract_id: ContractId,
    pub chip_asset_id: AssetId,
    pub network: NetworkKind,
}

impl Clients {
    fn instance(&self, who: WalletKind) -> &strapped::MyContract<WalletUnlocked> {
        match who {
            WalletKind::Owner => &self.owner,
            WalletKind::Alice => &self.alice,
        }
    }
}

#[derive(Clone, Debug)]
pub enum NetworkTarget {
    InMemory,
    Testnet { url: String },
    Devnet { url: String },
    LocalNode { url: String },
}

#[derive(Clone, Debug)]
pub enum WalletConfig {
    Generated,
    ForcKeystore {
        owner: String,
        player: String,
        dir: PathBuf,
    },
}

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub vrf_mode: VrfMode,
    pub network: NetworkTarget,
    pub wallets: WalletConfig,
    pub deploy_if_missing: bool,
}

pub async fn init_local(vrf_mode: VrfMode) -> Result<Clients> {
    // Mirror TestContext: base asset + chip asset for two wallets.
    let base_asset = AssetConfig {
        id: AssetId::zeroed(),
        num_coins: 1,
        coin_amount: 1_000_000_000,
    };
    let chip_asset_id = AssetId::from([1u8; 32]);
    let chip_asset = AssetConfig {
        id: chip_asset_id,
        num_coins: 1,
        coin_amount: 1_000_000_000,
    };

    let mut wallets = launch_custom_provider_and_get_wallets(
        WalletsConfig::new_multiple_assets(2, vec![base_asset, chip_asset]),
        None,
        None,
    )
    .await?;

    let owner = wallets.pop().ok_or_else(|| eyre!("missing owner wallet"))?;
    let alice = wallets.pop().ok_or_else(|| eyre!("missing alice wallet"))?;

    // Deploy strapped
    let strap_bin = choose_binary(&STRAPPED_BIN_CANDIDATES)?;
    let strapped_id = Contract::load_from(strap_bin, LoadConfiguration::default())?
        .deploy(&owner, TxPolicies::default())
        .await?;
    let contract_id: ContractId = strapped_id.clone().into();
    let owner_instance = strapped::MyContract::new(strapped_id.clone(), owner.clone());
    let alice_instance = strapped::MyContract::new(strapped_id.clone(), alice.clone());

    let (vrf_client, vrf_contract_id): (VrfClient, ContractId) = match vrf_mode {
        VrfMode::Fake => {
            let vrf_bin = "fake-vrf-contract/out/debug/fake-vrf-contract.bin";
            let vrf_id = Contract::load_from(vrf_bin, LoadConfiguration::default())?
                .deploy(&owner, TxPolicies::default())
                .await?;
            let instance = fake_vrf::FakeVRFContract::new(vrf_id.clone(), owner.clone());
            instance.methods().set_number(19).call().await?;
            (VrfClient::Fake(instance), vrf_id.into())
        }
        VrfMode::Pseudo => {
            let vrf_bin = choose_binary(&VRF_BIN_CANDIDATES)?;
            let vrf_id = Contract::load_from(vrf_bin, LoadConfiguration::default())?
                .deploy(&owner, TxPolicies::default())
                .await?;
            let instance =
                pseudo_vrf::PseudoVRFContract::new(vrf_id.clone(), owner.clone());
            let mut random_gen = rand::rng();
            let entropy = random_gen.random();
            instance.methods().set_entropy(entropy).call().await?;
            (VrfClient::Pseudo(instance), vrf_id.into())
        }
    };

    // Initialize strapped contract
    owner_instance
        .methods()
        .initialize(Bits256(*vrf_contract_id), chip_asset_id, 10)
        .call()
        .await?;

    // Fund contract with initial chips so claims can be paid
    let fund_call = CallParameters::new(1_000_000u64, chip_asset_id, 1_000_000);
    owner_instance
        .methods()
        .fund()
        .call_params(fund_call)?
        .call()
        .await?;

    Ok(Clients {
        owner: owner_instance,
        alice: alice_instance,
        vrf: Some(vrf_client),
        vrf_mode,
        contract_id,
        chip_asset_id,
        network: NetworkKind::InMemory,
    })
}

async fn get_contract_asset_balance(
    provider: &Provider,
    cid: &ContractId,
    aid: &AssetId,
) -> Result<u64> {
    let bech: Bech32ContractId = (*cid).into();
    let bal = provider.get_contract_asset_balance(&bech, *aid).await?;
    Ok(bal)
}

pub struct AppController {
    pub clients: Clients,
    pub wallet: WalletKind,
    pub selected_roll: strapped::Roll,
    pub vrf_number: u64,
    pub status: String,
    last_seen_game_id_owner: Option<u64>,
    last_seen_game_id_alice: Option<u64>,
    shared_last_roll_history: Vec<strapped::Roll>,
    shared_prev_games: Vec<SharedGame>,
    owner_bets_hist: HashMap<u64, Vec<(strapped::Roll, Vec<(strapped::Bet, u64, u64)>)>>,
    alice_bets_hist: HashMap<u64, Vec<(strapped::Roll, Vec<(strapped::Bet, u64, u64)>)>>,
    owner_claimed: HashSet<u64>,
    alice_claimed: HashSet<u64>,
    prev_owner_bets: Vec<(strapped::Roll, Vec<(strapped::Bet, u64, u64)>)>,
    prev_alice_bets: Vec<(strapped::Roll, Vec<(strapped::Bet, u64, u64)>)>,
    strap_rewards_by_game: HashMap<u64, Vec<(strapped::Roll, strapped::Strap, u64)>>,
    active_modifiers_by_game:
        HashMap<u64, Vec<(strapped::Roll, strapped::Modifier, u64)>>,
    errors: Vec<String>,
    last_snapshot: Option<AppSnapshot>,
    last_snapshot_time: Option<Instant>,
}

impl AppController {
    fn from_clients(clients: Clients, initial_vrf: u64) -> Self {
        Self {
            clients,
            wallet: WalletKind::Alice,
            selected_roll: strapped::Roll::Six,
            vrf_number: initial_vrf,
            status: String::from("Ready"),
            last_seen_game_id_owner: None,
            last_seen_game_id_alice: None,
            shared_last_roll_history: Vec::new(),
            shared_prev_games: Vec::new(),
            owner_bets_hist: HashMap::new(),
            alice_bets_hist: HashMap::new(),
            owner_claimed: HashSet::new(),
            alice_claimed: HashSet::new(),
            prev_owner_bets: Vec::new(),
            prev_alice_bets: Vec::new(),
            strap_rewards_by_game: HashMap::new(),
            active_modifiers_by_game: HashMap::new(),
            errors: Vec::new(),
            last_snapshot: None,
            last_snapshot_time: None,
        }
    }

    fn poll_interval(&self) -> Duration {
        match self.clients.network {
            NetworkKind::Remote => Duration::from_secs(5),
            NetworkKind::InMemory => Duration::from_secs(1),
        }
    }

    fn refresh_ttl(&self) -> Duration {
        self.poll_interval()
    }

    fn invalidate_cache(&mut self) {
        self.last_snapshot = None;
        self.last_snapshot_time = None;
    }

    pub async fn new(config: AppConfig) -> Result<Self> {
        let AppConfig {
            vrf_mode,
            network,
            wallets,
            deploy_if_missing,
        } = config;
        match network {
            NetworkTarget::InMemory => Self::new_local(vrf_mode).await,
            NetworkTarget::Devnet { url } => {
                Self::new_remote(
                    vrf_mode,
                    deployment::DeploymentEnv::Dev,
                    url,
                    wallets,
                    deploy_if_missing,
                )
                .await
            }
            NetworkTarget::Testnet { url } => {
                Self::new_remote(
                    vrf_mode,
                    deployment::DeploymentEnv::Test,
                    url,
                    wallets,
                    deploy_if_missing,
                )
                .await
            }
            NetworkTarget::LocalNode { url } => {
                Self::new_remote(
                    vrf_mode,
                    deployment::DeploymentEnv::Local,
                    url,
                    wallets,
                    deploy_if_missing,
                )
                .await
            }
        }
    }

    async fn fetch_bets_for(
        &self,
        who: WalletKind,
    ) -> Result<Vec<(strapped::Roll, Vec<(strapped::Bet, u64, u64)>)>> {
        let contract = self.clients.instance(who).clone();
        let futures = all_rolls()
            .into_iter()
            .map(|roll| {
                let contract = contract.clone();
                async move {
                    let bets = contract
                        .methods()
                        .get_my_bets(roll.clone())
                        .simulate(Execution::Realistic)
                        .await?
                        .value;
                    Ok::<_, color_eyre::eyre::Report>((roll, bets))
                }
            })
            .collect::<Vec<_>>();
        let results = try_join_all(futures).await?;
        Ok(results)
    }

    pub async fn new_local(vrf_mode: VrfMode) -> Result<Self> {
        let clients = init_local(vrf_mode).await?;
        let initial_vrf = match vrf_mode {
            VrfMode::Fake => 19,
            VrfMode::Pseudo => 0,
        };
        Ok(Self::from_clients(clients, initial_vrf))
    }
    pub async fn new_remote(
        vrf_mode: VrfMode,
        env: deployment::DeploymentEnv,
        url: String,
        wallet_config: WalletConfig,
        deploy_if_missing: bool,
    ) -> Result<Self> {
        if matches!(vrf_mode, VrfMode::Fake) {
            return Err(eyre!(
                "Fake VRF mode is only supported in in-memory deployments"
            ));
        }

        let provider = Provider::connect(&url)
            .await
            .wrap_err_with(|| format!("Failed to connect to provider at {url}"))?;

        let (owner_name, player_name, wallet_dir) = match wallet_config {
            WalletConfig::ForcKeystore { owner, player, dir } => (owner, player, dir),
            WalletConfig::Generated => {
                return Err(eyre!(
                    "Remote networks require forc-wallet keystore selection"
                ));
            }
        };

        let owner_descriptor = wallets::find_wallet(&wallet_dir, &owner_name)
            .wrap_err("Unable to locate owner wallet")?;
        let owner_wallet = wallets::unlock_wallet(&owner_descriptor, &provider)?;

        let alice_wallet = if player_name == owner_name {
            owner_wallet.clone()
        } else {
            let player_descriptor = wallets::find_wallet(&wallet_dir, &player_name)
                .wrap_err("Unable to locate player wallet")?;
            wallets::unlock_wallet(&player_descriptor, &provider)?
        };

        let store = deployment::DeploymentStore::new(env)?;
        let records = store.load()?;
        let strap_binary = choose_binary(&STRAPPED_BIN_CANDIDATES)?;
        let bytecode_hash = deployment::compute_bytecode_hash(strap_binary)?;

        let mut compatible: Vec<_> = records
            .iter()
            .cloned()
            .filter(|record| record.is_compatible_with_hash(&bytecode_hash))
            .collect();

        let initial_vrf = match vrf_mode {
            VrfMode::Fake => 19,
            VrfMode::Pseudo => 0,
        };

        if compatible.is_empty() {
            if !deploy_if_missing {
                let summary = format_deployment_summary(
                    env,
                    &url,
                    &store,
                    &records,
                    &bytecode_hash,
                )?;
                return Err(eyre!(summary));
            }

            let default_chip_asset_id =
                *provider.consensus_parameters().await?.base_asset_id();
            let chip_asset_id = prompt_chip_asset_id(default_chip_asset_id)?;

            let (clients, record) = Self::deploy_new_remote_contract(
                &url,
                vrf_mode,
                owner_wallet.clone(),
                alice_wallet.clone(),
                chip_asset_id,
                &bytecode_hash,
            )
            .await?;
            // let vrf_contract_id: ContractId = record.vrf_contract_id.unwrap().into();
            let vrf_contract_id =
                Bech32ContractId::from_str(&record.clone().vrf_contract_id.unwrap());
            let vrf_contract_id = ContractId::from(vrf_contract_id.unwrap());

            // Initialize contracts
            if let Some(VrfClient::Pseudo(vrf_instance)) = &clients.vrf {
                let mut random_gen = rand::rng();
                let entropy = random_gen.random();
                vrf_instance
                    .methods()
                    .set_entropy(entropy)
                    .call()
                    .await
                    .wrap_err(format!("vrf contract id: {:?}", vrf_contract_id))?;
            }
            clients
                .owner
                .methods()
                .initialize(Bits256(*vrf_contract_id), chip_asset_id, 10)
                .call()
                .await?;
            let fund_call = CallParameters::new(1_000_000u64, chip_asset_id, 1_000_000);
            clients
                .owner
                .methods()
                .fund()
                .call_params(fund_call)?
                .call()
                .await?;

            store.append(record)?;
            // initialize contracts
            return Ok(Self::from_clients(clients, initial_vrf));
        }

        compatible.sort_by(|a, b| a.deployed_at.cmp(&b.deployed_at));
        let selected = compatible
            .last()
            .expect("compatible deployments list should not be empty")
            .clone();

        let contract_bech32 = Bech32ContractId::from_str(&selected.contract_id)
            .wrap_err("Deployment record contains an invalid contract id")?;
        let contract_id: ContractId = contract_bech32.clone().into();

        let owner_instance =
            strapped::MyContract::new(contract_bech32.clone(), owner_wallet.clone());
        let alice_instance =
            strapped::MyContract::new(contract_bech32.clone(), alice_wallet.clone());

        let chip_asset_id = owner_instance
            .methods()
            .current_chip_asset_id()
            .simulate(Execution::Realistic)
            .await?
            .value;

        let vrf_bits = owner_instance
            .methods()
            .current_vrf_contract_id()
            .simulate(Execution::StateReadOnly)
            .await?
            .value;

        let vrf_contract_id = ContractId::new(vrf_bits.0);
        let vrf_client = if vrf_bits.0 == [0u8; 32] {
            None
        } else {
            let vrf_bech32: Bech32ContractId = vrf_contract_id.into();
            Some(VrfClient::Pseudo(pseudo_vrf::PseudoVRFContract::new(
                vrf_bech32,
                owner_wallet.clone(),
            )))
        };

        let clients = Clients {
            owner: owner_instance,
            alice: alice_instance,
            vrf: vrf_client,
            vrf_mode,
            contract_id,
            chip_asset_id,
            network: NetworkKind::Remote,
        };

        Ok(Self::from_clients(clients, initial_vrf))
    }

    async fn deploy_new_remote_contract(
        url: &str,
        vrf_mode: VrfMode,
        owner_wallet: WalletUnlocked,
        alice_wallet: WalletUnlocked,
        chip_asset_id: AssetId,
        bytecode_hash: &str,
    ) -> Result<(Clients, deployment::DeploymentRecord)> {
        let strap_salt = rand::rng().random::<[u8; 32]>();
        let strapped = load_contract(&STRAPPED_BIN_CANDIDATES, strap_salt)?;
        let strapped_id = strapped
            .clone()
            .smart_deploy(&owner_wallet, TxPolicies::default(), 4_096)
            .await?;
        let contract_id: ContractId = strapped_id.clone().into();

        let owner_instance =
            strapped::MyContract::new(strapped_id.clone(), owner_wallet.clone());
        let alice_instance =
            strapped::MyContract::new(strapped_id.clone(), alice_wallet.clone());

        let (
            vrf_client,
            vrf_contract_id,
            vrf_salt_hex,
            vrf_contract_bech32,
            vrf_bytecode_hash,
        ) = match vrf_mode {
            VrfMode::Fake => {
                return Err(eyre!(
                    "Fake VRF mode is only supported in in-memory deployments"
                ));
            }
            VrfMode::Pseudo => {
                let vrf_salt = rand::rng().random::<[u8; 32]>();
                let vrf_contract = load_contract(&VRF_BIN_CANDIDATES, vrf_salt)?;
                let vrf_contract = vrf_contract
                    .clone()
                    .deploy(&owner_wallet, TxPolicies::default())
                    .await?;
                let vrf_instance = pseudo_vrf::PseudoVRFContract::new(
                    vrf_contract.clone(),
                    owner_wallet.clone(),
                );
                // let mut random_gen = rand::rng();
                // let entropy = random_gen.random();
                // vrf_instance.methods().set_entropy(entropy).call().await?;
                let vrf_contract_id: ContractId = vrf_contract.clone().into();
                tracing::info!("VRF contract deployed: {:?}", vrf_contract_id);
                let vrf_contract_bech32: Bech32ContractId =
                    vrf_contract_id.clone().into();
                let vrf_hash_hex = choose_binary(&VRF_BIN_CANDIDATES)
                    .and_then(|path| deployment::compute_bytecode_hash(path))
                    .ok()
                    .map(|hash| format!("0x{}", hash));
                (
                    Some(VrfClient::Pseudo(vrf_instance)),
                    vrf_contract_id,
                    Some(format!("0x{}", hex::encode(vrf_salt))),
                    Some(vrf_contract_bech32.to_string()),
                    vrf_hash_hex,
                )
            }
        };

        // owner_instance
        //     .methods()
        //     .initialize(Bits256(*vrf_contract_id), chip_asset_id, 10)
        //     .call()
        //     .await?;

        // let fund_call = CallParameters::new(1_000_000u64, chip_asset_id, 1_000_000);
        // owner_instance
        //     .methods()
        //     .fund()
        //     .call_params(fund_call)?
        //     .call()
        //     .await?;

        let record = deployment::DeploymentRecord {
            deployed_at: Utc::now().to_rfc3339(),
            contract_id: strapped_id.to_string(),
            bytecode_hash: bytecode_hash.to_string(),
            network_url: url.to_string(),
            chip_asset_id: Some(format!(
                "0x{}",
                hex::encode::<[u8; 32]>(chip_asset_id.into())
            )),
            contract_salt: Some(format!("0x{}", hex::encode(strap_salt))),
            vrf_salt: vrf_salt_hex,
            vrf_contract_id: vrf_contract_bech32,
            vrf_bytecode_hash,
        };

        let clients = Clients {
            owner: owner_instance,
            alice: alice_instance,
            vrf: vrf_client,
            vrf_mode,
            contract_id,
            chip_asset_id,
            network: NetworkKind::Remote,
        };

        Ok((clients, record))
    }

    pub async fn snapshot(&mut self, force_refresh: bool) -> Result<AppSnapshot> {
        if !force_refresh {
            if let (Some(last), Some(cache)) =
                (self.last_snapshot_time, self.last_snapshot.clone())
            {
                if last.elapsed() < self.refresh_ttl() {
                    return Ok(cache);
                }
            }
        }

        let who = self.wallet;
        let me = self.clients.instance(who);
        let provider = me
            .account()
            .provider()
            .ok_or_else(|| eyre!("no provider"))?
            .clone();

        let provider_for_height = provider.clone();
        let owner_for_next = self.clients.owner.clone();
        let me_for_game = me.clone();
        let me_for_history = me.clone();
        let me_for_rewards = me.clone();
        let me_for_modifiers = me.clone();
        let me_for_active = me.clone();

        let (
            current_block_height,
            next_roll_height,
            current_game_id,
            roll_history,
            strap_rewards,
            modifier_triggers,
            active_modifiers,
        ) = tokio::try_join!(
            async move {
                provider_for_height
                    .latest_block_height()
                    .await
                    .map_err(color_eyre::eyre::Report::from)
            },
            async move {
                owner_for_next
                    .methods()
                    .next_roll_height()
                    .simulate(Execution::StateReadOnly)
                    .await
                    .map(|r| r.value)
                    .map_err(color_eyre::eyre::Report::from)
            },
            async move {
                me_for_game
                    .methods()
                    .current_game_id()
                    .simulate(Execution::StateReadOnly)
                    .await
                    .map(|r| r.value)
                    .map_err(color_eyre::eyre::Report::from)
            },
            async move {
                me_for_history
                    .methods()
                    .roll_history()
                    .simulate(Execution::StateReadOnly)
                    .await
                    .map(|r| r.value)
                    .map_err(color_eyre::eyre::Report::from)
            },
            async move {
                me_for_rewards
                    .methods()
                    .strap_rewards()
                    .simulate(Execution::StateReadOnly)
                    .await
                    .map(|r| r.value)
                    .map_err(color_eyre::eyre::Report::from)
            },
            async move {
                me_for_modifiers
                    .methods()
                    .modifier_triggers()
                    .simulate(Execution::StateReadOnly)
                    .await
                    .map(|r| r.value)
                    .map_err(color_eyre::eyre::Report::from)
            },
            async move {
                me_for_active
                    .methods()
                    .active_modifiers()
                    .simulate(Execution::StateReadOnly)
                    .await
                    .map(|r| r.value)
                    .map_err(color_eyre::eyre::Report::from)
            }
        )?;

        self.active_modifiers_by_game
            .insert(current_game_id, active_modifiers.clone());

        // My bets by roll
        let my_bets = self.fetch_bets_for(self.wallet).await?;
        let all_rolls = all_rolls();

        // Refresh current bets for both users on each tick so rollover can snapshot both reliably
        let (new_owner_bets, new_alice_bets) = tokio::try_join!(
            self.fetch_bets_for(WalletKind::Owner),
            self.fetch_bets_for(WalletKind::Alice)
        )?;

        // Remember strap rewards for this current game (for later claim delta display)
        self.strap_rewards_by_game
            .entry(current_game_id)
            .or_insert_with(|| strap_rewards.clone());

        // detect rollover using the active wallet's last seen id (avoid holding mutable borrow across await)
        let last_seen_opt = match self.wallet {
            WalletKind::Owner => self.last_seen_game_id_owner,
            WalletKind::Alice => self.last_seen_game_id_alice,
        };
        if let Some(prev) = last_seen_opt {
            if current_game_id > prev {
                // Build shared game entry and bets for both users so rolls persist and bets differ
                let owner_bets = self.prev_owner_bets.clone();
                let alice_bets = self.prev_alice_bets.clone();
                let mut completed_rolls = self.shared_last_roll_history.clone();
                if !completed_rolls
                    .last()
                    .map(|r| matches!(r, strapped::Roll::Seven))
                    .unwrap_or(false)
                {
                    completed_rolls.push(strapped::Roll::Seven);
                }
                self.shared_prev_games.insert(
                    0,
                    SharedGame {
                        game_id: prev,
                        rolls: completed_rolls,
                        modifiers: self
                            .active_modifiers_by_game
                            .get(&prev)
                            .cloned()
                            .unwrap_or_default(),
                    },
                );
                self.owner_bets_hist.insert(prev, owner_bets);
                self.alice_bets_hist.insert(prev, alice_bets);
                // Reset both last seen ids to current so we won't insert twice on switch
                self.last_seen_game_id_owner = Some(current_game_id);
                self.last_seen_game_id_alice = Some(current_game_id);
            }
        }
        match self.wallet {
            WalletKind::Owner => self.last_seen_game_id_owner = Some(current_game_id),
            WalletKind::Alice => self.last_seen_game_id_alice = Some(current_game_id),
        };
        // Update cached prev bets for next rollover detection
        self.prev_owner_bets = new_owner_bets;
        self.prev_alice_bets = new_alice_bets;

        let pot_balance = get_contract_asset_balance(
            &provider,
            &self.clients.contract_id,
            &self.clients.chip_asset_id,
        )
        .await?;

        let chip_balance = me
            .account()
            .get_asset_balance(&self.clients.chip_asset_id)
            .await?;

        // update shared rolls (one game globally)
        // update shared rolls (one game globally)
        self.shared_last_roll_history = roll_history.clone();

        // build cells for UI
        let mut cells = Vec::new();
        let mut unique_straps: Vec<strapped::Strap> = Vec::new();
        for (r, bets) in &my_bets {
            let chip_total: u64 = bets
                .iter()
                .filter_map(|(b, amt, _)| match b {
                    strapped::Bet::Chip => Some(*amt),
                    _ => None,
                })
                .sum();
            // Aggregate strap bets per unique strap without requiring Hash
            let mut straps: Vec<(strapped::Strap, u64)> = Vec::new();
            for (b, amt, _) in bets {
                if let strapped::Bet::Strap(s) = b {
                    if let Some((_es, total)) = straps.iter_mut().find(|(es, _)| es == s)
                    {
                        *total += *amt;
                    } else {
                        straps.push((s.clone(), *amt));
                    }
                    if !unique_straps.iter().any(|es| *es == *s) {
                        unique_straps.push(s.clone());
                    }
                }
            }
            let strap_total: u64 = straps.iter().map(|(_, n)| *n).sum();
            // rewards for this roll (count available rewards, not wallet balance)
            let mut rewards: Vec<RewardInfo> = Vec::new();
            for (_rr, s, cost) in strap_rewards.iter().filter(|(rr, _, _)| rr == r) {
                if let Some(existing) = rewards
                    .iter_mut()
                    .find(|info| info.strap == *s && info.cost == *cost)
                {
                    existing.count += 1;
                } else {
                    rewards.push(RewardInfo {
                        strap: s.clone(),
                        cost: *cost,
                        count: 1,
                    });
                }
                if !unique_straps.iter().any(|es| *es == *s) {
                    unique_straps.push(s.clone());
                }
            }
            cells.push(RollCell {
                roll: r.clone(),
                chip_total,
                strap_total,
                straps,
                rewards,
            });
        }

        // Sum owned straps for known strap variants (from current bets/rewards + all known rewards by game)
        let mut unique_straps = unique_straps;
        for (_gid, list) in &self.strap_rewards_by_game {
            for (_r, s, _) in list {
                if !unique_straps.iter().any(|es| *es == *s) {
                    unique_straps.push(s.clone());
                }
            }
        }

        let mut strap_balance: u64 = 0;
        let mut owned_straps: Vec<(strapped::Strap, u64)> = Vec::new();
        for s in unique_straps {
            let sub = strapped_contract::strap_to_sub_id(&s);
            let aid = self.clients.contract_id.asset_id(&sub);
            let bal = me.account().get_asset_balance(&aid).await.unwrap_or(0);
            strap_balance = strap_balance.saturating_add(bal);
            if bal > 0 {
                owned_straps.push((s, bal));
            }
        }

        // Build previous games by merging shared games with current user's stored bets
        let mut summaries: Vec<PreviousGameSummary> = Vec::new();
        for sg in &self.shared_prev_games {
            let bets = match self.wallet {
                WalletKind::Owner => self
                    .owner_bets_hist
                    .get(&sg.game_id)
                    .cloned()
                    .unwrap_or_default(),
                WalletKind::Alice => self
                    .alice_bets_hist
                    .get(&sg.game_id)
                    .cloned()
                    .unwrap_or_default(),
            };
            // Build cells from bets
            let mut cells = Vec::new();
            for r in &all_rolls {
                let bets_for = bets
                    .iter()
                    .find(|(rr, _)| rr == r)
                    .map(|(_, b)| b)
                    .cloned()
                    .unwrap_or_default();
                let chip_total: u64 = bets_for
                    .iter()
                    .filter_map(|(b, amt, _)| match b {
                        strapped::Bet::Chip => Some(*amt),
                        _ => None,
                    })
                    .sum();
                let strap_total: u64 = bets_for
                    .iter()
                    .filter_map(|(b, amt, _)| match b {
                        strapped::Bet::Strap(_) => Some(*amt),
                        _ => None,
                    })
                    .sum();
                cells.push(RollCell {
                    roll: r.clone(),
                    chip_total,
                    strap_total,
                    straps: Vec::new(),
                    rewards: Vec::new(),
                });
            }
            let claimed = match self.wallet {
                WalletKind::Owner => self.owner_claimed.contains(&sg.game_id),
                WalletKind::Alice => self.alice_claimed.contains(&sg.game_id),
            };
            summaries.push(PreviousGameSummary {
                game_id: sg.game_id,
                cells,
                modifiers: sg.modifiers.clone(),
                rolls: sg.rolls.clone(),
                bets_by_roll: bets,
                claimed,
            });
        }
        let previous_games = summaries;

        let snapshot = AppSnapshot {
            wallet: self.wallet,
            current_game_id,
            roll_history,
            modifier_triggers,
            active_modifiers,
            owned_straps,
            pot_balance,
            chip_balance,
            selected_roll: self.selected_roll.clone(),
            vrf_number: self.vrf_number,
            vrf_mode: self.clients.vrf_mode,
            current_block_height,
            next_roll_height,
            status: self.status.clone(),
            cells,
            previous_games,
            errors: self.errors.iter().rev().take(5).cloned().collect(),
        };

        self.last_snapshot = Some(snapshot.clone());
        self.last_snapshot_time = Some(Instant::now());

        Ok(snapshot)
    }

    pub fn set_wallet(&mut self, w: WalletKind) {
        self.wallet = w;
        self.invalidate_cache();
    }
    pub fn select_next_roll(&mut self) {
        self.selected_roll = next_roll(self.selected_roll.clone());
    }
    pub fn select_prev_roll(&mut self) {
        self.selected_roll = prev_roll(self.selected_roll.clone());
    }
    pub fn inc_vrf(&mut self) {
        self.vrf_number = self.vrf_number.wrapping_add(1);
    }
    pub fn dec_vrf(&mut self) {
        self.vrf_number = self.vrf_number.wrapping_sub(1);
    }

    pub async fn place_chip_bet(&mut self, amount: u64) -> Result<()> {
        let me = self.clients.instance(self.wallet);
        let call = CallParameters::new(amount, self.clients.chip_asset_id, 1_000_000);
        me.methods()
            .place_bet(self.selected_roll.clone(), strapped::Bet::Chip, amount)
            .call_params(call)?
            .call()
            .await?;
        self.status = format!("Placed {} chip(s) on {:?}", amount, self.selected_roll);
        self.invalidate_cache();
        Ok(())
    }

    pub async fn place_strap_bet(
        &mut self,
        strap: strapped::Strap,
        amount: u64,
    ) -> Result<()> {
        let me = self.clients.instance(self.wallet);
        let sub = strapped_contract::strap_to_sub_id(&strap);
        let asset_id = self.clients.contract_id.asset_id(&sub);
        let call = CallParameters::new(amount, asset_id, 1_000_000);
        me.methods()
            .place_bet(
                self.selected_roll.clone(),
                strapped::Bet::Strap(strap.clone()),
                amount,
            )
            .call_params(call)?
            .call()
            .await?;
        self.status = format!(
            "Placed {} of {} on {:?}",
            amount,
            super_compact_strap(&strap),
            self.selected_roll
        );
        self.invalidate_cache();
        Ok(())
    }

    pub async fn purchase_triggered_modifier(&mut self, cost: u64) -> Result<()> {
        // Find a triggered modifier that targets the selected roll
        let me = self.clients.instance(self.wallet);
        let triggers = me
            .methods()
            .modifier_triggers()
            .simulate(Execution::StateReadOnly)
            .await?
            .value;
        if let Some((_, target, modifier, _triggered)) = triggers
            .into_iter()
            .find(|(_, target, _, triggered)| *target == self.selected_roll && *triggered)
        {
            let call = CallParameters::new(cost, self.clients.chip_asset_id, 1_000_000);
            me.methods()
                .purchase_modifier(target.clone(), modifier.clone())
                .call_params(call)?
                .call()
                .await?;
            self.status = format!("Purchased {:?} for {:?}", modifier, target);
        } else {
            self.status = String::from("No triggered modifier for selected roll");
        }
        self.invalidate_cache();
        Ok(())
    }

    pub async fn set_vrf_number(&mut self, n: u64) -> Result<()> {
        match &self.clients.vrf {
            Some(VrfClient::Fake(vrf)) => {
                vrf.methods().set_number(n).call().await?;
                self.vrf_number = n;
                self.status = format!("VRF set to {}", n);
            }
            Some(VrfClient::Pseudo(_)) => {
                self.status =
                    String::from("Pseudo VRF mode does not support manual adjustment");
            }
            None => {
                self.status =
                    String::from("VRF controls are unavailable on this network");
            }
        }
        self.invalidate_cache();
        Ok(())
    }

    pub async fn roll(&mut self) -> Result<()> {
        // advance chain to next roll height
        let next_roll_height = self
            .clients
            .owner
            .methods()
            .next_roll_height()
            .simulate(Execution::StateReadOnly)
            .await?
            .value
            .ok_or_else(|| eyre!("Next roll height not scheduled"))?;
        let provider = self
            .clients
            .owner
            .account()
            .provider()
            .ok_or_else(|| eyre!("no provider"))?
            .clone();
        let current_height = provider
            .latest_block_height()
            .await
            .wrap_err("Failed to fetch latest block height")?;

        if current_height < next_roll_height {
            match self.clients.network {
                NetworkKind::InMemory => {
                    let blocks_to_advance =
                        next_roll_height.saturating_sub(current_height);
                    provider
                        .produce_blocks(blocks_to_advance, None)
                        .await
                        .wrap_err("Failed to produce blocks in local provider")?;
                }
                NetworkKind::Remote => {
                    self.status = format!(
                        "Waiting for block {} (current height {}) before rolling",
                        next_roll_height, current_height
                    );
                    return Ok(());
                }
            }
        }
        // Roll using owner instance but allow any wallet to trigger.
        match &self.clients.vrf {
            Some(VrfClient::Fake(vrf)) => {
                self.clients
                    .owner
                    .methods()
                    .roll_dice()
                    .with_contracts(&[vrf])
                    .call()
                    .await?;
            }
            Some(VrfClient::Pseudo(vrf)) => {
                self.clients
                    .owner
                    .methods()
                    .roll_dice()
                    .with_contracts(&[vrf])
                    .call()
                    .await?;
            }
            None => {
                self.status = String::from("VRF contract unavailable; cannot roll");
                return Ok(());
            }
        }
        self.status = String::from("Rolled dice");
        self.invalidate_cache();
        Ok(())
    }

    pub async fn claim_game(
        &mut self,
        game_id: u64,
        enabled: Vec<(strapped::Roll, strapped::Modifier)>,
    ) -> Result<()> {
        let me = self.clients.instance(self.wallet);
        let mut errs: Vec<String> = Vec::new();
        // pre-claim balances
        let pre_chip = me
            .account()
            .get_asset_balance(&self.clients.chip_asset_id)
            .await
            .unwrap_or(0);
        let upgraded_straps = self.expected_upgraded_straps(game_id, &enabled);
        let strap_list = self
            .strap_rewards_by_game
            .get(&game_id)
            .cloned()
            .unwrap_or_default();
        let mut strap_candidates: Vec<strapped::Strap> = Vec::new();
        for (_roll, strap, _) in &strap_list {
            if !strap_candidates.iter().any(|existing| existing == strap) {
                strap_candidates.push(strap.clone());
            }
        }
        for (_roll, strap) in &upgraded_straps {
            if !strap_candidates.iter().any(|existing| existing == strap) {
                strap_candidates.push(strap.clone());
            }
        }
        let mut pre_straps: Vec<(strapped::Strap, u64)> = Vec::new();
        for strap in &strap_candidates {
            let sub = strapped_contract::strap_to_sub_id(strap);
            let aid = self.clients.contract_id.asset_id(&sub);
            let bal = me.account().get_asset_balance(&aid).await.unwrap_or(0);
            pre_straps.push((strap.clone(), bal));
        }

        let mut claimed_ok = false;
        match me
            .methods()
            .claim_rewards(game_id, enabled.clone())
            .with_variable_output_policy(VariableOutputPolicy::EstimateMinimum)
            .call()
            .await
        {
            Ok(_) => {
                claimed_ok = true;
            }
            Err(e) => {
                error!(
                    %game_id,
                    error = %e,
                    "claim_rewards call failed"
                );
                errs.push(format!("claim(game_id={}) error: {}", game_id, e));
            }
        }
        if !claimed_ok {
            self.status = format!("Claim failed for game {}", game_id);
            self.push_errors(errs);
            return Ok(());
        }
        {
            let entry = self
                .strap_rewards_by_game
                .entry(game_id)
                .or_insert_with(Vec::new);
            for (roll, strap) in &upgraded_straps {
                if !entry.iter().any(|(_, existing, _)| existing == strap) {
                    let cost = Self::strap_cost(strap);
                    entry.push((roll.clone(), strap.clone(), cost));
                }
            }
        }
        // mark as claimed in local cache for the current user
        match self.wallet {
            WalletKind::Owner => {
                self.owner_claimed.insert(game_id);
            }
            WalletKind::Alice => {
                self.alice_claimed.insert(game_id);
            }
        }
        // post-claim deltas
        let post_chip = me
            .account()
            .get_asset_balance(&self.clients.chip_asset_id)
            .await
            .unwrap_or(0);
        let chip_delta = post_chip.saturating_sub(pre_chip);
        let mut strap_deltas: Vec<String> = Vec::new();
        for (s, pre) in pre_straps {
            let sub = strapped_contract::strap_to_sub_id(&s);
            let aid = self.clients.contract_id.asset_id(&sub);
            let post = me.account().get_asset_balance(&aid).await.unwrap_or(0);
            let d = post.saturating_sub(pre);
            if d > 0 {
                strap_deltas.push(format!("{} x{}", super_compact_strap(&s), d));
            }
        }
        let strap_part = if strap_deltas.is_empty() {
            String::from("")
        } else {
            format!(" | Straps: {}", strap_deltas.join(" "))
        };
        self.status = format!(
            "Claimed game {} | Chips +{}{}",
            game_id, chip_delta, strap_part
        );
        self.push_errors(errs);
        self.invalidate_cache();
        Ok(())
    }

    fn expected_upgraded_straps(
        &self,
        game_id: u64,
        enabled: &[(strapped::Roll, strapped::Modifier)],
    ) -> Vec<(strapped::Roll, strapped::Strap)> {
        let bets_hist = match self.wallet {
            WalletKind::Owner => self.owner_bets_hist.get(&game_id),
            WalletKind::Alice => self.alice_bets_hist.get(&game_id),
        };
        let bets_hist = match bets_hist {
            Some(bets) => bets.clone(),
            None => return Vec::new(),
        };

        let rolls = self
            .shared_prev_games
            .iter()
            .find(|g| g.game_id == game_id)
            .map(|g| g.rolls.clone())
            .unwrap_or_default();
        if rolls.is_empty() {
            return Vec::new();
        }

        let active_modifiers = self
            .active_modifiers_by_game
            .get(&game_id)
            .cloned()
            .unwrap_or_default();

        let mut upgrades: Vec<(strapped::Roll, strapped::Strap)> = Vec::new();
        for (idx, roll) in rolls.iter().enumerate() {
            if let Some((_, bets)) = bets_hist.iter().find(|(r, _)| r == roll) {
                for (bet, _amount, bet_roll_index) in bets {
                    if *bet_roll_index <= idx as u64 {
                        if let strapped::Bet::Strap(strap) = bet {
                            let mut new_strap = strap.clone();
                            new_strap.level = new_strap.level.saturating_add(1);
                            if let Some(modifier) = Self::modifier_override_for_roll(
                                &active_modifiers,
                                roll,
                                *bet_roll_index,
                                enabled,
                            ) {
                                new_strap.modifier = modifier;
                            }
                            upgrades.push((roll.clone(), new_strap));
                        }
                    }
                }
            }
        }

        upgrades
    }

    fn modifier_override_for_roll(
        active: &[(strapped::Roll, strapped::Modifier, u64)],
        roll: &strapped::Roll,
        bet_roll_index: u64,
        enabled: &[(strapped::Roll, strapped::Modifier)],
    ) -> Option<strapped::Modifier> {
        for (modifier_roll, modifier, activated_index) in active {
            if modifier_roll == roll && *activated_index <= bet_roll_index {
                let is_enabled = enabled
                    .iter()
                    .any(|(r, m)| r == modifier_roll && m == modifier);
                if is_enabled {
                    return Some(modifier.clone());
                } else {
                    return None;
                }
            }
        }
        None
    }

    pub async fn purchase_modifier_for(
        &mut self,
        target: strapped::Roll,
        modifier: strapped::Modifier,
        cost: u64,
    ) -> Result<()> {
        let me = self.clients.instance(self.wallet);
        let call = CallParameters::new(cost, self.clients.chip_asset_id, 1_000_000);
        me.methods()
            .purchase_modifier(target.clone(), modifier.clone())
            .call_params(call)?
            .call()
            .await?;
        self.status = format!("Purchased {:?} for {:?}", modifier, target);
        self.invalidate_cache();
        Ok(())
    }
}

fn super_compact_strap(s: &strapped::Strap) -> String {
    let mod_emoji = match s.modifier {
        strapped::Modifier::Nothing => "",
        strapped::Modifier::Burnt => "🧯",
        strapped::Modifier::Lucky => "🍀",
        strapped::Modifier::Holy => "👼",
        strapped::Modifier::Holey => "🫥",
        strapped::Modifier::Scotch => "🏴",
        strapped::Modifier::Soaked => "🌊",
        strapped::Modifier::Moldy => "🍄",
        strapped::Modifier::Starched => "🏳️",
        strapped::Modifier::Evil => "😈",
        strapped::Modifier::Groovy => "✌️",
        strapped::Modifier::Delicate => "❤️",
    };
    let kind_emoji = match s.kind {
        strapped::StrapKind::Shirt => "👕",
        strapped::StrapKind::Pants => "👖",
        strapped::StrapKind::Shoes => "👟",
        strapped::StrapKind::Dress => "👗",
        strapped::StrapKind::Hat => "🎩",
        strapped::StrapKind::Glasses => "👓",
        strapped::StrapKind::Watch => "⌚",
        strapped::StrapKind::Ring => "💍",
        strapped::StrapKind::Necklace => "📿",
        strapped::StrapKind::Earring => "🧷",
        strapped::StrapKind::Bracelet => "🧶",
        strapped::StrapKind::Tattoo => "🐉",
        strapped::StrapKind::Skirt => "👚",
        strapped::StrapKind::Piercing => "📌",
        strapped::StrapKind::Coat => "🧥",
        strapped::StrapKind::Scarf => "🧣",
        strapped::StrapKind::Gloves => "🧤",
        strapped::StrapKind::Gown => "👘",
        strapped::StrapKind::Belt => "🧵",
    };
    format!("{}{}{}", mod_emoji, kind_emoji, s.level)
}

impl AppController {
    fn strap_cost(strap: &strapped::Strap) -> u64 {
        match strap.kind {
            strapped::StrapKind::Shirt => 10,
            strapped::StrapKind::Pants => 10,
            strapped::StrapKind::Shoes => 10,
            strapped::StrapKind::Dress => 10,
            strapped::StrapKind::Hat => 20,
            strapped::StrapKind::Glasses => 20,
            strapped::StrapKind::Watch => 20,
            strapped::StrapKind::Ring => 20,
            strapped::StrapKind::Necklace => 50,
            strapped::StrapKind::Earring => 50,
            strapped::StrapKind::Bracelet => 50,
            strapped::StrapKind::Tattoo => 50,
            strapped::StrapKind::Skirt => 50,
            strapped::StrapKind::Piercing => 50,
            strapped::StrapKind::Coat => 100,
            strapped::StrapKind::Scarf => 100,
            strapped::StrapKind::Gloves => 100,
            strapped::StrapKind::Gown => 100,
            strapped::StrapKind::Belt => 200,
        }
    }

    fn push_errors(&mut self, mut items: Vec<String>) {
        if items.is_empty() {
            return;
        }
        for item in &items {
            error!("{}", item);
        }
        self.errors.append(&mut items);
        if self.errors.len() > 50 {
            let drain = self.errors.len() - 50;
            self.errors.drain(0..drain);
        }
    }
}

pub fn all_rolls() -> Vec<strapped::Roll> {
    use strapped::Roll::*;
    vec![
        Two, Three, Four, Five, Six, Seven, Eight, Nine, Ten, Eleven, Twelve,
    ]
}

fn next_roll(r: strapped::Roll) -> strapped::Roll {
    let rolls = all_rolls();
    let idx = rolls.iter().position(|x| *x == r).unwrap_or(0);
    rolls[(idx + 1) % rolls.len()].clone()
}

fn prev_roll(r: strapped::Roll) -> strapped::Roll {
    let rolls = all_rolls();
    let idx = rolls.iter().position(|x| *x == r).unwrap_or(0);
    rolls[(idx + rolls.len() - 1) % rolls.len()].clone()
}

#[derive(Clone, Debug)]
pub struct RollCell {
    pub roll: strapped::Roll,
    pub chip_total: u64,
    pub strap_total: u64,
    pub straps: Vec<(strapped::Strap, u64)>,
    pub rewards: Vec<RewardInfo>,
}

#[derive(Clone, Debug)]
pub struct RewardInfo {
    pub strap: strapped::Strap,
    pub cost: u64,
    pub count: u64,
}

#[derive(Clone, Debug)]
pub struct PreviousGameSummary {
    pub game_id: u64,
    pub cells: Vec<RollCell>,
    pub modifiers: Vec<(strapped::Roll, strapped::Modifier, u64)>,
    pub rolls: Vec<strapped::Roll>,
    pub bets_by_roll: Vec<(strapped::Roll, Vec<(strapped::Bet, u64, u64)>)>,
    pub claimed: bool,
}

#[derive(Clone, Debug)]
struct SharedGame {
    game_id: u64,
    rolls: Vec<strapped::Roll>,
    modifiers: Vec<(strapped::Roll, strapped::Modifier, u64)>,
}

fn format_deployment_summary(
    env: deployment::DeploymentEnv,
    url: &str,
    store: &deployment::DeploymentStore,
    records: &[deployment::DeploymentRecord],
    current_hash: &str,
) -> Result<String> {
    let mut message = format!(
        "No compatible deployments recorded for {env} at {url}.\n\nRecorded deployments for {env}:",
    );

    if records.is_empty() {
        message.push_str("\n  (none recorded)");
    } else {
        for record in records {
            let compat = if record.is_compatible_with_hash(current_hash) {
                " [compatible]"
            } else {
                ""
            };
            let asset_info = record.chip_asset_id.as_deref().unwrap_or("(unknown asset)");
            let contract_salt =
                record.contract_salt.as_deref().unwrap_or("(unknown salt)");
            let vrf_salt = record.vrf_salt.as_deref().unwrap_or("(unknown vrf salt)");
            let vrf_contract = record
                .vrf_contract_id
                .as_deref()
                .unwrap_or("(unknown vrf id)");
            let vrf_hash = record
                .vrf_bytecode_hash
                .as_deref()
                .unwrap_or("(unknown vrf hash)");
            message.push_str(&format!(
                "\n  {} - {} @ {} (hash {}){} asset {} contract_salt {} vrf_salt {} vrf_contract {} vrf_hash {}",
                record.deployed_at,
                record.contract_id,
                record.network_url,
                hash_preview(&record.bytecode_hash),
                compat,
                asset_info,
                contract_salt,
                vrf_salt,
                vrf_contract,
                vrf_hash,
            ));
        }
    }

    message.push_str(&format!(
        "\n\nCurrent local bytecode hash: {}",
        hash_preview(current_hash)
    ));
    message.push_str(&format!(
        "\nDeployment records file: {}",
        store.path().display()
    ));

    message
        .push_str("\n\nRun again with --deploy to publish a new compatible deployment.");

    Ok(message)
}

fn hash_preview(hash: &str) -> String {
    let preview_len = hash.len().min(16);
    let mut preview = hash[..preview_len].to_string();
    if hash.len() > preview_len {
        preview.push_str("...");
    }
    preview
}

fn prompt_chip_asset_id(default: AssetId) -> Result<AssetId> {
    let default_bytes: [u8; 32] = default.into();
    let default_hex = format!("0x{}", hex::encode(default_bytes));
    print!("Enter chip asset id to use [{}]: ", default_hex);
    io::stdout()
        .flush()
        .wrap_err("Failed to flush prompt to stdout")?;
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .wrap_err("Failed to read chip asset id")?;
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(default);
    }
    let cleaned = trimmed.strip_prefix("0x").unwrap_or(trimmed);
    if cleaned.len() != 64 {
        return Err(eyre!("Asset id must be 32-byte hex string (64 characters)"));
    }
    let mut bytes = [0u8; 32];
    hex::decode_to_slice(cleaned, &mut bytes as &mut [u8])
        .map_err(|_| eyre!("Invalid hex string for asset id"))?;
    Ok(AssetId::from(bytes))
}

fn choose_binary<'a>(paths: &'a [&str]) -> Result<&'a str> {
    paths
        .iter()
        .find(|p| Path::new(p).exists())
        .copied()
        .ok_or_else(|| eyre!("Contract binary not found. Tried {:?}", paths))
}

fn load_contract(paths: &[&str], salt: [u8; 32]) -> Result<LoadedContract<Regular>> {
    let path = choose_binary(paths)?;
    Contract::load_from(path, LoadConfiguration::default().with_salt(salt))
        .wrap_err_with(|| format!("Failed to load contract binary from {path}"))
}

pub async fn run_app(config: AppConfig) -> Result<()> {
    let mut controller = AppController::new(config).await?;
    let mut ui_state = ui::UiState::default();

    // UI bootstrap
    ui::terminal_enter(&mut ui_state)?;
    let res = run_loop(&mut controller, &mut ui_state).await;
    ui::terminal_exit()?;
    res
}

async fn run_loop(
    controller: &mut AppController,
    ui_state: &mut ui::UiState,
) -> Result<()> {
    let mut ticker = time::interval(controller.poll_interval());
    let mut last_snapshot = controller.snapshot(true).await?;
    ui::draw(ui_state, &last_snapshot)?;
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => { break; }
            _ = ticker.tick() => {
                last_snapshot = controller.snapshot(false).await?;
                ui::draw(ui_state, &last_snapshot)?;
            }
            ev = ui::next_event(ui_state) => {
                let mut force_refresh = false;
                match ev? {
                    ui::UserEvent::Quit => break,
                    ui::UserEvent::NextRoll => {
                        controller.select_next_roll();
                        if let Some(cache) = controller.last_snapshot.as_mut() {
                            cache.selected_roll = controller.selected_roll.clone();
                        }
                        last_snapshot.selected_roll = controller.selected_roll.clone();
                        ui::draw(ui_state, &last_snapshot)?;
                        continue;
                    }
                    ui::UserEvent::PrevRoll => {
                        controller.select_prev_roll();
                        if let Some(cache) = controller.last_snapshot.as_mut() {
                            cache.selected_roll = controller.selected_roll.clone();
                        }
                        last_snapshot.selected_roll = controller.selected_roll.clone();
                        ui::draw(ui_state, &last_snapshot)?;
                        continue;
                    }
                    ui::UserEvent::Owner => {
                        controller.set_wallet(WalletKind::Owner);
                        force_refresh = true;
                    }
                    ui::UserEvent::Alice => {
                        controller.set_wallet(WalletKind::Alice);
                        force_refresh = true;
                    }
                    ui::UserEvent::PlaceBetAmount(amount) => {
                        let _ = controller.place_chip_bet(amount).await;
                        force_refresh = true;
                    }
                    ui::UserEvent::Purchase => {
                        let _ = controller.purchase_triggered_modifier(1).await;
                        force_refresh = true;
                    }
                    ui::UserEvent::ConfirmStrapBet { strap, amount } => {
                        let _ = controller.place_strap_bet(strap, amount).await;
                        force_refresh = true;
                    }
                    ui::UserEvent::Roll => {
                        let _ = controller.roll().await;
                        force_refresh = true;
                    }
                    ui::UserEvent::VRFInc => {
                        controller.inc_vrf();
                        let _ = controller.set_vrf_number(controller.vrf_number).await;
                        force_refresh = true;
                    }
                    ui::UserEvent::VRFDec => {
                        controller.dec_vrf();
                        let _ = controller.set_vrf_number(controller.vrf_number).await;
                        force_refresh = true;
                    }
                    ui::UserEvent::SetVrf(n) => {
                        let _ = controller.set_vrf_number(n).await;
                        force_refresh = true;
                    }
                    ui::UserEvent::ConfirmClaim { game_id, enabled } => {
                        let _ = controller.claim_game(game_id, enabled).await;
                        force_refresh = true;
                    }
                    ui::UserEvent::OpenShop => { ui::draw(ui_state, &last_snapshot)?; continue; }
                    ui::UserEvent::ConfirmShopPurchase { roll, modifier } => {
                        let _ = controller.purchase_modifier_for(roll, modifier, 1).await;
                        force_refresh = true;
                    }
                    ui::UserEvent::OpenBetModal | ui::UserEvent::OpenClaimModal | ui::UserEvent::OpenVrfModal | ui::UserEvent::Redraw => {
                        // UI-only update; redraw without hitting the chain
                        ui::draw(ui_state, &last_snapshot)?;
                        continue;
                    }
                    _ => {}
                }
                if force_refresh {
                    controller.invalidate_cache();
                    last_snapshot = controller.snapshot(true).await?;
                } else {
                    last_snapshot = controller.snapshot(false).await?;
                }
                ui::draw(ui_state, &last_snapshot)?;
            }
        }
    }
    Ok(())
}
