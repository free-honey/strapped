use crate::{
    deployment::{
        self,
    },
    indexer_client::{
        AccountData,
        IndexerClient,
        OverviewData,
    },
    ui,
    wallets,
};
use color_eyre::eyre::{
    Result,
    WrapErr,
    anyhow,
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
use generated_abi::{
    strap_cost,
    strapped_types::{
        ClaimRewardsEvent,
        RollEvent,
    },
};
use std::{
    cmp::Ordering,
    collections::{
        HashMap,
        HashSet,
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
use tokio::{
    sync::mpsc,
    time,
};
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
const MAX_OWED_PERCENTAGE: u64 = 5;
const GAME_HISTORY_DEPTH: usize = 10;

type RollBetEntry = (strapped::Bet, u64, u32);
type BetsByRoll = Vec<(strapped::Roll, Vec<RollBetEntry>)>;
type BetsHistory = HashMap<u32, BetsByRoll>;
type ModifierEntries = Vec<(strapped::Roll, strapped::Modifier, u32)>;
type StrapRewards = Vec<(strapped::Roll, strapped::Strap, u64)>;

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
    pub modifier_triggers: Vec<(
        strapped::Roll,
        strapped::Roll,
        strapped::Modifier,
        bool,
        bool,
        u64,
    )>,
    pub active_modifiers: Vec<(
        strapped::Roll,
        strapped::Modifier,
        u32, // roll_index
    )>,
    pub owned_straps: Vec<(strapped::Strap, u64)>,
    pub pot_balance: u64,
    pub chips_owed: u64,
    pub total_chip_bets: u64,
    pub available_bet_capacity: u64,
    pub chip_balance: u64,
    pub chip_asset_id: AssetId,
    pub chip_asset_ticker: Option<String>,
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

#[derive(Clone, Debug)]
struct PendingRoll {
    game_id: u32,
    index: usize,
    roll: strapped::Roll,
}

#[derive(Clone, Debug)]
struct PendingBet {
    game_id: u32,
    roll: strapped::Roll,
    bet: strapped::Bet,
    amount: u64,
    roll_index: Option<u32>,
}

pub struct Clients {
    pub alice: strapped::MyContract<Wallet>,
    pub vrf: Option<VrfClient>,
    pub vrf_mode: VrfMode,
    pub contract_id: ContractId,
    pub chip_asset_id: AssetId,
    pub chip_asset_ticker: Option<String>,
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
    ForcKeystore { owner: String, dir: PathBuf },
}

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub vrf_mode: VrfMode,
    pub network: NetworkTarget,
    pub wallets: WalletConfig,
    pub indexer_url: Option<String>,
}

pub struct AppController {
    pub clients: Clients,
    pub selected_roll: strapped::Roll,
    pub vrf_number: u64,
    pub status: String,
    indexer: Option<IndexerClient>,
    cached_overview: Option<OverviewData>,
    cached_overview_time: Option<Instant>,
    cached_account: Option<AccountData>,
    cached_account_time: Option<Instant>,
    cached_active_modifiers: ModifierEntries,
    cached_active_modifiers_time: Option<Instant>,
    known_straps: Vec<(AssetId, strapped::Strap)>,
    cached_owned_straps: Vec<(strapped::Strap, u64)>,
    cached_chip_balance: Option<u64>,
    alice_identity: Identity,
    last_seen_game_id_alice: Option<u32>,
    shared_last_roll_history: Vec<strapped::Roll>,
    shared_prev_games: Vec<SharedGame>,
    alice_bets_hist: BetsHistory,
    alice_claimed: HashSet<u32>,
    prev_alice_bets: BetsByRoll,
    strap_rewards_by_game: HashMap<u32, StrapRewards>,
    active_modifiers_by_game: HashMap<u32, ModifierEntries>,
    errors: Vec<String>,
    last_snapshot: Option<AppSnapshot>,
    last_snapshot_time: Option<Instant>,
    pending_rolls: Vec<PendingRoll>,
    pending_bets: Vec<PendingBet>,
}

impl AppController {
    fn from_clients(
        clients: Clients,
        initial_vrf: u64,
        indexer: Option<IndexerClient>,
    ) -> Self {
        let alice_identity =
            Identity::Address((*clients.alice.account().address()).into());

        Self {
            clients,
            selected_roll: strapped::Roll::Six,
            vrf_number: initial_vrf,
            status: String::from("Ready"),
            indexer,
            cached_overview: None,
            cached_overview_time: None,
            cached_account: None,
            cached_account_time: None,
            cached_active_modifiers: Vec::new(),
            cached_active_modifiers_time: None,
            known_straps: Vec::new(),
            cached_owned_straps: Vec::new(),
            cached_chip_balance: None,
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
            pending_rolls: Vec::new(),
            pending_bets: Vec::new(),
        }
    }

    fn poll_interval(&self) -> Duration {
        Duration::from_millis(500)
    }

    #[allow(dead_code)]
    fn refresh_ttl(&self) -> Duration {
        self.poll_interval()
    }

    fn invalidate_cache(&mut self) {
        self.last_snapshot = None;
        self.last_snapshot_time = None;
    }

    fn finalize_snapshot(&mut self) -> Result<AppSnapshot> {
        let overview = self
            .cached_overview
            .clone()
            .ok_or_else(|| eyre!("no overview snapshot cached"))?;
        let account = self
            .cached_account
            .clone()
            .unwrap_or_else(AccountData::empty);
        let current_block_height = overview.current_block_height;
        let next_roll_height = overview.next_roll_height;
        let current_game_id = overview.game_id;
        let mut roll_history = overview.rolls.clone();
        if roll_history.is_empty()
            && let Some(prev_snapshot) = &self.last_snapshot
            && prev_snapshot.current_game_id == current_game_id
            && !prev_snapshot.roll_history.is_empty()
        {
            roll_history = prev_snapshot.roll_history.clone();
        }
        if roll_history.is_empty()
            && self.last_seen_game_id_alice == Some(current_game_id)
            && !self.shared_last_roll_history.is_empty()
        {
            roll_history = self.shared_last_roll_history.clone();
        }
        let strap_rewards = overview.rewards.clone();
        let modifier_triggers = overview.modifier_shop.clone();
        let active_modifiers = self.cached_active_modifiers.clone();
        let mut my_bets = account.per_roll_bets.clone();
        self.overlay_pending_bets(current_game_id, &mut my_bets);
        let all_rolls = all_rolls();
        self.active_modifiers_by_game
            .insert(current_game_id, active_modifiers.clone());

        self.strap_rewards_by_game
            .entry(current_game_id)
            .or_insert_with(|| strap_rewards.clone());

        let last_seen_opt = self.last_seen_game_id_alice;
        if let Some(prev) = last_seen_opt
            && current_game_id > prev
        {
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
            self.shared_prev_games
                .sort_by(|a, b| b.game_id.cmp(&a.game_id));
            if self.shared_prev_games.len() > GAME_HISTORY_DEPTH {
                self.shared_prev_games.truncate(GAME_HISTORY_DEPTH);
            }
            self.last_seen_game_id_alice = Some(current_game_id);
        }
        self.last_seen_game_id_alice = Some(current_game_id);

        self.prev_alice_bets = my_bets.clone();

        self.shared_last_roll_history = roll_history.clone();

        let mut cells = Vec::new();
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
            }
            cells.push(RollCell {
                roll: r.clone(),
                chip_total,
                strap_total,
                straps,
                rewards,
            });
        }

        let owned_straps = self.cached_owned_straps.clone();
        let chip_balance = self.cached_chip_balance.unwrap_or_default();
        let pot_balance = overview.pot_size;
        let chips_owed = overview.chips_owed;
        let total_chip_bets = overview.total_chip_bets;
        let available_bet_capacity = {
            let safe_pool = pot_balance.saturating_sub(chips_owed);
            let limit = safe_pool.saturating_mul(MAX_OWED_PERCENTAGE) / 100;
            limit.saturating_sub(total_chip_bets)
        };

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
            chips_owed,
            total_chip_bets,
            available_bet_capacity,
            chip_balance,
            chip_asset_id: self.clients.chip_asset_id,
            chip_asset_ticker: self.clients.chip_asset_ticker.clone(),
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

    #[allow(dead_code)]
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
            let needs_game = if game_id + GAME_HISTORY_DEPTH as u32 <= current_game_id {
                false
            } else {
                !self.shared_prev_games.iter().any(|g| g.game_id == game_id)
            };
            let needs_modifiers = !self.active_modifiers_by_game.contains_key(&game_id);
            let needs_rewards = !self.strap_rewards_by_game.contains_key(&game_id);

            if (needs_game || needs_modifiers || needs_rewards)
                && let Some(hist) = client.historical_snapshot(game_id).await?
            {
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

            if !self.alice_bets_hist.contains_key(&game_id)
                && let Some(account) = client
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

        self.shared_prev_games
            .sort_by(|a, b| b.game_id.cmp(&a.game_id));
        if self.shared_prev_games.len() > GAME_HISTORY_DEPTH {
            self.shared_prev_games.truncate(GAME_HISTORY_DEPTH);
        }
        Ok(())
    }

    fn apply_pending_rolls(&mut self, overview: &mut OverviewData) {
        let mut pending = std::mem::take(&mut self.pending_rolls);
        pending.sort_by_key(|p| (p.game_id, p.index));
        let mut still_pending = Vec::new();

        for pending_roll in pending.into_iter() {
            match pending_roll.game_id.cmp(&overview.game_id) {
                Ordering::Less => {
                    // Indexer has already advanced past this game; drop the override.
                }
                Ordering::Greater => {
                    // Hold rolls for future games until the indexer catches up.
                    still_pending.push(pending_roll);
                }
                Ordering::Equal => {
                    if overview.rolls.len() > pending_roll.index {
                        // Indexer already reflects this roll.
                        continue;
                    }
                    if overview.rolls.len() == pending_roll.index {
                        overview.rolls.push(pending_roll.roll.clone());
                        // Keep the pending record so a subsequent stale snapshot can be patched again.
                        still_pending.push(pending_roll);
                    } else {
                        // Indexer snapshot is missing earlier rolls; retain until it catches up.
                        still_pending.push(pending_roll);
                    }
                }
            }
        }

        self.pending_rolls = still_pending;
    }

    fn current_game_id(&self) -> Option<u32> {
        self.cached_overview
            .as_ref()
            .map(|overview| overview.game_id)
            .or(self.last_seen_game_id_alice)
    }

    fn drop_pending_bets_for_past_games(&mut self, current_game_id: u32) {
        self.pending_bets
            .retain(|pending| pending.game_id >= current_game_id);
    }

    fn reconcile_pending_bets_with_account(&mut self) {
        let Some(current_game_id) = self.current_game_id() else {
            self.pending_bets.clear();
            return;
        };
        let Some(account) = self.cached_account.clone() else {
            return;
        };
        let mut account_remaining = account.per_roll_bets.clone();

        let mut remaining = Vec::new();
        for pending in self.pending_bets.drain(..) {
            if pending.game_id < current_game_id {
                continue;
            }
            if pending.game_id > current_game_id {
                remaining.push(pending);
                continue;
            }
            let target = (
                pending.bet.clone(),
                pending.amount,
                pending.roll_index.unwrap_or(0),
            );
            let matched = account_remaining
                .iter_mut()
                .find(|(roll, _)| roll == &pending.roll)
                .and_then(|(_, entries)| {
                    entries
                        .iter()
                        .position(|entry| Self::bet_entry_eq(entry, &target))
                        .map(|pos| entries.remove(pos))
                })
                .is_some();
            if !matched {
                remaining.push(pending);
            }
        }
        self.pending_bets = remaining;
    }

    fn overlay_pending_bets(&self, current_game_id: u32, per_roll_bets: &mut BetsByRoll) {
        for pending in self
            .pending_bets
            .iter()
            .filter(|p| p.game_id == current_game_id)
        {
            let bet_entry = (
                pending.bet.clone(),
                pending.amount,
                pending.roll_index.unwrap_or(0),
            );
            if let Some((_, bets)) = per_roll_bets
                .iter_mut()
                .find(|(roll, _)| roll == &pending.roll)
            {
                bets.push(bet_entry);
            } else {
                per_roll_bets.push((pending.roll.clone(), vec![bet_entry]));
            }
        }
    }

    fn record_new_pending_bets(
        &mut self,
        roll: &strapped::Roll,
        latest_bets: Vec<RollBetEntry>,
    ) {
        let Some(current_game_id) = self.current_game_id() else {
            return;
        };
        let mut unmatched = latest_bets;
        if let Some(account) = &self.cached_account
            && let Some((_, known)) =
                account.per_roll_bets.iter().find(|(r, _)| r == roll)
        {
            Self::remove_known_bets(&mut unmatched, known);
        }

        let pending_known: Vec<RollBetEntry> = self
            .pending_bets
            .iter()
            .filter(|pending| pending.game_id == current_game_id && pending.roll == *roll)
            .map(|pending| {
                (
                    pending.bet.clone(),
                    pending.amount,
                    pending.roll_index.unwrap_or(0),
                )
            })
            .collect();
        Self::remove_known_bets(&mut unmatched, &pending_known);

        for (bet, amount, roll_index) in unmatched {
            self.pending_bets.push(PendingBet {
                game_id: current_game_id,
                roll: roll.clone(),
                bet,
                amount,
                roll_index: Some(roll_index),
            });
        }
    }

    fn remove_known_bets(
        pool: &mut Vec<(strapped::Bet, u64, u32)>,
        known: &[(strapped::Bet, u64, u32)],
    ) {
        for entry in known {
            if let Some(pos) = pool
                .iter()
                .position(|candidate| Self::bet_entry_eq(candidate, entry))
            {
                pool.remove(pos);
            }
        }
    }

    fn bet_entry_eq(
        lhs: &(strapped::Bet, u64, u32),
        rhs: &(strapped::Bet, u64, u32),
    ) -> bool {
        lhs.1 == rhs.1 && lhs.2 == rhs.2 && lhs.0 == rhs.0
    }

    fn set_status(&mut self, message: impl Into<String>) {
        self.status = message.into();
        self.errors.clear();
    }

    fn upsert_shared_game(
        &mut self,
        game_id: u32,
        rolls: Vec<strapped::Roll>,
        modifiers: ModifierEntries,
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
        let (owner_name, wallet_dir) = match wallet_config {
            WalletConfig::ForcKeystore { owner, dir } => (owner, dir),
        };

        tracing::info!("c");
        let user_descriptor = wallets::find_wallet(&wallet_dir, &owner_name)
            .wrap_err("Unable to locate owner wallet")?;
        let user_wallet = wallets::unlock_wallet(&user_descriptor, &provider)?;

        tracing::info!("e");
        let store = deployment::DeploymentStore::new(env).map_err(|e| eyre!(e))?;
        let record = store.load().map_err(|e| eyre!(e))?;
        let strap_binary = choose_binary(&STRAPPED_BIN_CANDIDATES)?;
        let bytecode_hash =
            deployment::compute_bytecode_hash(strap_binary).map_err(|e| eyre!(e))?;

        tracing::info!("f");
        let selected = match record {
            Some(record) if record.is_compatible_with_hash(&bytecode_hash) => record,
            other => {
                let summary = format_deployment_summary(
                    env,
                    &url,
                    &store,
                    other.as_ref(),
                    &bytecode_hash,
                )?;
                return Err(eyre!(summary));
            }
        };

        tracing::info!("g");
        let initial_vrf = match vrf_mode {
            VrfMode::Fake => 19,
            VrfMode::Pseudo => 0,
        };
        let consensus_parameters = provider.consensus_parameters().await?;
        let base_asset_id = *consensus_parameters.base_asset_id();
        let max_gas_per_tx = consensus_parameters.tx_params().max_gas_per_tx();
        let safe_script_gas_limit = max_gas_per_tx
            .saturating_sub(1)
            .clamp(1, DEFAULT_SAFE_SCRIPT_GAS_LIMIT);
        tracing::info!(
            "Using safe script gas limit {} (max_gas_per_tx={})",
            safe_script_gas_limit,
            max_gas_per_tx
        );
        tracing::info!("h");
        tracing::info!("i");

        let trimmed_contract_id_string = &selected.contract_id.trim_start_matches("fuel");
        let contract_id =
            ContractId::from_str(trimmed_contract_id_string).map_err(|e| {
                eyre!(
                    "Deployment record contains an invalid contract id: {e:?}, {:?}",
                    trimmed_contract_id_string
                )
            })?;

        tracing::info!("j");
        let user_instance = strapped::MyContract::new(contract_id, user_wallet.clone());

        let chip_asset_id = if let Some(id_hex) = selected.chip_asset_id.as_ref() {
            AssetId::from_str(id_hex).map_err(|e| {
                eyre!("Deployment record contains an invalid chip asset id: {e}")
            })?
        } else {
            panic!("Deployment record is missing chip asset id");
        };
        let chip_asset_ticker = selected.chip_asset_ticker.clone().or_else(|| {
            if chip_asset_id == base_asset_id {
                Some("Gwei".to_string())
            } else {
                None
            }
        });

        let (vrf_client, _vrf_contract_id) = if let Some(vrf_id) =
            selected.vrf_contract_id.as_ref()
        {
            let vrf_bech32 = ContractId::from_str(vrf_id).map_err(|e| {
                eyre!("Deployment record contains an invalid VRF contract id: {e:?}")
            })?;
            (
                Some(VrfClient::Pseudo(pseudo_vrf::PseudoVRFContract::new(
                    vrf_bech32,
                    user_wallet.clone(),
                ))),
                vrf_bech32,
            )
        } else {
            let vrf_bits = user_instance
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
                    user_wallet.clone(),
                )))
            };
            (vrf_client, id)
        };
        let clients = Clients {
            alice: user_instance,
            vrf: vrf_client,
            vrf_mode,
            contract_id,
            chip_asset_id,
            chip_asset_ticker,
            safe_script_gas_limit,
        };

        let controller = Self::from_clients(clients, initial_vrf, indexer);
        Ok(controller)
    }

    fn ingest_overview_snapshot(
        &mut self,
        mut overview: OverviewData,
    ) -> Result<Vec<u32>> {
        let current_game_id = overview.game_id;
        self.apply_pending_rolls(&mut overview);
        self.cached_overview_time = Some(Instant::now());
        self.cached_overview = Some(overview);
        self.drop_pending_bets_for_past_games(current_game_id);
        Ok(self.pending_history_game_ids(current_game_id))
    }

    fn pending_history_game_ids(&self, current_game_id: u32) -> Vec<u32> {
        if current_game_id == 0 {
            return Vec::new();
        }
        let mut missing = Vec::new();
        for game_id in 0..current_game_id {
            let needs_game = !self.shared_prev_games.iter().any(|g| g.game_id == game_id);
            let needs_modifiers = !self.active_modifiers_by_game.contains_key(&game_id);
            let needs_rewards = !self.strap_rewards_by_game.contains_key(&game_id);
            let needs_bets = !self.alice_bets_hist.contains_key(&game_id);
            if needs_game || needs_modifiers || needs_rewards || needs_bets {
                missing.push(game_id);
            }
        }
        missing
    }

    fn ingest_account_snapshot(&mut self, account: AccountData) {
        self.cached_account_time = Some(Instant::now());
        self.cached_account = Some(account);
        self.reconcile_pending_bets_with_account();
    }

    fn ingest_history_records(&mut self, records: Vec<HistoryRecord>) {
        if records.is_empty() {
            return;
        }
        for record in records {
            self.upsert_shared_game(
                record.game_id,
                record.rolls.clone(),
                record.modifiers.clone(),
            );
            self.active_modifiers_by_game
                .insert(record.game_id, record.modifiers.clone());
            self.strap_rewards_by_game
                .insert(record.game_id, record.strap_rewards.clone());
            self.alice_bets_hist
                .insert(record.game_id, record.per_roll_bets.clone());
            if record.claimed {
                self.alice_claimed.insert(record.game_id);
            }
        }
        self.shared_prev_games
            .sort_by(|a, b| b.game_id.cmp(&a.game_id));
        if self.shared_prev_games.len() > GAME_HISTORY_DEPTH {
            self.shared_prev_games.truncate(GAME_HISTORY_DEPTH);
        }
    }

    async fn refresh_chip_balance(&mut self) -> Result<()> {
        let chip_balance_raw = self
            .clients
            .alice
            .account()
            .get_asset_balance(&self.clients.chip_asset_id)
            .await
            .wrap_err("fetching wallet chip balance failed")?;
        let chip_balance = u64::try_from(chip_balance_raw)
            .map_err(|_| eyre!("chip balance exceeds u64 range"))?;
        self.cached_chip_balance = Some(chip_balance);
        Ok(())
    }

    async fn refresh_active_modifiers_now(&mut self) -> Result<()> {
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
        self.cached_active_modifiers = active_modifiers;
        self.cached_active_modifiers_time = Some(Instant::now());
        Ok(())
    }

    async fn ensure_known_straps(&mut self) -> Result<()> {
        if self.known_straps.is_empty() {
            self.refresh_known_straps().await?;
        }
        Ok(())
    }

    async fn refresh_known_straps(&mut self) -> Result<()> {
        let indexer = self
            .indexer
            .clone()
            .ok_or(anyhow!("No indexer configured"))?;
        self.known_straps = indexer.all_known_straps().await?;
        Ok(())
    }

    async fn refresh_strap_inventory(&mut self) -> Result<()> {
        self.refresh_known_straps().await?;
        let balances = self
            .clients
            .alice
            .account()
            .get_balances()
            .await
            .wrap_err("fetching wallet balances failed")?;

        let mut asset_balances: HashMap<AssetId, u128> = HashMap::new();
        for (raw_id, amount) in balances {
            match AssetId::from_str(&raw_id) {
                Ok(asset_id) => {
                    asset_balances.insert(asset_id, amount);
                }
                Err(err) => {
                    warn!(%raw_id, ?err, "failed to parse asset id from wallet balances");
                }
            }
        }

        if let Some(amount) = asset_balances.get(&self.clients.chip_asset_id).copied() {
            let chip_balance = u64::try_from(amount)
                .map_err(|_| eyre!("chip balance exceeds u64 range"))?;
            self.cached_chip_balance = Some(chip_balance);
        }

        let mut strap_info: HashMap<AssetId, strapped::Strap> = HashMap::new();
        for (asset_id, strap) in &self.known_straps {
            strap_info.insert(*asset_id, strap.clone());
        }

        let ordered_asset_ids: Vec<AssetId> = self
            .known_straps
            .iter()
            .map(|(asset_id, _)| *asset_id)
            .collect();
        let owned_straps =
            Self::build_owned_straps(&ordered_asset_ids, &strap_info, &asset_balances);
        self.cached_owned_straps = owned_straps;
        Ok(())
    }

    fn build_snapshot(&mut self) -> Result<AppSnapshot> {
        let snapshot = self.finalize_snapshot()?;
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
        let target_roll = self.selected_roll.clone();
        let call = CallParameters::new(
            amount,
            self.clients.chip_asset_id,
            self.clients.safe_script_gas_limit,
        );
        me.methods()
            .place_bet(target_roll.clone(), strapped::Bet::Chip, amount)
            .with_variable_output_policy(VariableOutputPolicy::EstimateMinimum)
            .call_params(call)?
            .with_tx_policies(self.script_policies())
            .call()
            .await?;
        let latest_bets = me
            .methods()
            .get_my_bets(target_roll.clone())
            .with_tx_policies(
                TxPolicies::default()
                    .with_script_gas_limit(self.clients.safe_script_gas_limit),
            )
            .simulate(Execution::realistic())
            .await?
            .value;
        self.record_new_pending_bets(&target_roll, latest_bets);
        self.set_status(format!("Placed {} chip(s) on {:?}", amount, target_roll));
        self.invalidate_cache();
        Ok(())
    }

    pub async fn place_strap_bet(
        &mut self,
        strap: strapped::Strap,
        amount: u64,
    ) -> Result<()> {
        let me = self.clients.alice.clone();
        let target_roll = self.selected_roll.clone();
        let sub = strapped_contract::strap_to_sub_id(&strap);
        let asset_id = self.clients.contract_id.asset_id(&sub);
        let call =
            CallParameters::new(amount, asset_id, self.clients.safe_script_gas_limit);
        me.methods()
            .place_bet(
                target_roll.clone(),
                strapped::Bet::Strap(strap.clone()),
                amount,
            )
            // .with_variable_output_policy(VariableOutputPolicy::EstimateMinimum)
            .call_params(call)?
            .with_tx_policies(self.script_policies())
            .call()
            .await?;
        let latest_bets = me
            .methods()
            .get_my_bets(target_roll.clone())
            .with_tx_policies(
                TxPolicies::default()
                    .with_script_gas_limit(self.clients.safe_script_gas_limit),
            )
            .simulate(Execution::realistic())
            .await?
            .value;
        self.record_new_pending_bets(&target_roll, latest_bets);
        self.set_status(format!(
            "Placed {} of {} on {:?}",
            amount,
            super_compact_strap(&strap),
            target_roll
        ));
        self.invalidate_cache();
        Ok(())
    }

    pub async fn purchase_triggered_modifier(
        &mut self,
        target: strapped::Roll,
        modifier: strapped::Modifier,
        snapshot_cost: u64,
    ) -> Result<()> {
        let me = self.clients.alice.clone();
        let amount = snapshot_cost;
        let call = CallParameters::new(
            amount,
            self.clients.chip_asset_id,
            self.clients.safe_script_gas_limit,
        );
        me.methods()
            .purchase_modifier(target.clone(), modifier.clone())
            .call_params(call)?
            .with_tx_policies(self.script_policies())
            .call()
            .await
            .wrap_err_with(|| {
                format!(
                    "purchase_triggered failed (roll {:?}, modifier {:?}, sent {})",
                    target, modifier, amount
                )
            })?;
        self.set_status(format!("Purchased {:?} for {:?}", modifier, target));
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
        let response = match &self.clients.vrf {
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
                    .with_variable_output_policy(VariableOutputPolicy::EstimateMinimum)
                    .call()
                    .await?
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
                    .with_variable_output_policy(VariableOutputPolicy::EstimateMinimum)
                    .call()
                    .await?
            }
            None => {
                self.set_status("VRF contract unavailable; cannot roll");
                return Ok(());
            }
        };

        let events = response.decode_logs_with_type::<RollEvent>()?;
        if let Some(event) = events.first() {
            let rolled_value = event.rolled_value.clone();
            let is_seven = matches!(rolled_value, strapped::Roll::Seven);
            if is_seven {
                self.set_status("Rolled a Seven! Waiting for the next game...");
            } else {
                self.set_status(format!("Rolled a {:?}", &rolled_value));
            }

            let event_game_id = event.game_id;
            let mut pending_index: Option<usize> = None;

            if let Some(overview) = self.cached_overview.as_mut() {
                if overview.game_id == event_game_id {
                    if is_seven {
                        overview.rolls.clear();
                        self.shared_last_roll_history = overview.rolls.clone();
                    } else {
                        pending_index = Some(overview.rolls.len());
                        overview.rolls.push(rolled_value.clone());
                        self.shared_last_roll_history = overview.rolls.clone();
                    }
                } else {
                    self.shared_last_roll_history.clear();
                }
            } else if !is_seven
                && self
                    .last_seen_game_id_alice
                    .map(|prev| prev == event_game_id)
                    .unwrap_or(false)
            {
                pending_index = Some(self.shared_last_roll_history.len());
                self.shared_last_roll_history.push(rolled_value.clone());
            } else {
                self.shared_last_roll_history.clear();
                if !is_seven {
                    pending_index = Some(0);
                    self.shared_last_roll_history.push(rolled_value.clone());
                }
            }

            self.last_seen_game_id_alice = Some(event_game_id);
            if is_seven {
                self.pending_rolls
                    .retain(|pending| pending.game_id > event_game_id);
            } else if let Some(index) = pending_index {
                self.pending_rolls.push(PendingRoll {
                    game_id: event_game_id,
                    index,
                    roll: rolled_value,
                });
            }
        } else {
            tracing::warn!("roll_dice emitted no RollEvent logs");
            self.set_status("no roll event found");
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
        let pre_chip: u128 = me
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
        let mut claim_response = None;
        match me
            .methods()
            .claim_rewards(game_id, enabled.clone())
            .with_variable_output_policy(VariableOutputPolicy::EstimateMinimum)
            .with_tx_policies(self.script_policies())
            .call()
            .await
        {
            Ok(resp) => {
                claimed_ok = true;
                claim_response = Some(resp);
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
            let entry = self.strap_rewards_by_game.entry(game_id).or_default();
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
        let post_chip: u128 = me
            .account()
            .get_asset_balance(&self.clients.chip_asset_id)
            .await
            .unwrap_or(0);
        let balance_chip_delta = post_chip.saturating_sub(pre_chip);
        let balance_chip_delta_u64 =
            u64::try_from(balance_chip_delta).unwrap_or(u64::MAX);
        let event_chip_delta = claim_response.as_ref().and_then(|resp| {
            resp.decode_logs_with_type::<ClaimRewardsEvent>()
                .ok()
                .and_then(|events| {
                    events
                        .iter()
                        .find(|ev| ev.player == self.alice_identity)
                        .map(|ev| ev.total_chips_winnings)
                })
        });
        let claimed_chips = event_chip_delta.unwrap_or(balance_chip_delta_u64);
        let total_chip_bet: u64 = self
            .alice_bets_hist
            .get(&game_id)
            .map(|bets| {
                bets.iter()
                    .flat_map(|(_, bs)| bs.iter())
                    .filter_map(|(b, amt, _)| match b {
                        strapped::Bet::Chip => Some(*amt),
                        _ => None,
                    })
                    .sum()
            })
            .unwrap_or(0);
        let net_chips: i128 = i128::from(claimed_chips) - i128::from(total_chip_bet);
        let net_part = if total_chip_bet > 0 {
            let sign = if net_chips >= 0 { "+" } else { "-" };
            let magnitude = net_chips.unsigned_abs();
            format!(" | Bet {} | Net {}{}", total_chip_bet, sign, magnitude)
        } else {
            String::from("")
        };
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
            "Claimed game {} | Chips {}{}{}",
            game_id, claimed_chips, net_part, strap_part
        ));
        self.push_errors(errs);
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
        active: &ModifierEntries,
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
        snapshot_cost: u64,
    ) -> Result<()> {
        let me = self.clients.alice.clone();
        let amount = snapshot_cost;
        let call = CallParameters::new(
            amount,
            self.clients.chip_asset_id,
            self.clients.safe_script_gas_limit,
        );
        me.methods()
            .purchase_modifier(target.clone(), modifier.clone())
            .call_params(call)?
            .with_tx_policies(self.script_policies())
            .call()
            .await
            .wrap_err_with(|| {
                format!(
                    "purchase_modifier_for failed (roll {:?}, modifier {:?}, sent {})",
                    target, modifier, amount
                )
            })?;
        self.set_status(format!("Purchased {:?} for {:?}", modifier, target));
        self.invalidate_cache();
        Ok(())
    }
}

fn modifier_floor_price(modifier: &strapped::Modifier) -> u64 {
    match modifier {
        strapped::Modifier::Nothing => 0,
        strapped::Modifier::Burnt => 10,
        strapped::Modifier::Lucky => 20,
        strapped::Modifier::Holy => 30,
        strapped::Modifier::Holey => 40,
        strapped::Modifier::Scotch => 50,
        strapped::Modifier::Soaked => 60,
        strapped::Modifier::Moldy => 70,
        strapped::Modifier::Starched => 80,
        strapped::Modifier::Evil => 90,
        strapped::Modifier::Groovy => 100,
        strapped::Modifier::Delicate => 110,
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
    pub modifiers: ModifierEntries,
    pub rolls: Vec<strapped::Roll>,
    pub bets_by_roll: BetsByRoll,
    pub claimed: bool,
}

#[derive(Clone, Debug)]
struct SharedGame {
    game_id: u32,
    rolls: Vec<strapped::Roll>,
    modifiers: ModifierEntries,
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
    record: Option<&deployment::DeploymentRecord>,
    current_hash: &str,
) -> Result<String> {
    let mut message = format!(
        "No compatible deployment recorded for {env} at {url}.\n\nRecorded deployment for {env}:",
    );

    if let Some(record) = record {
        let compat = if record.is_compatible_with_hash(current_hash) {
            " [compatible]"
        } else {
            ""
        };
        let asset_info = record.chip_asset_id.as_deref().unwrap_or("(unknown asset)");
        let asset_details = match record.chip_asset_ticker.as_deref() {
            Some(ticker) => format!("{asset_info} ({ticker})"),
            None => asset_info.to_string(),
        };
        let contract_salt = record.contract_salt.as_deref().unwrap_or("(unknown salt)");
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
            asset_details,
            contract_salt,
            vrf_salt,
            vrf_contract,
            vrf_hash,
        ));
    } else {
        message.push_str("\n  (none recorded)");
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

#[allow(unused)]
pub fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();
}

pub async fn run_app(config: AppConfig) -> Result<()> {
    let controller = AppController::new(config).await?;
    let mut ui_state = ui::UiState::default();
    let mut input_events = ui::input_event_stream();

    tracing::info!("Starting UI");
    // UI bootstrap
    ui::terminal_enter(&mut ui_state)?;
    tracing::info!("UI ready");
    let res = run_loop(controller, &mut ui_state, &mut input_events).await;
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

fn sync_error(
    controller: &mut AppController,
    snapshot: &mut AppSnapshot,
    error_msg: impl Into<String>,
) {
    let error_msg = error_msg.into();
    controller.push_errors(vec![error_msg]);
    if let Some(cache) = controller.last_snapshot.as_mut() {
        cache.errors = controller.errors.clone();
        cache.status = controller.status.clone();
    }
    snapshot.errors = controller.errors.clone();
    snapshot.status = controller.status.clone();
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
            "Rolled a Seven! New Game {} | Pot: {} | Owed: {} | Capacity: {}",
            new_game_id,
            snapshot.pot_balance,
            snapshot.chips_owed,
            snapshot.available_bet_capacity
        );
        sync_status(controller, snapshot, message);
        pending.take();
        return;
    }

    if (new_len > prev_len || (new_len == prev_len && new_last != prev_last))
        && let Some(roll) = new_last
    {
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

#[derive(Clone, Debug)]
struct SnapshotBundle {
    overview: OverviewData,
    account: AccountData,
}

#[derive(Clone, Debug)]
struct HistoryRecord {
    game_id: u32,
    rolls: Vec<strapped::Roll>,
    modifiers: ModifierEntries,
    strap_rewards: StrapRewards,
    per_roll_bets: BetsByRoll,
    claimed: bool,
}

enum SnapshotWorkerCommand {
    FetchNow,
    FetchHistory(Vec<u32>),
    Shutdown,
}

enum SnapshotWorkerEvent {
    Snapshot(SnapshotBundle),
    History(Vec<HistoryRecord>),
}

async fn snapshot_worker(
    poll_interval: Duration,
    indexer: IndexerClient,
    identity: Identity,
    mut cmd_rx: mpsc::UnboundedReceiver<SnapshotWorkerCommand>,
    snapshot_tx: mpsc::UnboundedSender<SnapshotWorkerEvent>,
) -> Result<()> {
    async fn fetch_snapshot(
        indexer: &IndexerClient,
        identity: &Identity,
        snapshot_tx: &mpsc::UnboundedSender<SnapshotWorkerEvent>,
    ) -> Result<()> {
        if let Some(overview) = indexer.latest_overview().await? {
            let account = indexer
                .latest_account_snapshot(identity)
                .await?
                .unwrap_or_else(AccountData::empty);
            snapshot_tx
                .send(SnapshotWorkerEvent::Snapshot(SnapshotBundle {
                    overview,
                    account,
                }))
                .map_err(|_| eyre!("snapshot receiver dropped"))?;
        }
        Ok(())
    }

    async fn fetch_history(
        indexer: &IndexerClient,
        identity: &Identity,
        game_ids: Vec<u32>,
        snapshot_tx: &mpsc::UnboundedSender<SnapshotWorkerEvent>,
    ) -> Result<()> {
        if game_ids.is_empty() {
            return Ok(());
        }
        let mut records = Vec::new();
        for game_id in game_ids {
            let Some(hist) = indexer.historical_snapshot(game_id).await? else {
                continue;
            };
            let account = indexer
                .historical_account_snapshot(identity, game_id)
                .await?
                .unwrap_or_else(AccountData::empty);
            records.push(HistoryRecord {
                game_id,
                rolls: hist.rolls,
                modifiers: hist.modifiers,
                strap_rewards: hist.strap_rewards,
                per_roll_bets: account.per_roll_bets,
                claimed: account.claimed_rewards.is_some(),
            });
        }
        if !records.is_empty() {
            snapshot_tx
                .send(SnapshotWorkerEvent::History(records))
                .map_err(|_| eyre!("snapshot receiver dropped"))?;
        }
        Ok(())
    }

    let mut ticker = time::interval(poll_interval);
    fetch_snapshot(&indexer, &identity, &snapshot_tx).await?;

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                if let Err(err) = fetch_snapshot(&indexer, &identity, &snapshot_tx).await {
                    warn!(?err, "snapshot fetch failed");
                }
            }
            cmd = cmd_rx.recv() => {
                let Some(cmd) = cmd else {
                    break;
                };
                match cmd {
                    SnapshotWorkerCommand::FetchNow => {
                        if let Err(err) = fetch_snapshot(&indexer, &identity, &snapshot_tx).await {
                            warn!(?err, "snapshot fetch failed");
                        }
                    }
                    SnapshotWorkerCommand::FetchHistory(ids) => {
                        if let Err(err) =
                            fetch_history(&indexer, &identity, ids, &snapshot_tx).await
                        {
                            warn!(?err, "historical snapshot fetch failed");
                        }
                    }
                    SnapshotWorkerCommand::Shutdown => break,
                }
            }
        }
    }
    Ok(())
}

async fn run_loop(
    mut controller: AppController,
    ui_state: &mut ui::UiState,
    input_events: &mut ui::InputEventReceiver,
) -> Result<()> {
    tracing::info!("Running app loop");
    controller.ensure_known_straps().await?;
    controller.refresh_strap_inventory().await?;
    controller.refresh_active_modifiers_now().await?;
    let poll_interval = controller.poll_interval();
    let indexer = controller
        .indexer
        .clone()
        .ok_or(anyhow!("No indexer configured"))?;
    let identity = controller.alice_identity;

    let (snapshot_cmd_tx, snapshot_cmd_rx) = mpsc::unbounded_channel();
    let (snapshot_event_tx, mut snapshot_event_rx) = mpsc::unbounded_channel();
    let snapshot_handle = tokio::spawn(snapshot_worker(
        poll_interval,
        indexer,
        identity,
        snapshot_cmd_rx,
        snapshot_event_tx,
    ));
    let _ = snapshot_cmd_tx.send(SnapshotWorkerCommand::FetchNow);

    let mut pending_post_action: Option<PostAction> = None;
    let mut last_snapshot: Option<AppSnapshot> = None;
    let mut snapshot_worker_closed = false;

    loop {
        tokio::select! {
            maybe_event = snapshot_event_rx.recv() => {
                match maybe_event {
                    Some(SnapshotWorkerEvent::Snapshot(bundle)) => {
                        let SnapshotBundle { overview, account } = bundle;
                        let missing_history = controller
                            .ingest_overview_snapshot(overview)
                            .wrap_err("applying overview snapshot failed")?;
                        controller.ingest_account_snapshot(account);
                        if !missing_history.is_empty() {
                            let _ = snapshot_cmd_tx
                                .send(SnapshotWorkerCommand::FetchHistory(missing_history));
                        }
                        let mut snapshot = controller
                            .build_snapshot()
                            .wrap_err("snapshot refresh failed")?;
                        process_post_action(
                            &mut controller,
                            &mut snapshot,
                            &mut pending_post_action,
                        );
                        ui::draw(ui_state, &snapshot)
                            .wrap_err("draw after snapshot refresh failed")?;
                        last_snapshot = Some(snapshot);
                    }
                    Some(SnapshotWorkerEvent::History(records)) => {
                        controller.ingest_history_records(records);
                        if last_snapshot.is_some() {
                            let mut snapshot = controller
                                .build_snapshot()
                                .wrap_err("snapshot refresh after history update failed")?;
                            process_post_action(
                                &mut controller,
                                &mut snapshot,
                                &mut pending_post_action,
                            );
                            ui::draw(ui_state, &snapshot)
                                .wrap_err("draw after history update failed")?;
                            last_snapshot = Some(snapshot);
                        }
                    }
                    None => {
                        tracing::warn!("snapshot worker channel closed");
                        snapshot_worker_closed = true;
                        break;
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                let _ = snapshot_cmd_tx.send(SnapshotWorkerCommand::Shutdown);
                break;
            }
            raw_ev = ui::next_raw_event(input_events) => {
                let event = raw_ev?;
                let Some(ev) = ui::interpret_event(ui_state, event) else {
                    continue;
                };
                if last_snapshot.is_none() {
                    if matches!(ev, ui::UserEvent::Quit) {
                        let _ = snapshot_cmd_tx.send(SnapshotWorkerCommand::Shutdown);
                        break;
                    }
                    continue;
                }
                let mut request_snapshot = false;
                let mut refresh_modifiers_now = false;
                let mut refresh_strap_inventory = false;
                let mut refresh_chip_balance = false;
                match ev {
                    ui::UserEvent::Quit => {
                        let _ = snapshot_cmd_tx.send(SnapshotWorkerCommand::Shutdown);
                        break;
                    }
                    ui::UserEvent::NextRoll => {
                        controller.select_next_roll();
                        if let Some(cache) = controller.last_snapshot.as_mut() {
                            cache.selected_roll = controller.selected_roll.clone();
                        }
                        if let Some(snapshot) = last_snapshot.as_mut() {
                            snapshot.selected_roll = controller.selected_roll.clone();
                            ui::draw(ui_state, snapshot)
                                .wrap_err("draw after NextRoll failed")?;
                        }
                        continue;
                    }
                    ui::UserEvent::PrevRoll => {
                        controller.select_prev_roll();
                        if let Some(cache) = controller.last_snapshot.as_mut() {
                            cache.selected_roll = controller.selected_roll.clone();
                        }
                        if let Some(snapshot) = last_snapshot.as_mut() {
                            snapshot.selected_roll = controller.selected_roll.clone();
                            ui::draw(ui_state, snapshot)
                                .wrap_err("draw after PrevRoll failed")?;
                        }
                        continue;
                    }
                    ui::UserEvent::PlaceBetAmount(amount) => {
                        if let Some(snapshot) = last_snapshot.as_mut() {
                            let roll = controller.selected_roll.clone();
                            let chip_label = if amount == 1 { "chip" } else { "chips" };
                            let status_msg = format!(
                                "Placing bet of {} {} on {:?}...",
                                amount, chip_label, roll
                            );
                            show_processing_status(
                                &mut controller,
                                snapshot,
                                ui_state,
                                status_msg,
                                "draw while submitting chip bet failed",
                            )?;
                        }
                        match controller.place_chip_bet(amount).await {
                            Ok(_) => {
                                request_snapshot = true;
                                refresh_chip_balance = true;
                            }
                            Err(e) => {
                                let msg = format!("Chip bet failed: {}", e);
                                error!(error = %e, "chip bet failed");
                                if let Some(snapshot) = last_snapshot.as_mut() {
                                    sync_error(&mut controller, snapshot, msg);
                                    ui::draw(ui_state, snapshot)
                                        .wrap_err("draw after chip bet failure failed")?;
                                } else {
                                    controller.push_errors(vec![msg]);
                                }
                                continue;
                            }
                        }
                    }
                    ui::UserEvent::Purchase => {
                        let price_lookup = last_snapshot.as_ref().and_then(|snap| {
                            snap.modifier_triggers
                                .iter()
                                .find(|(_, target, _, triggered, purchased, _)| {
                                    *target == controller.selected_roll
                                        && *triggered
                                        && !*purchased
                                })
                                .map(|(_, target, modifier, _, _, price)| {
                                    (*price, target.clone(), modifier.clone())
                                })
                        });
                        if let Some(snapshot) = last_snapshot.as_mut() {
                            let status_msg = format!(
                                "Purchasing triggered modifier for {:?}...",
                                controller.selected_roll
                            );
                            show_processing_status(
                                &mut controller,
                                snapshot,
                                ui_state,
                                status_msg,
                                "draw while submitting triggered modifier purchase failed",
                            )?;
                        }
                        let Some((cost, target, modifier)) = price_lookup
                        else {
                            controller.push_errors(vec![
                                "No triggered modifier for the selected roll".to_string(),
                            ]);
                            if let Some(snapshot) = last_snapshot.as_mut() {
                                ui::draw(ui_state, snapshot)
                                    .wrap_err("draw after missing modifier failed")?;
                            }
                            continue;
                        };
                        controller
                            .purchase_triggered_modifier(
                                target.clone(),
                                modifier.clone(),
                                cost,
                            )
                            .await
                            .wrap_err_with(|| {
                                format!(
                                    "purchasing triggered modifier failed (roll {:?}, modifier {:?}, cost {})",
                                    target, modifier, cost
                                )
                            })?;
                        request_snapshot = true;
                        refresh_modifiers_now = true;
                        refresh_chip_balance = true;
                    }
                    ui::UserEvent::ConfirmStrapBet { strap, amount } => {
                        if let Some(snapshot) = last_snapshot.as_mut() {
                            let strap_label = super_compact_strap(&strap);
                            let status_msg = format!(
                                "Placing {} of {} on {:?}...",
                                amount, strap_label, controller.selected_roll
                            );
                            show_processing_status(
                                &mut controller,
                                snapshot,
                                ui_state,
                                status_msg,
                                "draw while submitting strap bet failed",
                            )?;
                        }
                        match controller.place_strap_bet(strap, amount).await {
                            Ok(_) => {
                                request_snapshot = true;
                                refresh_strap_inventory = true;
                            }
                            Err(e) => {
                                let msg = format!("Strap bet failed: {}", e);
                                error!(error = %e, "strap bet failed");
                                if let Some(snapshot) = last_snapshot.as_mut() {
                                    sync_error(&mut controller, snapshot, msg);
                                    ui::draw(ui_state, snapshot)
                                        .wrap_err("draw after strap bet failure failed")?;
                                } else {
                                    controller.push_errors(vec![msg]);
                                }
                                continue;
                            }
                        }
                    }
                    ui::UserEvent::Roll => {
                        if let Some(snapshot) = last_snapshot.as_ref() {
                            let prev_len = snapshot.roll_history.len();
                            let prev_last = snapshot.roll_history.last().cloned();
                            let prev_game_id = snapshot.current_game_id;
                            if let Some(snapshot_mut) = last_snapshot.as_mut() {
                                show_processing_status(
                                    &mut controller,
                                    snapshot_mut,
                                    ui_state,
                                    "Rolling...",
                                    "draw while submitting roll failed",
                                )?;
                            }
                            controller.roll().await.wrap_err("roll failed")?;
                            request_snapshot = true;
                            refresh_modifiers_now = true;
                            pending_post_action = Some(PostAction::Roll {
                                prev_len,
                                prev_last,
                                prev_game_id,
                            });
                        }
                    }
                    ui::UserEvent::VRFInc => {
                        controller.inc_vrf();
                        if let Some(snapshot) = last_snapshot.as_mut() {
                            let target = controller.vrf_number;
                            let status_msg = format!("Setting VRF to {}...", target);
                            show_processing_status(
                                &mut controller,
                                snapshot,
                                ui_state,
                                status_msg,
                                "draw while submitting VRF increment failed",
                            )?;
                        }
                        controller
                            .set_vrf_number(controller.vrf_number)
                            .await
                            .wrap_err("setting VRF number (inc) failed")?;
                        request_snapshot = true;
                    }
                    ui::UserEvent::VRFDec => {
                        controller.dec_vrf();
                        if let Some(snapshot) = last_snapshot.as_mut() {
                            let target = controller.vrf_number;
                            let status_msg = format!("Setting VRF to {}...", target);
                            show_processing_status(
                                &mut controller,
                                snapshot,
                                ui_state,
                                status_msg,
                                "draw while submitting VRF decrement failed",
                            )?;
                        }
                        controller
                            .set_vrf_number(controller.vrf_number)
                            .await
                            .wrap_err("setting VRF number (dec) failed")?;
                        request_snapshot = true;
                    }
                    ui::UserEvent::SetVrf(n) => {
                        if let Some(snapshot) = last_snapshot.as_mut() {
                            let status_msg = format!("Setting VRF to {}...", n);
                            show_processing_status(
                                &mut controller,
                                snapshot,
                                ui_state,
                                status_msg,
                                "draw while submitting explicit VRF set failed",
                            )?;
                        }
                        controller
                            .set_vrf_number(n)
                            .await
                            .wrap_err_with(|| format!("setting VRF number to {} failed", n))?;
                        request_snapshot = true;
                    }
                    ui::UserEvent::ConfirmClaim { game_id, enabled } => {
                        if let Some(snapshot) = last_snapshot.as_mut() {
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
                                &mut controller,
                                snapshot,
                                ui_state,
                                status_msg,
                                "draw while submitting claim failed",
                            )?;
                        }
                        controller
                            .claim_game(game_id, enabled)
                            .await
                            .wrap_err_with(|| {
                                format!("claiming game {} with modifiers failed", game_id)
                            })?;
                        request_snapshot = true;
                        refresh_strap_inventory = true;
                    }
                    ui::UserEvent::OpenShop
                    | ui::UserEvent::OpenBetModal
                    | ui::UserEvent::OpenClaimModal
                    | ui::UserEvent::OpenVrfModal
                    | ui::UserEvent::OpenStrapBet
                    | ui::UserEvent::Redraw => {
                        if let Some(snapshot) = last_snapshot.as_ref() {
                            ui::draw(ui_state, snapshot)
                                .wrap_err("draw during modal/redraw failed")?;
                        }
                        continue;
                    }
                    ui::UserEvent::OpenStrapInventory => {
                        controller
                            .refresh_strap_inventory()
                            .await
                            .wrap_err("refreshing strap inventory failed")?;
                        let mut snapshot = controller
                            .build_snapshot()
                            .wrap_err("snapshot refresh after opening strap inventory failed")?;
                        process_post_action(
                            &mut controller,
                            &mut snapshot,
                            &mut pending_post_action,
                        );
                        ui::draw(ui_state, &snapshot)
                            .wrap_err("draw after OpenStrapInventory failed")?;
                        last_snapshot = Some(snapshot);
                        continue;
                    }
                    ui::UserEvent::ConfirmShopPurchase { roll, modifier } => {
                        let already_purchased = last_snapshot.as_ref().is_some_and(
                            |snap| {
                                snap.modifier_triggers.iter().any(
                                    |(_, target, m, _, purchased, _)| {
                                        *target == roll && *m == modifier && *purchased
                                    },
                                )
                            },
                        );
                        if already_purchased {
                            controller.push_errors(vec![format!(
                                "{:?} for {:?} is already purchased",
                                modifier, roll
                            )]);
                            if let Some(snapshot) = last_snapshot.as_mut() {
                                ui::draw(ui_state, snapshot)
                                    .wrap_err("draw after already-purchased modifier")?;
                            }
                            continue;
                        }
                        let cost = last_snapshot
                            .as_ref()
                            .and_then(|snap| {
                                snap.modifier_triggers.iter().find_map(
                                    |(_, target, m, _, _, hint)| {
                                        if *target == roll && *m == modifier {
                                            Some(*hint)
                                        } else {
                                            None
                                        }
                                    },
                                )
                            })
                            .unwrap_or_else(|| modifier_floor_price(&modifier));
                        if let Some(snapshot) = last_snapshot.as_mut() {
                            let status_msg = format!(
                                "Purchasing {:?} for {:?} ({cost} chips)...",
                                modifier, roll
                            );
                            show_processing_status(
                                &mut controller,
                                snapshot,
                                ui_state,
                                status_msg,
                                "draw while submitting shop purchase failed",
                            )?;
                        }
                        controller
                            .purchase_modifier_for(
                                roll.clone(),
                                modifier.clone(),
                                cost,
                            )
                            .await
                            .wrap_err_with(|| {
                                format!(
                                    "shop purchase failed (roll {:?}, modifier {:?}, cost {})",
                                    roll, modifier, cost
                                )
                            })?;
                        request_snapshot = true;
                        refresh_modifiers_now = true;
                        refresh_chip_balance = true;
                    }
                }

                if refresh_modifiers_now {
                    controller
                        .refresh_active_modifiers_now()
                        .await
                        .wrap_err("active modifiers refresh failed")?;
                }
                if refresh_strap_inventory {
                    controller
                        .refresh_strap_inventory()
                        .await
                        .wrap_err("strap inventory refresh failed")?;
                    let mut snapshot = controller
                        .build_snapshot()
                        .wrap_err("snapshot refresh after strap inventory update failed")?;
                    process_post_action(
                        &mut controller,
                        &mut snapshot,
                        &mut pending_post_action,
                    );
                    ui::draw(ui_state, &snapshot)
                        .wrap_err("draw after strap inventory update failed")?;
                    last_snapshot = Some(snapshot);
                } else if refresh_chip_balance {
                    controller
                        .refresh_chip_balance()
                        .await
                        .wrap_err("chip balance refresh failed")?;
                    let mut snapshot = controller
                        .build_snapshot()
                        .wrap_err("snapshot refresh after chip balance update failed")?;
                    process_post_action(
                        &mut controller,
                        &mut snapshot,
                        &mut pending_post_action,
                    );
                    ui::draw(ui_state, &snapshot)
                        .wrap_err("draw after chip balance update failed")?;
                    last_snapshot = Some(snapshot);
                }
                if request_snapshot {
                    let _ = snapshot_cmd_tx.send(SnapshotWorkerCommand::FetchNow);
                }
            }
        }
    }

    let _ = snapshot_cmd_tx.send(SnapshotWorkerCommand::Shutdown);
    match snapshot_handle.await {
        Ok(Ok(())) => {
            if snapshot_worker_closed {
                return Err(anyhow!(
                    "Snapshot worker exited unexpectedly; check the indexer connection"
                ));
            }
        }
        Ok(Err(err)) => {
            return Err(err).wrap_err("snapshot worker failed");
        }
        Err(err) => {
            return Err(anyhow!(err)).wrap_err("snapshot worker panicked");
        }
    }
    Ok(())
}
