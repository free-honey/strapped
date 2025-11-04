use crate::{
    deployment::{
        self,
        HistoryStore,
        StoredBet,
        StoredGameHistory,
        StoredModifier,
        StoredRollBets,
        StoredStrap,
        StoredStrapReward,
    },
    indexer_client::{
        AccountData,
        IndexerClient,
    },
    ui,
    wallets,
};
use color_eyre::eyre::{
    Result,
    WrapErr,
    eyre,
};
use fuels::{
    accounts::{
        ViewOnlyAccount,
        wallet::Wallet,
    },
    prelude::{
        AssetId,
        CallParameters,
        ContractId,
        Execution,
        Provider,
        TxPolicies,
        VariableOutputPolicy,
    },
    tx::ContractIdExt,
    types::Identity,
};
use futures::future::try_join_all;
use generated_abi::strap_cost;
use std::{
    collections::{
        HashMap,
        HashSet,
    },
    convert::TryFrom,
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
use tracing::{
    error,
    warn,
};

pub const DEFAULT_TESTNET_RPC_URL: &str = "https://testnet.fuel.network";
pub const DEFAULT_DEVNET_RPC_URL: &str = "https://devnet.fuel.network";
pub const DEFAULT_LOCAL_RPC_URL: &str = "http://localhost:4000/";
const STRAPPED_BIN_CANDIDATES: [&str; 1] =
    ["./sway-projects/strapped/out/release/strapped.bin"];
// const STRAPPED_BIN_CANDIDATES: [&str; 1] =
//     ["./sway-projects/strapped/out/debug/strapped.bin"];
const DEFAULT_SAFE_SCRIPT_GAS_LIMIT: u64 = 29_000_000;
const GAME_HISTORY_DEPTH: usize = 10;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VrfMode {
    Fake,
    Pseudo,
}

#[derive(Clone)]
pub enum VrfClient {
    #[allow(dead_code)]
    Fake(fake_vrf::FakeVRFContract<Wallet>),
    Pseudo(pseudo_vrf::PseudoVRFContract<Wallet>),
}

#[derive(Clone, Debug)]
pub struct AppSnapshot {
    pub current_game_id: u32,
    pub roll_history: Vec<strapped::Roll>,
    pub modifier_triggers:
        Vec<(strapped::Roll, strapped::Roll, strapped::Modifier, bool)>,
    pub active_modifiers: Vec<(
        strapped::Roll,
        strapped::Modifier,
        u32, // roll_index
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
    pub alice: strapped::MyContract<Wallet>,
    pub vrf: Option<VrfClient>,
    pub vrf_mode: VrfMode,
    pub contract_id: ContractId,
    pub chip_asset_id: AssetId,
    pub safe_script_gas_limit: u64,
}

#[derive(Clone, Debug)]
pub enum NetworkTarget {
    Testnet { url: String },
    Devnet { url: String },
    LocalNode { url: String },
}

#[derive(Clone, Debug)]
pub enum WalletConfig {
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
    pub indexer_url: Option<String>,
}

async fn get_contract_asset_balance(
    provider: &Provider,
    cid: &ContractId,
    aid: &AssetId,
) -> Result<u64> {
    let bal = provider.get_contract_asset_balance(cid, aid).await?;
    Ok(bal)
}

pub struct AppController {
    pub clients: Clients,
    pub selected_roll: strapped::Roll,
    pub vrf_number: u64,
    pub status: String,
    indexer: Option<IndexerClient>,
    alice_identity: Identity,
    last_seen_game_id_alice: Option<u32>,
    shared_last_roll_history: Vec<strapped::Roll>,
    shared_prev_games: Vec<SharedGame>,
    alice_bets_hist: HashMap<u32, Vec<(strapped::Roll, Vec<(strapped::Bet, u64, u32)>)>>,
    alice_claimed: HashSet<u32>,
    prev_alice_bets: Vec<(strapped::Roll, Vec<(strapped::Bet, u64, u32)>)>,
    strap_rewards_by_game: HashMap<u32, Vec<(strapped::Roll, strapped::Strap, u64)>>,
    active_modifiers_by_game:
        HashMap<u32, Vec<(strapped::Roll, strapped::Modifier, u32)>>,
    errors: Vec<String>,
    last_snapshot: Option<AppSnapshot>,
    last_snapshot_time: Option<Instant>,
    history_store: HistoryStore,
}

impl AppController {
    fn from_clients(
        clients: Clients,
        initial_vrf: u64,
        history_store: HistoryStore,
        indexer: Option<IndexerClient>,
    ) -> Self {
        let alice_identity = Identity::Address(clients.alice.account().address().clone());

        Self {
            clients,
            selected_roll: strapped::Roll::Six,
            vrf_number: initial_vrf,
            status: String::from("Ready"),
            indexer,
            alice_identity,
            last_seen_game_id_alice: None,
            shared_last_roll_history: Vec::new(),
            shared_prev_games: Vec::new(),
            alice_bets_hist: HashMap::new(),
            alice_claimed: HashSet::new(),
            prev_alice_bets: Vec::new(),
            strap_rewards_by_game: HashMap::new(),
            active_modifiers_by_game: HashMap::new(),
            errors: Vec::new(),
            last_snapshot: None,
            last_snapshot_time: None,
            history_store,
        }
    }

    fn poll_interval(&self) -> Duration {
        Duration::from_secs(5)
    }

    fn refresh_ttl(&self) -> Duration {
        self.poll_interval()
    }

    fn invalidate_cache(&mut self) {
        self.last_snapshot = None;
        self.last_snapshot_time = None;
    }

    async fn finalize_snapshot(
        &mut self,
        data: SnapshotSourceData,
    ) -> Result<AppSnapshot> {
        let SnapshotSourceData {
            current_block_height,
            next_roll_height,
            current_game_id,
            roll_history,
            strap_rewards,
            modifier_triggers,
            active_modifiers,
            my_bets,
            known_straps,
        } = data;

        let all_rolls = all_rolls();
        self.active_modifiers_by_game
            .insert(current_game_id, active_modifiers.clone());

        self.strap_rewards_by_game
            .entry(current_game_id)
            .or_insert_with(|| strap_rewards.clone());

        let last_seen_opt = self.last_seen_game_id_alice;
        if let Some(prev) = last_seen_opt {
            if current_game_id > prev {
                let alice_bets_prev = self.prev_alice_bets.clone();
                let mut completed_rolls = self.shared_last_roll_history.clone();
                if !completed_rolls
                    .last()
                    .map(|r| matches!(r, strapped::Roll::Seven))
                    .unwrap_or(false)
                {
                    completed_rolls.push(strapped::Roll::Seven);
                }
                let modifiers_for_prev = self
                    .active_modifiers_by_game
                    .get(&prev)
                    .cloned()
                    .unwrap_or_default();
                self.upsert_shared_game(prev, completed_rolls, modifiers_for_prev);
                self.alice_bets_hist.insert(prev, alice_bets_prev.clone());
                if alice_bets_prev.iter().all(|(_, bets)| bets.is_empty()) {
                    self.alice_claimed.insert(prev);
                }
                self.persist_history()?;
                self.shared_prev_games
                    .sort_by(|a, b| b.game_id.cmp(&a.game_id));
                if self.shared_prev_games.len() > GAME_HISTORY_DEPTH {
                    self.shared_prev_games.truncate(GAME_HISTORY_DEPTH);
                }
                self.last_seen_game_id_alice = Some(current_game_id);
            }
        }
        self.last_seen_game_id_alice = Some(current_game_id);

        self.prev_alice_bets = my_bets.clone();

        let me = self.clients.alice.clone();
        let provider = me.account().provider().clone();

        let pot_balance = get_contract_asset_balance(
            &provider,
            &self.clients.contract_id,
            &self.clients.chip_asset_id,
        )
        .await
        .wrap_err("fetching pot balance failed")?;

        let chip_balance = me
            .account()
            .get_asset_balance(&self.clients.chip_asset_id)
            .await
            .wrap_err("fetching wallet chip balance failed")?;

        self.shared_last_roll_history = roll_history.clone();

        let mut cells = Vec::new();
        let mut strap_info_by_asset: HashMap<AssetId, strapped::Strap> = HashMap::new();
        for (asset_id, strap) in &known_straps {
            strap_info_by_asset.insert(*asset_id, strap.clone());
        }
        for (r, bets) in &my_bets {
            let chip_total: u64 = bets
                .iter()
                .filter_map(|(b, amt, _)| match b {
                    strapped::Bet::Chip => Some(*amt),
                    _ => None,
                })
                .sum();
            let mut straps: Vec<(strapped::Strap, u64)> = Vec::new();
            for (b, amt, _) in bets {
                if let strapped::Bet::Strap(s) = b {
                    if let Some((_es, total)) = straps.iter_mut().find(|(es, _)| es == s)
                    {
                        *total += *amt;
                    } else {
                        straps.push((s.clone(), *amt));
                    }
                    Self::track_strap_metadata(
                        &mut strap_info_by_asset,
                        &self.clients.contract_id,
                        s,
                    );
                }
            }
            let strap_total: u64 = straps.iter().map(|(_, n)| *n).sum();
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
                Self::track_strap_metadata(
                    &mut strap_info_by_asset,
                    &self.clients.contract_id,
                    s,
                );
            }
            cells.push(RollCell {
                roll: r.clone(),
                chip_total,
                strap_total,
                straps,
                rewards,
            });
        }

        for (_gid, list) in &self.strap_rewards_by_game {
            for (_r, s, _) in list {
                Self::track_strap_metadata(
                    &mut strap_info_by_asset,
                    &self.clients.contract_id,
                    s,
                );
            }
        }

        let mut ordered_asset_ids: Vec<AssetId> = Vec::new();
        let mut seen_asset_ids = HashSet::new();
        for (asset_id, _) in &known_straps {
            if seen_asset_ids.insert(*asset_id) {
                ordered_asset_ids.push(*asset_id);
            }
        }
        for asset_id in strap_info_by_asset.keys().copied() {
            if seen_asset_ids.insert(asset_id) {
                ordered_asset_ids.push(asset_id);
            }
        }

        let mut asset_balances: HashMap<AssetId, u128> = HashMap::new();
        for asset_id in &ordered_asset_ids {
            let balance = me.account().get_asset_balance(asset_id).await.unwrap_or(0);
            asset_balances.insert(*asset_id, balance);
        }

        let owned_straps = Self::build_owned_straps(
            &ordered_asset_ids,
            &strap_info_by_asset,
            &asset_balances,
        );

        for asset_id in &ordered_asset_ids {
            let balance = asset_balances.get(asset_id).copied().unwrap_or(0);
            if balance > 0 && !strap_info_by_asset.contains_key(asset_id) {
                warn!(
                    ?asset_id,
                    balance, "strap asset balance present without known metadata"
                );
            }
        }

        let mut summaries: Vec<PreviousGameSummary> = Vec::new();
        for sg in &self.shared_prev_games {
            let bets = self
                .alice_bets_hist
                .get(&sg.game_id)
                .cloned()
                .unwrap_or_default();
            let mut summary_cells = Vec::new();
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
                summary_cells.push(RollCell {
                    roll: r.clone(),
                    chip_total,
                    strap_total,
                    straps: Vec::new(),
                    rewards: Vec::new(),
                });
            }
            let claimed = self.alice_claimed.contains(&sg.game_id);
            summaries.push(PreviousGameSummary {
                game_id: sg.game_id,
                cells: summary_cells,
                modifiers: sg.modifiers.clone(),
                rolls: sg.rolls.clone(),
                bets_by_roll: bets,
                claimed,
            });
        }

        let snapshot = AppSnapshot {
            current_game_id,
            roll_history,
            modifier_triggers,
            active_modifiers,
            owned_straps,
            pot_balance,
            chip_balance: chip_balance
                .try_into()
                .expect("naively assuming this will fit into u64"),
            selected_roll: self.selected_roll.clone(),
            vrf_number: self.vrf_number,
            vrf_mode: self.clients.vrf_mode,
            current_block_height,
            next_roll_height,
            status: self.status.clone(),
            cells,
            previous_games: summaries,
            errors: self.errors.iter().rev().take(5).cloned().collect(),
        };

        Ok(snapshot)
    }

    fn track_strap_metadata(
        strap_info: &mut HashMap<AssetId, strapped::Strap>,
        contract_id: &ContractId,
        strap: &strapped::Strap,
    ) {
        let sub = strapped_contract::strap_to_sub_id(strap);
        let asset_id = contract_id.asset_id(&sub);
        strap_info.entry(asset_id).or_insert_with(|| strap.clone());
    }

    fn build_owned_straps(
        ordered_asset_ids: &[AssetId],
        strap_info_by_asset: &HashMap<AssetId, strapped::Strap>,
        asset_balances: &HashMap<AssetId, u128>,
    ) -> Vec<(strapped::Strap, u64)> {
        let mut owned = Vec::new();
        for asset_id in ordered_asset_ids {
            let Some(balance) = asset_balances.get(asset_id) else {
                continue;
            };
            if *balance == 0 {
                continue;
            }
            if let Some(strap) = strap_info_by_asset.get(asset_id) {
                match u64::try_from(*balance) {
                    Ok(amount) => owned.push((strap.clone(), amount)),
                    Err(_) => warn!(
                        ?asset_id,
                        balance,
                        "strap balance exceeds u64 range; omitting from snapshot"
                    ),
                }
            }
        }
        owned
    }

    async fn snapshot_with_indexer(
        &mut self,
        client: IndexerClient,
    ) -> Result<AppSnapshot> {
        let overview = client
            .latest_overview()
            .await?
            .ok_or_else(|| eyre!("indexer returned no overview snapshot"))?;

        let alice_account = client
            .latest_account_snapshot(&self.alice_identity)
            .await?
            .unwrap_or_else(AccountData::empty);

        let known_straps = client.all_known_straps().await?;

        let safe_limit = self.clients.safe_script_gas_limit;
        let me_for_active = self.clients.alice.clone();
        let active_modifiers = me_for_active
            .methods()
            .active_modifiers()
            .with_tx_policies(TxPolicies::default().with_script_gas_limit(safe_limit))
            .simulate(Execution::realistic())
            .await
            .map(|r| r.value)
            .map_err(color_eyre::eyre::Report::from)
            .wrap_err("active_modifiers call failed")?;

        self.backfill_recent_games_indexer(overview.game_id, &client)
            .await?;

        let data = SnapshotSourceData {
            current_block_height: overview.current_block_height,
            next_roll_height: overview.next_roll_height,
            current_game_id: overview.game_id,
            roll_history: overview.rolls.clone(),
            strap_rewards: overview.rewards.clone(),
            modifier_triggers: overview.modifier_shop.clone(),
            active_modifiers,
            my_bets: alice_account.per_roll_bets,
            known_straps,
        };

        self.finalize_snapshot(data).await
    }

    async fn backfill_recent_games_indexer(
        &mut self,
        current_game_id: u32,
        client: &IndexerClient,
    ) -> Result<()> {
        if current_game_id == 0 {
            return Ok(());
        }

        let start = current_game_id.saturating_sub(GAME_HISTORY_DEPTH as u32);
        for game_id in start..current_game_id {
            let needs_game = !self.shared_prev_games.iter().any(|g| g.game_id == game_id);
            let needs_modifiers = !self.active_modifiers_by_game.contains_key(&game_id);
            let needs_rewards = !self.strap_rewards_by_game.contains_key(&game_id);

            if needs_game || needs_modifiers || needs_rewards {
                if let Some(hist) = client.historical_snapshot(game_id).await? {
                    if needs_game && !hist.rolls.is_empty() {
                        self.upsert_shared_game(
                            game_id,
                            hist.rolls.clone(),
                            hist.modifiers.clone(),
                        );
                    }
                    if needs_modifiers {
                        self.active_modifiers_by_game
                            .insert(game_id, hist.modifiers.clone());
                    }
                    if needs_rewards {
                        self.strap_rewards_by_game
                            .insert(game_id, hist.strap_rewards.clone());
                    }
                }
            }

            if !self.alice_bets_hist.contains_key(&game_id) {
                if let Some(account) = client
                    .historical_account_snapshot(&self.alice_identity, game_id)
                    .await?
                {
                    self.alice_bets_hist
                        .insert(game_id, account.per_roll_bets.clone());
                    if account.claimed_rewards.is_some() {
                        self.alice_claimed.insert(game_id);
                    }
                }
            }
        }

        self.shared_prev_games
            .sort_by(|a, b| b.game_id.cmp(&a.game_id));
        if self.shared_prev_games.len() > GAME_HISTORY_DEPTH {
            self.shared_prev_games.truncate(GAME_HISTORY_DEPTH);
        }
        Ok(())
    }

    fn set_status(&mut self, message: impl Into<String>) {
        self.status = message.into();
        self.errors.clear();
    }

    fn load_history_from_disk(&mut self) -> Result<()> {
        let records = self.history_store.load().map_err(|e| eyre!(e))?;
        if records.is_empty() {
            return Ok(());
        }

        self.shared_prev_games.clear();
        self.alice_bets_hist.clear();
        self.alice_claimed.clear();

        for record in records {
            self.apply_stored_history_record(record)?;
        }

        self.shared_prev_games
            .sort_by(|a, b| b.game_id.cmp(&a.game_id));
        if self.shared_prev_games.len() > GAME_HISTORY_DEPTH {
            self.shared_prev_games.truncate(GAME_HISTORY_DEPTH);
        }
        Ok(())
    }

    fn apply_stored_history_record(&mut self, record: StoredGameHistory) -> Result<()> {
        let rolls = record
            .rolls
            .iter()
            .map(|r| roll_from_key(r))
            .collect::<Result<Vec<_>>>()?;
        let modifiers = record
            .modifiers
            .iter()
            .map(|m| {
                Ok((
                    roll_from_key(&m.roll)?,
                    modifier_from_key(&m.modifier)?,
                    m.roll_index,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        let strap_rewards = record
            .strap_rewards
            .iter()
            .map(|sr| {
                Ok((
                    roll_from_key(&sr.roll)?,
                    stored_to_strap(&sr.strap)?,
                    sr.cost,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        let alice_bets = stored_bets_to_runtime(&record.alice_bets)?;

        self.strap_rewards_by_game
            .insert(record.game_id, strap_rewards);
        self.active_modifiers_by_game
            .insert(record.game_id, modifiers.clone());
        self.alice_bets_hist.insert(record.game_id, alice_bets);
        if record.alice_claimed {
            self.alice_claimed.insert(record.game_id);
        }
        self.upsert_shared_game(record.game_id, rolls, modifiers);
        Ok(())
    }

    fn persist_history(&self) -> Result<()> {
        let mut records = Vec::new();
        for shared in self.shared_prev_games.iter().take(GAME_HISTORY_DEPTH) {
            let rolls = shared
                .rolls
                .iter()
                .map(|r| roll_to_key(r).to_string())
                .collect::<Vec<_>>();
            let modifiers = shared
                .modifiers
                .iter()
                .map(|(r, m, idx)| StoredModifier {
                    roll: roll_to_key(r).to_string(),
                    modifier: modifier_to_key(m).to_string(),
                    roll_index: *idx,
                })
                .collect::<Vec<_>>();
            let strap_rewards = self
                .strap_rewards_by_game
                .get(&shared.game_id)
                .cloned()
                .unwrap_or_default()
                .iter()
                .map(|(roll, strap, cost)| StoredStrapReward {
                    roll: roll_to_key(roll).to_string(),
                    strap: strap_to_stored(strap),
                    cost: *cost,
                })
                .collect::<Vec<_>>();

            let alice_bets_vec = self
                .alice_bets_hist
                .get(&shared.game_id)
                .cloned()
                .unwrap_or_else(empty_bets_template);
            let alice_bets = runtime_bets_to_store(alice_bets_vec);
            records.push(StoredGameHistory {
                game_id: shared.game_id,
                rolls,
                modifiers,
                alice_bets,
                strap_rewards,
                alice_claimed: self.alice_claimed.contains(&shared.game_id),
            });
        }
        self.history_store.save(&records).map_err(|e| eyre!(e))
    }

    fn upsert_shared_game(
        &mut self,
        game_id: u32,
        rolls: Vec<strapped::Roll>,
        modifiers: Vec<(strapped::Roll, strapped::Modifier, u32)>,
    ) {
        if let Some(existing) = self
            .shared_prev_games
            .iter_mut()
            .find(|g| g.game_id == game_id)
        {
            existing.rolls = rolls;
            existing.modifiers = modifiers;
        } else {
            self.shared_prev_games.push(SharedGame {
                game_id,
                rolls,
                modifiers,
            });
        }
    }

    pub async fn new(config: AppConfig) -> Result<Self> {
        let AppConfig {
            vrf_mode,
            network,
            wallets,
            indexer_url,
        } = config;
        let indexer_client = indexer_url.map(IndexerClient::new).transpose()?;
        match network {
            NetworkTarget::Devnet { url } => {
                tracing::info!("Connecting to devnet at URL: {url}");
                Self::new_remote(
                    vrf_mode,
                    deployment::DeploymentEnv::Dev,
                    url,
                    wallets,
                    indexer_client.clone(),
                )
                .await
            }
            NetworkTarget::Testnet { url } => {
                tracing::info!("Connecting to testnet at URL: {}", url);
                Self::new_remote(
                    vrf_mode,
                    deployment::DeploymentEnv::Test,
                    url,
                    wallets,
                    indexer_client.clone(),
                )
                .await
            }
            NetworkTarget::LocalNode { url } => {
                tracing::info!("Connecting to local node at URL: {url}");
                Self::new_remote(
                    vrf_mode,
                    deployment::DeploymentEnv::Local,
                    url,
                    wallets,
                    indexer_client,
                )
                .await
            }
        }
    }

    async fn fetch_bets(
        &self,
    ) -> Result<Vec<(strapped::Roll, Vec<(strapped::Bet, u64, u32)>)>> {
        let contract = self.clients.alice.clone();
        let safe_limit = self.clients.safe_script_gas_limit;
        let futures = all_rolls()
            .into_iter()
            .map(move |roll| {
                let contract = contract.clone();
                async move {
                    let bets = contract
                        .methods()
                        .get_my_bets(roll.clone())
                        .with_tx_policies(
                            TxPolicies::default().with_script_gas_limit(safe_limit),
                        )
                        .simulate(Execution::realistic())
                        .await?
                        .value;
                    Ok::<_, color_eyre::eyre::Report>((roll, bets))
                }
            })
            .collect::<Vec<_>>();
        let results = try_join_all(futures).await?;
        Ok(results)
    }

    async fn fetch_bets_for_game(
        &self,
        game_id: u32,
    ) -> Result<Vec<(strapped::Roll, Vec<(strapped::Bet, u64, u32)>)>> {
        let contract = self.clients.alice.clone();
        let safe_limit = self.clients.safe_script_gas_limit;
        let bets = contract
            .methods()
            .get_my_bets_for_game(game_id)
            .with_tx_policies(TxPolicies::default().with_script_gas_limit(safe_limit))
            .simulate(Execution::realistic())
            .await?
            .value;
        Ok(bets)
    }

    async fn backfill_recent_games(&mut self) -> Result<bool> {
        let safe_limit = self.clients.safe_script_gas_limit;
        let current_game_id_u32 = self
            .clients
            .alice
            .methods()
            .current_game_id()
            .with_tx_policies(TxPolicies::default().with_script_gas_limit(safe_limit))
            .simulate(Execution::realistic())
            .await?
            .value;
        if current_game_id_u32 == 0 {
            return Ok(false);
        }

        let mut updated_any = false;
        let start = current_game_id_u32.saturating_sub(GAME_HISTORY_DEPTH as u32);
        for game_id in start..current_game_id_u32 {
            let mut game_updated = false;
            if !self.shared_prev_games.iter().any(|g| g.game_id == game_id) {
                let rolls = self
                    .clients
                    .alice
                    .methods()
                    .roll_history_for_game(game_id)
                    .with_tx_policies(
                        TxPolicies::default().with_script_gas_limit(safe_limit),
                    )
                    .simulate(Execution::realistic())
                    .await?
                    .value;
                if rolls.is_empty() {
                    continue;
                }
                let modifiers = self
                    .clients
                    .alice
                    .methods()
                    .active_modifiers_for_game(game_id)
                    .with_tx_policies(
                        TxPolicies::default().with_script_gas_limit(safe_limit),
                    )
                    .simulate(Execution::realistic())
                    .await?
                    .value;
                self.active_modifiers_by_game
                    .insert(game_id, modifiers.clone());
                self.upsert_shared_game(game_id, rolls, modifiers);
                game_updated = true;
            } else if !self.active_modifiers_by_game.contains_key(&game_id) {
                let modifiers = self
                    .clients
                    .alice
                    .methods()
                    .active_modifiers_for_game(game_id)
                    .with_tx_policies(
                        TxPolicies::default().with_script_gas_limit(safe_limit),
                    )
                    .simulate(Execution::realistic())
                    .await?
                    .value;
                self.active_modifiers_by_game.insert(game_id, modifiers);
                game_updated = true;
            }

            if !self.strap_rewards_by_game.contains_key(&game_id) {
                let strap_rewards = self
                    .clients
                    .alice
                    .methods()
                    .strap_rewards_for_game(game_id)
                    .with_tx_policies(
                        TxPolicies::default().with_script_gas_limit(safe_limit),
                    )
                    .simulate(Execution::realistic())
                    .await?
                    .value;
                if !strap_rewards.is_empty() {
                    self.strap_rewards_by_game.insert(game_id, strap_rewards);
                    game_updated = true;
                }
            }

            if !self.alice_bets_hist.contains_key(&game_id) {
                let alice_bets = self.fetch_bets_for_game(game_id).await?;
                let alice_claimed = alice_bets.iter().all(|(_, bets)| bets.is_empty());
                self.alice_bets_hist.insert(game_id, alice_bets);
                if alice_claimed {
                    self.alice_claimed.insert(game_id);
                }
                game_updated = true;
            }

            if game_updated {
                updated_any = true;
            }
        }

        if updated_any {
            self.shared_prev_games
                .sort_by(|a, b| b.game_id.cmp(&a.game_id));
            if self.shared_prev_games.len() > GAME_HISTORY_DEPTH {
                self.shared_prev_games.truncate(GAME_HISTORY_DEPTH);
            }
        }

        Ok(updated_any)
    }

    pub async fn new_remote(
        vrf_mode: VrfMode,
        env: deployment::DeploymentEnv,
        url: String,
        wallet_config: WalletConfig,
        indexer: Option<IndexerClient>,
    ) -> Result<Self> {
        if matches!(vrf_mode, VrfMode::Fake) {
            return Err(eyre!(
                "Fake VRF mode is only supported in in-memory deployments"
            ));
        }

        tracing::info!("a");
        let provider = Provider::connect(&url)
            .await
            .wrap_err_with(|| format!("Failed to connect to provider at {url}"))?;

        tracing::info!("b");
        let (owner_name, player_name, wallet_dir) = match wallet_config {
            WalletConfig::ForcKeystore { owner, player, dir } => (owner, player, dir),
        };
        let history_profile = format!("owner-{owner_name}-player-{player_name}");

        tracing::info!("c");
        let owner_descriptor = wallets::find_wallet(&wallet_dir, &owner_name)
            .wrap_err("Unable to locate owner wallet")?;
        let owner_wallet = wallets::unlock_wallet(&owner_descriptor, &provider)?;

        tracing::info!("d");
        let alice_wallet = if player_name == owner_name {
            owner_wallet.clone()
        } else {
            let player_descriptor = wallets::find_wallet(&wallet_dir, &player_name)
                .wrap_err("Unable to locate player wallet")?;
            wallets::unlock_wallet(&player_descriptor, &provider)?
        };

        tracing::info!("e");
        let store = deployment::DeploymentStore::new(env).map_err(|e| eyre!(e))?;
        let history_store =
            deployment::HistoryStore::new(env, Some(history_profile.as_str()))
                .map_err(|e| eyre!(e))?;
        let records = store.load().map_err(|e| eyre!(e))?;
        let strap_binary = choose_binary(&STRAPPED_BIN_CANDIDATES)?;
        let bytecode_hash =
            deployment::compute_bytecode_hash(strap_binary).map_err(|e| eyre!(e))?;

        tracing::info!("f");
        let mut compatible: Vec<_> = records
            .iter()
            .cloned()
            .filter(|record| record.is_compatible_with_hash(&bytecode_hash))
            .collect();

        tracing::info!("g");
        let initial_vrf = match vrf_mode {
            VrfMode::Fake => 19,
            VrfMode::Pseudo => 0,
        };
        let consensus_parameters = provider.consensus_parameters().await?;
        let max_gas_per_tx = consensus_parameters.tx_params().max_gas_per_tx();
        let safe_script_gas_limit = std::cmp::max(
            1,
            std::cmp::min(
                DEFAULT_SAFE_SCRIPT_GAS_LIMIT,
                max_gas_per_tx.saturating_sub(1),
            ),
        );
        tracing::info!(
            "Using safe script gas limit {} (max_gas_per_tx={})",
            safe_script_gas_limit,
            max_gas_per_tx
        );
        tracing::info!("h");
        if compatible.is_empty() {
            let summary =
                format_deployment_summary(env, &url, &store, &records, &bytecode_hash)?;
            return Err(eyre!(summary));
        }

        tracing::info!("i");

        compatible.sort_by(|a, b| a.deployed_at.cmp(&b.deployed_at));
        let selected = compatible
            .last()
            .expect("compatible deployments list should not be empty")
            .clone();

        let trimmed_contract_id_string = &selected.contract_id.trim_start_matches("fuel");
        let contract_id =
            ContractId::from_str(trimmed_contract_id_string).map_err(|e| {
                eyre!(
                    "Deployment record contains an invalid contract id: {e:?}, {:?}",
                    trimmed_contract_id_string
                )
            })?;

        tracing::info!("j");
        let owner_instance =
            strapped::MyContract::new(contract_id.clone(), owner_wallet.clone());
        let alice_instance =
            strapped::MyContract::new(contract_id.clone(), alice_wallet.clone());

        let chip_asset_id = if let Some(id_hex) = selected.chip_asset_id.as_ref() {
            AssetId::from_str(id_hex).map_err(|e| {
                eyre!("Deployment record contains an invalid chip asset id: {e}")
            })?
        } else {
            panic!("Deployment record is missing chip asset id");
        };

        let (vrf_client, _vrf_contract_id) = if let Some(vrf_id) =
            selected.vrf_contract_id.as_ref()
        {
            let vrf_bech32 = ContractId::from_str(vrf_id).map_err(|e| {
                eyre!("Deployment record contains an invalid VRF contract id: {e:?}")
            })?;
            (
                Some(VrfClient::Pseudo(pseudo_vrf::PseudoVRFContract::new(
                    vrf_bech32.clone(),
                    owner_wallet.clone(),
                ))),
                ContractId::from(vrf_bech32),
            )
        } else {
            let vrf_bits = owner_instance
                .methods()
                .current_vrf_contract_id()
                .with_tx_policies(
                    TxPolicies::default().with_script_gas_limit(safe_script_gas_limit),
                )
                .simulate(Execution::realistic())
                .await?
                .value;
            let id = ContractId::new(vrf_bits.0);
            let vrf_client = if vrf_bits.0 == [0u8; 32] {
                None
            } else {
                Some(VrfClient::Pseudo(pseudo_vrf::PseudoVRFContract::new(
                    id,
                    owner_wallet.clone(),
                )))
            };
            (vrf_client, id)
        };
        let clients = Clients {
            alice: alice_instance,
            vrf: vrf_client,
            vrf_mode,
            contract_id,
            chip_asset_id,
            safe_script_gas_limit,
        };

        let mut controller =
            Self::from_clients(clients, initial_vrf, history_store, indexer);
        controller.load_history_from_disk()?;
        let _ = controller.backfill_recent_games().await?;
        controller.persist_history()?;
        Ok(controller)
    }

    pub async fn snapshot(&mut self, force_refresh: bool) -> Result<AppSnapshot> {
        tracing::info!("Taking snapshot (force_refresh={})", force_refresh);
        if !force_refresh {
            if let (Some(last), Some(cache)) =
                (self.last_snapshot_time, self.last_snapshot.clone())
            {
                if last.elapsed() < self.refresh_ttl() {
                    return Ok(cache);
                }
            }
        }

        if let Some(indexer) = self.indexer.clone() {
            match self.snapshot_with_indexer(indexer).await {
                Ok(snapshot) => {
                    self.last_snapshot = Some(snapshot.clone());
                    self.last_snapshot_time = Some(Instant::now());
                    return Ok(snapshot);
                }
                Err(err) => {
                    self.push_errors(vec![format!("Indexer snapshot failed: {err:?}")]);
                }
            }
        }

        let me = self.clients.alice.clone();
        let provider = me.account().provider().clone();
        let safe_limit = self.clients.safe_script_gas_limit;

        let provider_for_height = provider.clone();
        let me_for_game = me.clone();
        let me_for_history = me.clone();
        let me_for_rewards = me.clone();
        let me_for_modifiers = me.clone();
        let me_for_height = me.clone();
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
                let res = me_for_height
                    .methods()
                    .next_roll_height()
                    .with_tx_policies(
                        TxPolicies::default().with_script_gas_limit(safe_limit),
                    )
                    .simulate(Execution::realistic())
                    .await
                    .map(|r| r.value)
                    .map_err(color_eyre::eyre::Report::from)
                    .wrap_err("next_roll_height call failed");
                if let Err(ref e) = res {
                    error!(error = %e, "next_roll_height simulate failed");
                }
                res.wrap_err("next_roll_height call failed")
            },
            async move {
                let res = me_for_game
                    .methods()
                    .current_game_id()
                    .with_tx_policies(
                        TxPolicies::default().with_script_gas_limit(safe_limit),
                    )
                    .simulate(Execution::realistic())
                    .await
                    .map(|r| r.value)
                    .map_err(color_eyre::eyre::Report::from)
                    .wrap_err(format!(
                        "current_game_id call failed with gas limit: {safe_limit:?}"
                    ));
                if let Err(ref e) = res {
                    error!(error = %e, "current_game_id simulate failed");
                }
                res
            },
            async move {
                let res = me_for_history
                    .methods()
                    .roll_history()
                    .with_tx_policies(
                        TxPolicies::default().with_script_gas_limit(safe_limit),
                    )
                    .simulate(Execution::realistic())
                    .await
                    .map(|r| r.value)
                    .map_err(color_eyre::eyre::Report::from);
                if let Err(ref e) = res {
                    error!(error = %e, "roll_history simulate failed");
                }
                res.wrap_err(format!(
                    "roll_history call failed with gas limit: {safe_limit:?}"
                ))
            },
            async move {
                let res = me_for_rewards
                    .methods()
                    .strap_rewards()
                    .with_tx_policies(
                        TxPolicies::default().with_script_gas_limit(safe_limit),
                    )
                    .simulate(Execution::realistic())
                    .await
                    .map(|r| r.value)
                    .map_err(color_eyre::eyre::Report::from);
                if let Err(ref e) = res {
                    error!(error = %e, "strap_rewards simulate failed");
                }
                res.wrap_err(format!(
                    "strap_rewards call failed with gas limit: {safe_limit:?}"
                ))
            },
            async move {
                let res = me_for_modifiers
                    .methods()
                    .modifier_triggers()
                    .with_tx_policies(
                        TxPolicies::default().with_script_gas_limit(safe_limit),
                    )
                    .simulate(Execution::realistic())
                    .await
                    .map(|r| r.value)
                    .map_err(color_eyre::eyre::Report::from)
                    .wrap_err("modifier_triggers call failed");
                if let Err(ref e) = res {
                    error!(error = %e, "modifier_triggers simulate failed");
                }
                res.wrap_err("modifier_triggers call failed")
            },
            async move {
                let res = me_for_active
                    .methods()
                    .active_modifiers()
                    .with_tx_policies(
                        TxPolicies::default().with_script_gas_limit(safe_limit),
                    )
                    .simulate(Execution::realistic())
                    .await
                    .map(|r| r.value)
                    .map_err(color_eyre::eyre::Report::from)
                    .wrap_err("active_modifiers call failed");
                if let Err(ref e) = res {
                    error!(error = %e, "active_modifiers simulate failed");
                }
                res.wrap_err("active_modifiers call failed")
            }
        )?;

        let my_bets = self
            .fetch_bets()
            .await
            .wrap_err("fetching bets for active wallet failed")?;

        let data = SnapshotSourceData {
            current_block_height,
            next_roll_height,
            current_game_id,
            roll_history,
            strap_rewards,
            modifier_triggers,
            active_modifiers,
            my_bets,
            known_straps: Vec::new(),
        };

        let snapshot = self.finalize_snapshot(data).await?;
        self.last_snapshot = Some(snapshot.clone());
        self.last_snapshot_time = Some(Instant::now());
        Ok(snapshot)
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
        let me = self.clients.alice.clone();
        let call = CallParameters::new(
            amount,
            self.clients.chip_asset_id,
            self.clients.safe_script_gas_limit,
        );
        me.methods()
            .place_bet(self.selected_roll.clone(), strapped::Bet::Chip, amount)
            .with_variable_output_policy(VariableOutputPolicy::EstimateMinimum)
            .call_params(call)?
            .with_tx_policies(self.script_policies())
            .call()
            .await?;
        self.set_status(format!(
            "Placed {} chip(s) on {:?}",
            amount, self.selected_roll
        ));
        self.invalidate_cache();
        Ok(())
    }

    pub async fn place_strap_bet(
        &mut self,
        strap: strapped::Strap,
        amount: u64,
    ) -> Result<()> {
        let me = self.clients.alice.clone();
        let sub = strapped_contract::strap_to_sub_id(&strap);
        let asset_id = self.clients.contract_id.asset_id(&sub);
        let call =
            CallParameters::new(amount, asset_id, self.clients.safe_script_gas_limit);
        me.methods()
            .place_bet(
                self.selected_roll.clone(),
                strapped::Bet::Strap(strap.clone()),
                amount,
            )
            // .with_variable_output_policy(VariableOutputPolicy::EstimateMinimum)
            .call_params(call)?
            .with_tx_policies(self.script_policies())
            .call()
            .await?;
        self.set_status(format!(
            "Placed {} of {} on {:?}",
            amount,
            super_compact_strap(&strap),
            self.selected_roll
        ));
        self.invalidate_cache();
        Ok(())
    }

    pub async fn purchase_triggered_modifier(&mut self, cost: u64) -> Result<()> {
        // Find a triggered modifier that targets the selected roll
        let me = self.clients.alice.clone();
        let triggers = me
            .methods()
            .modifier_triggers()
            .with_tx_policies(self.script_policies())
            .simulate(Execution::realistic())
            .await?
            .value;
        if let Some((_, target, modifier, _triggered)) = triggers
            .into_iter()
            .find(|(_, target, _, triggered)| *target == self.selected_roll && *triggered)
        {
            let call = CallParameters::new(
                cost,
                self.clients.chip_asset_id,
                self.clients.safe_script_gas_limit,
            );
            me.methods()
                .purchase_modifier(target.clone(), modifier.clone())
                .call_params(call)?
                .with_tx_policies(self.script_policies())
                .call()
                .await?;
            self.set_status(format!("Purchased {:?} for {:?}", modifier, target));
        } else {
            self.set_status("No triggered modifier for selected roll");
        }
        self.invalidate_cache();
        Ok(())
    }

    pub async fn set_vrf_number(&mut self, n: u64) -> Result<()> {
        match &self.clients.vrf {
            Some(VrfClient::Fake(vrf)) => {
                vrf.methods().set_number(n).call().await?;
                self.vrf_number = n;
                self.set_status(format!("VRF set to {}", n));
            }
            Some(VrfClient::Pseudo(_)) => {
                self.set_status("Pseudo VRF mode does not support manual adjustment");
            }
            None => {
                self.set_status("VRF controls are unavailable on this network");
            }
        }
        self.invalidate_cache();
        Ok(())
    }

    pub async fn roll(&mut self) -> Result<()> {
        // advance chain to next roll height
        let next_roll_height = self
            .clients
            .alice
            .methods()
            .next_roll_height()
            .with_tx_policies(self.script_policies())
            .simulate(Execution::realistic())
            .await
            .wrap_err(format!(
                "with gas limit: {}",
                self.clients.safe_script_gas_limit
            ))?
            .value
            .ok_or_else(|| eyre!("Next roll height not scheduled"))?;
        let provider = self.clients.alice.account().provider().clone();
        let current_height = provider
            .latest_block_height()
            .await
            .wrap_err("Failed to fetch latest block height")?;

        if current_height < next_roll_height {
            self.set_status(format!(
                "Waiting for block {} (current height {}) before rolling",
                next_roll_height, current_height
            ));
            return Ok(());
        }
        // Roll using owner instance but allow any wallet to trigger.
        match &self.clients.vrf {
            Some(VrfClient::Fake(vrf)) => {
                tracing::info!(
                    "Rolling (fake VRF) with script gas limit {}",
                    self.clients.safe_script_gas_limit
                );
                self.clients
                    .alice
                    .methods()
                    .roll_dice()
                    .with_contracts(&[vrf])
                    .with_tx_policies(self.script_policies())
                    .call()
                    .await?;
            }
            Some(VrfClient::Pseudo(vrf)) => {
                tracing::info!(
                    "Rolling (pseudo VRF) with script gas limit {}",
                    self.clients.safe_script_gas_limit
                );
                self.clients
                    .alice
                    .methods()
                    .roll_dice()
                    .with_contracts(&[vrf])
                    .with_tx_policies(self.script_policies())
                    .call()
                    .await?;
            }
            None => {
                self.set_status("VRF contract unavailable; cannot roll");
                return Ok(());
            }
        }
        self.invalidate_cache();
        Ok(())
    }

    pub async fn claim_game(
        &mut self,
        game_id: u32,
        enabled: Vec<(strapped::Roll, strapped::Modifier)>,
    ) -> Result<()> {
        let me = self.clients.alice.clone();
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
            pre_straps.push((
                strap.clone(),
                bal.try_into()
                    .expect("naively assuming this will fit into u64"),
            ));
        }

        let mut claimed_ok = false;
        match me
            .methods()
            .claim_rewards(game_id, enabled.clone())
            .with_variable_output_policy(VariableOutputPolicy::EstimateMinimum)
            .with_tx_policies(self.script_policies())
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
                    let cost = strap_cost(strap);
                    entry.push((roll.clone(), strap.clone(), cost));
                }
            }
        }
        // mark as claimed in local cache for the current user
        self.alice_claimed.insert(game_id);
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
            let d = post.saturating_sub(pre as u128);
            if d > 0 {
                strap_deltas.push(format!("{} x{}", super_compact_strap(&s), d));
            }
        }
        let strap_part = if strap_deltas.is_empty() {
            String::from("")
        } else {
            format!(" | Straps: {}", strap_deltas.join(" "))
        };
        self.set_status(format!(
            "Claimed game {} | Chips +{}{}",
            game_id, chip_delta, strap_part
        ));
        self.push_errors(errs);
        self.persist_history()?;
        self.invalidate_cache();
        Ok(())
    }

    fn expected_upgraded_straps(
        &self,
        game_id: u32,
        enabled: &[(strapped::Roll, strapped::Modifier)],
    ) -> Vec<(strapped::Roll, strapped::Strap)> {
        let bets_hist = match self.alice_bets_hist.get(&game_id) {
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
                    if *bet_roll_index <= idx as u32
                        && let strapped::Bet::Strap(strap) = bet
                    {
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

        upgrades
    }

    fn modifier_override_for_roll(
        active: &[(strapped::Roll, strapped::Modifier, u32)],
        roll: &strapped::Roll,
        bet_roll_index: u32,
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
        let me = self.clients.alice.clone();
        let call = CallParameters::new(
            cost,
            self.clients.chip_asset_id,
            self.clients.safe_script_gas_limit,
        );
        me.methods()
            .purchase_modifier(target.clone(), modifier.clone())
            .call_params(call)?
            .with_tx_policies(self.script_policies())
            .call()
            .await?;
        self.set_status(format!("Purchased {:?} for {:?}", modifier, target));
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
    fn script_policies(&self) -> TxPolicies {
        TxPolicies::default().with_script_gas_limit(self.clients.safe_script_gas_limit)
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

fn empty_bets_template() -> Vec<(strapped::Roll, Vec<(strapped::Bet, u64, u32)>)> {
    all_rolls().into_iter().map(|r| (r, Vec::new())).collect()
}

fn roll_to_key(roll: &strapped::Roll) -> &'static str {
    match roll {
        strapped::Roll::Two => "Two",
        strapped::Roll::Three => "Three",
        strapped::Roll::Four => "Four",
        strapped::Roll::Five => "Five",
        strapped::Roll::Six => "Six",
        strapped::Roll::Seven => "Seven",
        strapped::Roll::Eight => "Eight",
        strapped::Roll::Nine => "Nine",
        strapped::Roll::Ten => "Ten",
        strapped::Roll::Eleven => "Eleven",
        strapped::Roll::Twelve => "Twelve",
    }
}

fn roll_from_key(key: &str) -> Result<strapped::Roll> {
    match key {
        "Two" => Ok(strapped::Roll::Two),
        "Three" => Ok(strapped::Roll::Three),
        "Four" => Ok(strapped::Roll::Four),
        "Five" => Ok(strapped::Roll::Five),
        "Six" => Ok(strapped::Roll::Six),
        "Seven" => Ok(strapped::Roll::Seven),
        "Eight" => Ok(strapped::Roll::Eight),
        "Nine" => Ok(strapped::Roll::Nine),
        "Ten" => Ok(strapped::Roll::Ten),
        "Eleven" => Ok(strapped::Roll::Eleven),
        "Twelve" => Ok(strapped::Roll::Twelve),
        other => Err(eyre!("Unknown roll variant: {other}")),
    }
}

fn kind_to_key(kind: &strapped::StrapKind) -> &'static str {
    match kind {
        strapped::StrapKind::Shirt => "Shirt",
        strapped::StrapKind::Pants => "Pants",
        strapped::StrapKind::Shoes => "Shoes",
        strapped::StrapKind::Dress => "Dress",
        strapped::StrapKind::Hat => "Hat",
        strapped::StrapKind::Glasses => "Glasses",
        strapped::StrapKind::Watch => "Watch",
        strapped::StrapKind::Ring => "Ring",
        strapped::StrapKind::Necklace => "Necklace",
        strapped::StrapKind::Earring => "Earring",
        strapped::StrapKind::Bracelet => "Bracelet",
        strapped::StrapKind::Tattoo => "Tattoo",
        strapped::StrapKind::Skirt => "Skirt",
        strapped::StrapKind::Piercing => "Piercing",
        strapped::StrapKind::Coat => "Coat",
        strapped::StrapKind::Scarf => "Scarf",
        strapped::StrapKind::Gloves => "Gloves",
        strapped::StrapKind::Gown => "Gown",
        strapped::StrapKind::Belt => "Belt",
    }
}

fn kind_from_key(key: &str) -> Result<strapped::StrapKind> {
    match key {
        "Shirt" => Ok(strapped::StrapKind::Shirt),
        "Pants" => Ok(strapped::StrapKind::Pants),
        "Shoes" => Ok(strapped::StrapKind::Shoes),
        "Dress" => Ok(strapped::StrapKind::Dress),
        "Hat" => Ok(strapped::StrapKind::Hat),
        "Glasses" => Ok(strapped::StrapKind::Glasses),
        "Watch" => Ok(strapped::StrapKind::Watch),
        "Ring" => Ok(strapped::StrapKind::Ring),
        "Necklace" => Ok(strapped::StrapKind::Necklace),
        "Earring" => Ok(strapped::StrapKind::Earring),
        "Bracelet" => Ok(strapped::StrapKind::Bracelet),
        "Tattoo" => Ok(strapped::StrapKind::Tattoo),
        "Skirt" => Ok(strapped::StrapKind::Skirt),
        "Piercing" => Ok(strapped::StrapKind::Piercing),
        "Coat" => Ok(strapped::StrapKind::Coat),
        "Scarf" => Ok(strapped::StrapKind::Scarf),
        "Gloves" => Ok(strapped::StrapKind::Gloves),
        "Gown" => Ok(strapped::StrapKind::Gown),
        "Belt" => Ok(strapped::StrapKind::Belt),
        other => Err(eyre!("Unknown strap kind: {other}")),
    }
}

fn modifier_to_key(modifier: &strapped::Modifier) -> &'static str {
    match modifier {
        strapped::Modifier::Nothing => "Nothing",
        strapped::Modifier::Burnt => "Burnt",
        strapped::Modifier::Lucky => "Lucky",
        strapped::Modifier::Holy => "Holy",
        strapped::Modifier::Holey => "Holey",
        strapped::Modifier::Scotch => "Scotch",
        strapped::Modifier::Soaked => "Soaked",
        strapped::Modifier::Moldy => "Moldy",
        strapped::Modifier::Starched => "Starched",
        strapped::Modifier::Evil => "Evil",
        strapped::Modifier::Groovy => "Groovy",
        strapped::Modifier::Delicate => "Delicate",
    }
}

fn modifier_from_key(key: &str) -> Result<strapped::Modifier> {
    match key {
        "Nothing" => Ok(strapped::Modifier::Nothing),
        "Burnt" => Ok(strapped::Modifier::Burnt),
        "Lucky" => Ok(strapped::Modifier::Lucky),
        "Holy" => Ok(strapped::Modifier::Holy),
        "Holey" => Ok(strapped::Modifier::Holey),
        "Scotch" => Ok(strapped::Modifier::Scotch),
        "Soaked" => Ok(strapped::Modifier::Soaked),
        "Moldy" => Ok(strapped::Modifier::Moldy),
        "Starched" => Ok(strapped::Modifier::Starched),
        "Evil" => Ok(strapped::Modifier::Evil),
        "Groovy" => Ok(strapped::Modifier::Groovy),
        "Delicate" => Ok(strapped::Modifier::Delicate),
        other => Err(eyre!("Unknown modifier: {other}")),
    }
}

fn strap_to_stored(strap: &strapped::Strap) -> StoredStrap {
    StoredStrap {
        level: strap.level,
        kind: kind_to_key(&strap.kind).to_string(),
        modifier: modifier_to_key(&strap.modifier).to_string(),
    }
}

fn stored_to_strap(stored: &StoredStrap) -> Result<strapped::Strap> {
    Ok(strapped::Strap {
        level: stored.level,
        kind: kind_from_key(&stored.kind)?,
        modifier: modifier_from_key(&stored.modifier)?,
    })
}

fn runtime_bets_to_store(
    bets: Vec<(strapped::Roll, Vec<(strapped::Bet, u64, u32)>)>,
) -> Vec<StoredRollBets> {
    let mut stored = Vec::new();
    for roll in all_rolls() {
        let entries = bets
            .iter()
            .find(|(r, _)| r == &roll)
            .map(|(_, list)| list.clone())
            .unwrap_or_default();
        let bets = entries
            .into_iter()
            .map(|(bet, amount, roll_index)| {
                let (bet_type, strap) = match &bet {
                    strapped::Bet::Chip => ("Chip".to_string(), None),
                    strapped::Bet::Strap(strap) => {
                        ("Strap".to_string(), Some(strap_to_stored(strap)))
                    }
                };
                StoredBet {
                    bet_type,
                    amount,
                    roll_index,
                    strap,
                }
            })
            .collect::<Vec<_>>();
        stored.push(StoredRollBets {
            roll: roll_to_key(&roll).to_string(),
            bets,
        });
    }
    stored
}

fn stored_bets_to_runtime(
    entries: &[StoredRollBets],
) -> Result<Vec<(strapped::Roll, Vec<(strapped::Bet, u64, u32)>)>> {
    let mut result = Vec::new();
    for roll in all_rolls() {
        let bets = entries
            .iter()
            .find(|entry| entry.roll == roll_to_key(&roll))
            .map(|entry| {
                entry
                    .bets
                    .iter()
                    .map(|stored| {
                        let bet = match stored.bet_type.as_str() {
                            "Chip" => strapped::Bet::Chip,
                            "Strap" => {
                                let strap = stored.strap.as_ref().ok_or_else(|| {
                                    eyre!("Stored strap bet missing strap details")
                                })?;
                                strapped::Bet::Strap(stored_to_strap(strap)?)
                            }
                            other => {
                                return Err(eyre!("Unknown bet type: {other}"));
                            }
                        };
                        Ok((bet, stored.amount, stored.roll_index))
                    })
                    .collect::<Result<Vec<_>>>()
            })
            .transpose()?
            .unwrap_or_default();
        result.push((roll, bets));
    }
    Ok(result)
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

#[allow(clippy::complexity)]
#[derive(Clone, Debug)]
pub struct PreviousGameSummary {
    pub game_id: u32,
    pub cells: Vec<RollCell>,
    pub modifiers: Vec<(strapped::Roll, strapped::Modifier, u32)>,
    pub rolls: Vec<strapped::Roll>,
    pub bets_by_roll: Vec<(strapped::Roll, Vec<(strapped::Bet, u64, u32)>)>,
    pub claimed: bool,
}

#[derive(Clone, Debug)]
struct SharedGame {
    game_id: u32,
    rolls: Vec<strapped::Roll>,
    modifiers: Vec<(strapped::Roll, strapped::Modifier, u32)>,
}

struct SnapshotSourceData {
    current_block_height: u32,
    next_roll_height: Option<u32>,
    current_game_id: u32,
    roll_history: Vec<strapped::Roll>,
    strap_rewards: Vec<(strapped::Roll, strapped::Strap, u64)>,
    modifier_triggers: Vec<(strapped::Roll, strapped::Roll, strapped::Modifier, bool)>,
    active_modifiers: Vec<(strapped::Roll, strapped::Modifier, u32)>,
    my_bets: Vec<(strapped::Roll, Vec<(strapped::Bet, u64, u32)>)>,
    known_straps: Vec<(AssetId, strapped::Strap)>,
}

#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]
    use super::*;
    use std::collections::HashMap;
    use strapped_contract::strapped_types as strapped;

    #[test]
    fn build_owned_straps__returns_owned_assets_with_positive_balances() {
        // given
        let strap = strapped::Strap {
            level: 1,
            kind: strapped::StrapKind::Hat,
            modifier: strapped::Modifier::Lucky,
        };
        let asset_id = AssetId::from([1u8; 32]);
        let mut strap_info = HashMap::new();
        strap_info.insert(asset_id, strap.clone());
        let mut balances = HashMap::new();
        balances.insert(asset_id, 3u128);
        let ordered_ids = vec![asset_id];

        // when
        let owned =
            AppController::build_owned_straps(&ordered_ids, &strap_info, &balances);

        // then
        assert_eq!(owned, vec![(strap, 3)]);
    }

    #[test]
    fn build_owned_straps__skips_unknown_or_zero_balance_assets() {
        // given
        let known_strap = strapped::Strap {
            level: 2,
            kind: strapped::StrapKind::Scarf,
            modifier: strapped::Modifier::Holy,
        };
        let known_asset = AssetId::from([2u8; 32]);
        let unknown_asset = AssetId::from([3u8; 32]);
        let mut strap_info = HashMap::new();
        strap_info.insert(known_asset, known_strap.clone());
        let mut balances = HashMap::new();
        balances.insert(known_asset, 5u128);
        balances.insert(unknown_asset, 7u128);
        balances.insert(AssetId::from([4u8; 32]), 0u128);
        let ordered_ids = vec![known_asset, unknown_asset, AssetId::from([4u8; 32])];

        // when
        let owned =
            AppController::build_owned_straps(&ordered_ids, &strap_info, &balances);

        // then
        assert_eq!(owned, vec![(known_strap, 5)]);
    }
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

fn choose_binary<'a>(paths: &'a [&str]) -> Result<&'a str> {
    paths
        .iter()
        .find(|p| Path::new(p).exists())
        .copied()
        .ok_or_else(|| eyre!("Contract binary not found. Tried {:?}", paths))
}

pub async fn run_app(config: AppConfig) -> Result<()> {
    let mut controller = AppController::new(config).await?;
    let mut ui_state = ui::UiState::default();

    tracing::info!("Starting UI");
    // UI bootstrap
    ui::terminal_enter(&mut ui_state)?;
    tracing::info!("UI ready");
    let res = run_loop(&mut controller, &mut ui_state).await;
    ui::terminal_exit()?;
    res
}

fn sync_status(
    controller: &mut AppController,
    snapshot: &mut AppSnapshot,
    status: impl Into<String>,
) {
    let status = status.into();
    controller.set_status(status);
    if let Some(cache) = controller.last_snapshot.as_mut() {
        cache.status = controller.status.clone();
        cache.errors = controller.errors.clone();
    }
    snapshot.status = controller.status.clone();
    snapshot.errors = controller.errors.clone();
}

fn process_post_action(
    controller: &mut AppController,
    snapshot: &mut AppSnapshot,
    pending: &mut Option<PostAction>,
) {
    let Some(PostAction::Roll {
        prev_len,
        prev_last,
        prev_game_id,
    }) = pending.as_ref()
    else {
        return;
    };

    let prev_len = *prev_len;
    let prev_game_id = *prev_game_id;
    let prev_last = prev_last.clone();

    let new_len = snapshot.roll_history.len();
    let new_last = snapshot.roll_history.last().cloned();
    let new_game_id = snapshot.current_game_id;

    let new_game_started = new_game_id > prev_game_id;

    if new_game_started {
        let message = format!(
            "Rolled a Seven! Board was cleared! Starting Game {}!",
            new_game_id
        );
        sync_status(controller, snapshot, message);
        pending.take();
        return;
    }

    if new_len > prev_len || (new_len == prev_len && new_last != prev_last) {
        if let Some(roll) = new_last {
            let article = match roll {
                strapped::Roll::Eight | strapped::Roll::Eleven => "an",
                _ => "a",
            };
            let roll_name = format!("{:?}", roll);
            let message = format!("Rolled {} {}", article, roll_name);
            sync_status(controller, snapshot, message);
            pending.take();
            return;
        }
    }

    if controller.status == "Rolling..." {
        sync_status(
            controller,
            snapshot,
            "Roll submitted; waiting for confirmation...",
        );
    }
    controller.invalidate_cache();
}

fn show_processing_status(
    controller: &mut AppController,
    snapshot: &mut AppSnapshot,
    ui_state: &mut ui::UiState,
    message: impl Into<String>,
    context: &'static str,
) -> Result<()> {
    sync_status(controller, snapshot, message);
    ui::draw(ui_state, snapshot).wrap_err(context)
}

enum PostAction {
    Roll {
        prev_len: usize,
        prev_last: Option<strapped::Roll>,
        prev_game_id: u32,
    },
}

async fn run_loop(
    controller: &mut AppController,
    ui_state: &mut ui::UiState,
) -> Result<()> {
    tracing::info!("Running app loop");
    let mut ticker = time::interval(controller.poll_interval());
    let mut last_snapshot = controller
        .snapshot(true)
        .await
        .wrap_err("initial snapshot failed")?;
    ui::draw(ui_state, &last_snapshot).wrap_err("initial draw failed")?;
    let mut pending_post_action: Option<PostAction> = None;
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => { break; }
            _ = ticker.tick() => {
                last_snapshot = controller
                    .snapshot(false)
                    .await
                    .wrap_err("periodic snapshot failed")?;
                process_post_action(
                    controller,
                    &mut last_snapshot,
                    &mut pending_post_action,
                );
                ui::draw(ui_state, &last_snapshot).wrap_err("periodic draw failed")?;
            }
            ev = ui::next_event(ui_state) => {
                let mut force_refresh = false;
                match ev.wrap_err("UI event polling failed")? {
                    ui::UserEvent::Quit => break,
                    ui::UserEvent::NextRoll => {
                        controller.select_next_roll();
                        if let Some(cache) = controller.last_snapshot.as_mut() {
                            cache.selected_roll = controller.selected_roll.clone();
                        }
                        last_snapshot.selected_roll = controller.selected_roll.clone();
                        ui::draw(ui_state, &last_snapshot)
                            .wrap_err("draw after NextRoll failed")?;
                        continue;
                    }
                    ui::UserEvent::PrevRoll => {
                        controller.select_prev_roll();
                        if let Some(cache) = controller.last_snapshot.as_mut() {
                            cache.selected_roll = controller.selected_roll.clone();
                        }
                        last_snapshot.selected_roll = controller.selected_roll.clone();
                        ui::draw(ui_state, &last_snapshot)
                            .wrap_err("draw after PrevRoll failed")?;
                        continue;
                    }
                    ui::UserEvent::PlaceBetAmount(amount) => {
                        let roll = controller.selected_roll.clone();
                        let chip_label = if amount == 1 { "chip" } else { "chips" };
                        let status_msg = format!(
                            "Placing bet of {} {} on {:?}...",
                            amount, chip_label, roll
                        );
                        show_processing_status(
                            controller,
                            &mut last_snapshot,
                            ui_state,
                            status_msg,
                            "draw while submitting chip bet failed",
                        )?;
                        controller
                            .place_chip_bet(amount)
                            .await
                            .wrap_err_with(|| {
                                format!("placing chip bet of {} failed", amount)
                            })?;
                        force_refresh = true;
                    }
                    ui::UserEvent::Purchase => {
                        let status_msg = format!(
                            "Purchasing triggered modifier for {:?}...",
                            controller.selected_roll
                        );
                        show_processing_status(
                            controller,
                            &mut last_snapshot,
                            ui_state,
                            status_msg,
                            "draw while submitting triggered modifier purchase failed",
                        )?;
                        controller
                            .purchase_triggered_modifier(1)
                            .await
                            .wrap_err("purchasing triggered modifier failed")?;
                        force_refresh = true;
                    }
                    ui::UserEvent::ConfirmStrapBet { strap, amount } => {
                        let strap_label = super_compact_strap(&strap);
                        let status_msg = format!(
                            "Placing {} of {} on {:?}...",
                            amount, strap_label, controller.selected_roll
                        );
                        show_processing_status(
                            controller,
                            &mut last_snapshot,
                            ui_state,
                            status_msg,
                            "draw while submitting strap bet failed",
                        )?;
                        controller
                            .place_strap_bet(strap, amount)
                            .await
                            .wrap_err_with(|| {
                                format!(
                                    "placing strap bet of {} on {} failed",
                                    amount, strap_label
                                )
                            })?;
                        force_refresh = true;
                    }
                    ui::UserEvent::Roll => {
                        let prev_len = last_snapshot.roll_history.len();
                        let prev_last = last_snapshot.roll_history.last().cloned();
                        let prev_game_id = last_snapshot.current_game_id;
                        show_processing_status(
                            controller,
                            &mut last_snapshot,
                            ui_state,
                            "Rolling...",
                            "draw while submitting roll failed",
                        )?;
                        controller.roll().await.wrap_err("roll failed")?;
                        force_refresh = true;
                        pending_post_action = Some(PostAction::Roll {
                            prev_len,
                            prev_last,
                            prev_game_id,
                        });
                    }
                    ui::UserEvent::VRFInc => {
                        controller.inc_vrf();
                        let target = controller.vrf_number;
                        let status_msg = format!("Setting VRF to {}...", target);
                        show_processing_status(
                            controller,
                            &mut last_snapshot,
                            ui_state,
                            status_msg,
                            "draw while submitting VRF increment failed",
                        )?;
                        controller
                            .set_vrf_number(target)
                            .await
                            .wrap_err("setting VRF number (inc) failed")?;
                        force_refresh = true;
                    }
                    ui::UserEvent::VRFDec => {
                        controller.dec_vrf();
                        let target = controller.vrf_number;
                        let status_msg = format!("Setting VRF to {}...", target);
                        show_processing_status(
                            controller,
                            &mut last_snapshot,
                            ui_state,
                            status_msg,
                            "draw while submitting VRF decrement failed",
                        )?;
                        controller
                            .set_vrf_number(target)
                            .await
                            .wrap_err("setting VRF number (dec) failed")?;
                        force_refresh = true;
                    }
                    ui::UserEvent::SetVrf(n) => {
                        let status_msg = format!("Setting VRF to {}...", n);
                        show_processing_status(
                            controller,
                            &mut last_snapshot,
                            ui_state,
                            status_msg,
                            "draw while submitting explicit VRF set failed",
                        )?;
                        controller
                            .set_vrf_number(n)
                            .await
                            .wrap_err_with(|| format!("setting VRF number to {} failed", n))?;
                        force_refresh = true;
                    }
                    ui::UserEvent::ConfirmClaim { game_id, enabled } => {
                        let selection_count = enabled.len();
                        let status_msg = if selection_count == 0 {
                            format!("Claiming rewards for game {}...", game_id)
                        } else {
                            format!(
                                "Claiming rewards for game {} ({} modifier{})...",
                                game_id,
                                selection_count,
                                if selection_count == 1 { "" } else { "s" }
                            )
                        };
                        show_processing_status(
                            controller,
                            &mut last_snapshot,
                            ui_state,
                            status_msg,
                            "draw while submitting claim failed",
                        )?;
                        controller
                            .claim_game(game_id, enabled)
                            .await
                            .wrap_err_with(|| {
                                format!("claiming game {} with modifiers failed", game_id)
                            })?;
                        force_refresh = true;
                    }
                    ui::UserEvent::OpenShop => { ui::draw(ui_state, &last_snapshot).wrap_err("draw after OpenShop failed")?; continue; }
                    ui::UserEvent::OpenStrapInventory => { ui::draw(ui_state, &last_snapshot).wrap_err("draw after OpenStrapInventory failed")?; continue; }
                    ui::UserEvent::ConfirmShopPurchase { roll, modifier } => {
                        let status_msg = format!("Purchasing {:?} for {:?}...", modifier, roll);
                        show_processing_status(
                            controller,
                            &mut last_snapshot,
                            ui_state,
                            status_msg,
                            "draw while submitting shop purchase failed",
                        )?;
                        controller
                            .purchase_modifier_for(roll, modifier, 1)
                            .await
                            .wrap_err("shop purchase failed")?;
                        force_refresh = true;
                    }
                    ui::UserEvent::OpenBetModal | ui::UserEvent::OpenClaimModal | ui::UserEvent::OpenVrfModal | ui::UserEvent::Redraw => {
                        // UI-only update; redraw without hitting the chain
                        ui::draw(ui_state, &last_snapshot)
                            .wrap_err("draw during modal/redraw failed")?;
                        continue;
                    }
                    _ => {}
                }
                if force_refresh {
                    controller.invalidate_cache();
                    last_snapshot = controller
                        .snapshot(true)
                        .await
                        .wrap_err("forced snapshot refresh failed")?;
                } else {
                    last_snapshot = controller
                        .snapshot(false)
                        .await
                        .wrap_err("snapshot refresh failed")?;
                }
                process_post_action(
                    controller,
                    &mut last_snapshot,
                    &mut pending_post_action,
                );
                ui::draw(ui_state, &last_snapshot)
                    .wrap_err("draw after snapshot refresh failed")?;
            }
        }
    }
    Ok(())
}
