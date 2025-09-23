use strapped_contract::strapped_types as strapped;
use strapped_contract::vrf_types as vrf;
use strapped_contract::strap_to_sub_id;
use crate::ui;
use crate::ui::UserEvent;
use color_eyre::eyre::{eyre, Result};
use fuels::prelude::{
    launch_custom_provider_and_get_wallets, AssetConfig, AssetId, Bech32ContractId, CallParameters,
    Contract, ContractId, LoadConfiguration, Provider, TxPolicies, WalletUnlocked, WalletsConfig, VariableOutputPolicy,
};
use fuels::accounts::ViewOnlyAccount;
use fuels::tx::ContractIdExt;
use fuels::types::{Bits256, Bytes32};
use itertools::Itertools;
use std::time::{Duration, Instant};
use std::collections::{HashSet, HashMap};
use tokio::sync::mpsc;
use tokio::time;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum WalletKind {
    Owner,
    Alice,
}

#[derive(Clone, Debug, Default)]
pub struct BetsSummary {
    pub by_roll: Vec<(strapped::Roll, u64 /*total amount*/)> ,
}

#[derive(Clone, Debug)]
pub struct AppSnapshot {
    pub now: Instant,
    pub wallet: WalletKind,
    pub current_game_id: u64,
    pub roll_history: Vec<strapped::Roll>,
    pub strap_rewards: Vec<(strapped::Roll, strapped::Strap)>,
    pub modifier_triggers: Vec<(strapped::Roll, strapped::Roll, strapped::Modifier, bool)>,
    pub active_modifiers: Vec<(strapped::Roll, strapped::Modifier, u64 /*roll_index*/ )>,
    pub my_bets: Vec<(strapped::Roll, Vec<(strapped::Bet, u64, u64)>)>,
    pub chip_balance: u64,
    pub strap_balance: u64,
    pub owned_straps: Vec<(strapped::Strap, u64)>,
    pub pot_balance: u64,
    pub selected_roll: strapped::Roll,
    pub vrf_number: u64,
    pub status: String,
    pub cells: Vec<RollCell>,
    pub previous_games: Vec<PreviousGameSummary>,
    pub errors: Vec<String>,
}

pub struct Clients {
    pub owner: strapped::MyContract<WalletUnlocked>,
    pub alice: strapped::MyContract<WalletUnlocked>,
    pub vrf: vrf::VRFContract<WalletUnlocked>,
    pub contract_id: ContractId,
    pub chip_asset_id: AssetId,
}

impl Clients {
    fn instance(&self, who: WalletKind) -> &strapped::MyContract<WalletUnlocked> {
        match who {
            WalletKind::Owner => &self.owner,
            WalletKind::Alice => &self.alice,
        }
    }
}

pub async fn init_local() -> Result<Clients> {
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
    let strapped_bin = "strapped/out/debug/strapped.bin";
    let strapped_id = Contract::load_from(strapped_bin, LoadConfiguration::default())?
        .deploy(&owner, TxPolicies::default())
        .await?;
    let contract_id: ContractId = strapped_id.clone().into();
    let owner_instance = strapped::MyContract::new(strapped_id.clone(), owner.clone());
    let alice_instance = strapped::MyContract::new(strapped_id.clone(), alice.clone());

    // Deploy VRF and connect
    let vrf_bin = "vrf-contract/out/debug/vrf-contract.bin";
    let vrf_id = Contract::load_from(vrf_bin, LoadConfiguration::default())?
        .deploy(&owner, TxPolicies::default())
        .await?;
    let vrf_instance = vrf::VRFContract::new(vrf_id.clone(), owner.clone());

    let vrf_contract_id: ContractId = vrf_id.clone().into();
    owner_instance
        .methods()
        .set_vrf_contract_id(Bits256(*vrf_contract_id))
        .call()
        .await?;

    // Initialize VRF to a known value so first roll matches the UI
    vrf_instance
        .methods()
        .set_number(19)
        .call()
        .await?;

    // Set chip asset id on contract
    owner_instance
        .methods()
        .set_chip_asset_id(chip_asset_id)
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
        vrf: vrf_instance,
        contract_id,
        chip_asset_id,
    })
}

async fn get_contract_asset_balance(provider: &Provider, cid: &ContractId, aid: &AssetId) -> Result<u64> {
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
    strap_rewards_by_game: HashMap<u64, Vec<(strapped::Roll, strapped::Strap)>>,
    errors: Vec<String>,
}

impl AppController {
    async fn fetch_bets_for(&self, who: WalletKind) -> Result<Vec<(strapped::Roll, Vec<(strapped::Bet, u64, u64)>)>> {
        let me = self.clients.instance(who);
        let rolls = all_rolls();
        let mut out = Vec::with_capacity(rolls.len());
        for r in &rolls {
            let bets = me.methods().get_my_bets(r.clone()).call().await?.value;
            out.push((r.clone(), bets));
        }
        Ok(out)
    }
    pub async fn new_local() -> Result<Self> {
        let clients = init_local().await?;
        Ok(Self {
            clients,
            wallet: WalletKind::Alice,
            selected_roll: strapped::Roll::Six,
            vrf_number: 19,
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
            errors: Vec::new(),
        })
    }

    pub async fn snapshot(&mut self) -> Result<AppSnapshot> {
        let who = self.wallet;
        let me = self.clients.instance(who);
        let provider = me.account().provider().ok_or_else(|| eyre!("no provider"))?.clone();

        let current_game_id = me.methods().current_game_id().call().await?.value;
        let roll_history = me.methods().roll_history().call().await?.value;
        let strap_rewards = me.methods().strap_rewards().call().await?.value;
        let modifier_triggers = me.methods().modifier_triggers().call().await?.value;
        let active_modifiers = me.methods().active_modifiers().call().await?.value;

        // My bets by roll
        let all_rolls = all_rolls();
        let mut my_bets = Vec::with_capacity(all_rolls.len());
        for r in &all_rolls {
            let bets = me.methods().get_my_bets(r.clone()).call().await?.value;
            my_bets.push((r.clone(), bets));
        }

        // Refresh current bets for both users on each tick so rollover can snapshot both reliably
        let new_owner_bets = self.fetch_bets_for(WalletKind::Owner).await?;
        let new_alice_bets = self.fetch_bets_for(WalletKind::Alice).await?;

        // Remember strap rewards for this current game (for later claim delta display)
        self.strap_rewards_by_game.entry(current_game_id).or_insert_with(|| strap_rewards.clone());

        // detect rollover using the active wallet's last seen id (avoid holding mutable borrow across await)
        let last_seen_opt = match self.wallet { WalletKind::Owner => self.last_seen_game_id_owner, WalletKind::Alice => self.last_seen_game_id_alice };
        if let Some(prev) = last_seen_opt {
            if current_game_id > prev {
                // Build shared game entry and bets for both users so rolls persist and bets differ
                let owner_bets = self.prev_owner_bets.clone();
                let alice_bets = self.prev_alice_bets.clone();
                self.shared_prev_games.insert(0, SharedGame { game_id: prev, rolls: self.shared_last_roll_history.clone(), modifiers: active_modifiers.clone() });
                self.owner_bets_hist.insert(prev, owner_bets);
                self.alice_bets_hist.insert(prev, alice_bets);
                // Reset both last seen ids to current so we won't insert twice on switch
                self.last_seen_game_id_owner = Some(current_game_id);
                self.last_seen_game_id_alice = Some(current_game_id);
            }
        }
        match self.wallet { WalletKind::Owner => self.last_seen_game_id_owner = Some(current_game_id), WalletKind::Alice => self.last_seen_game_id_alice = Some(current_game_id) };
        // Update cached prev bets for next rollover detection
        self.prev_owner_bets = new_owner_bets;
        self.prev_alice_bets = new_alice_bets;

        let chip_balance = me.account().get_asset_balance(&self.clients.chip_asset_id).await?;
        let pot_balance = get_contract_asset_balance(
            &provider,
            &self.clients.contract_id,
            &self.clients.chip_asset_id,
        )
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
                .filter_map(|(b, amt, _)| match b { strapped::Bet::Chip => Some(*amt), _ => None })
                .sum();
            // Aggregate strap bets per unique strap without requiring Hash
            let mut straps: Vec<(strapped::Strap, u64)> = Vec::new();
            for (b, amt, _) in bets {
                if let strapped::Bet::Strap(s) = b {
                    if let Some((_es, total)) = straps.iter_mut().find(|(es, _)| es == s) {
                        *total += *amt;
                    } else {
                        straps.push((s.clone(), *amt));
                    }
                    if !unique_straps.iter().any(|es| *es == *s) { unique_straps.push(s.clone()); }
                }
            }
            let strap_total: u64 = straps.iter().map(|(_, n)| *n).sum();
            // rewards for this roll (count available rewards, not wallet balance)
            let mut rewards: Vec<(strapped::Strap, u64)> = Vec::new();
            for (_rr, s) in strap_rewards.iter().filter(|(rr, _)| rr == r) {
                if let Some((es, cnt)) = rewards.iter_mut().find(|(es, _)| es == s) {
                    *cnt += 1;
                } else {
                    rewards.push((s.clone(), 1));
                }
                if !unique_straps.iter().any(|es| *es == *s) { unique_straps.push(s.clone()); }
            }
            cells.push(RollCell { roll: r.clone(), chip_total, strap_total, straps, rewards });
        }

        // Sum owned straps for known strap variants (from current bets/rewards + all known rewards by game)
        let mut unique_straps = unique_straps;
        for (_gid, list) in &self.strap_rewards_by_game {
            for (_r, s) in list { if !unique_straps.iter().any(|es| *es == *s) { unique_straps.push(s.clone()); } }
        }

        let mut strap_balance: u64 = 0;
        let mut owned_straps: Vec<(strapped::Strap, u64)> = Vec::new();
        for s in unique_straps {
            let sub = strapped_contract::strap_to_sub_id(&s);
            let aid = self.clients.contract_id.asset_id(&sub);
            let bal = me.account().get_asset_balance(&aid).await.unwrap_or(0);
            strap_balance = strap_balance.saturating_add(bal);
            if bal > 0 { owned_straps.push((s, bal)); }
        }

        // Build previous games by merging shared games with current user's stored bets
        let mut summaries: Vec<PreviousGameSummary> = Vec::new();
        for sg in &self.shared_prev_games {
            let bets = match self.wallet {
                WalletKind::Owner => self.owner_bets_hist.get(&sg.game_id).cloned().unwrap_or_default(),
                WalletKind::Alice => self.alice_bets_hist.get(&sg.game_id).cloned().unwrap_or_default(),
            };
            // Build cells from bets
            let mut cells = Vec::new();
            for r in &all_rolls {
                let bets_for = bets.iter().find(|(rr, _)| rr==r).map(|(_, b)| b).cloned().unwrap_or_default();
                let chip_total: u64 = bets_for.iter().filter_map(|(b, amt, _)| match b { strapped::Bet::Chip => Some(*amt), _ => None }).sum();
                let strap_total: u64 = bets_for.iter().filter_map(|(b, amt, _)| match b { strapped::Bet::Strap(_) => Some(*amt), _ => None }).sum();
                cells.push(RollCell { roll: r.clone(), chip_total, strap_total, straps: Vec::new(), rewards: Vec::new() });
            }
            let claimed = match self.wallet { WalletKind::Owner => self.owner_claimed.contains(&sg.game_id), WalletKind::Alice => self.alice_claimed.contains(&sg.game_id) };
            summaries.push(PreviousGameSummary { game_id: sg.game_id, cells, modifiers: sg.modifiers.clone(), rolls: sg.rolls.clone(), bets_by_roll: bets, claimed });
        }
        let previous_games = summaries;

        Ok(AppSnapshot {
            now: Instant::now(),
            wallet: self.wallet,
            current_game_id,
            roll_history,
            strap_rewards,
            modifier_triggers,
            active_modifiers,
            my_bets,
            chip_balance,
            strap_balance,
            owned_straps,
            pot_balance,
            selected_roll: self.selected_roll.clone(),
            vrf_number: self.vrf_number,
            status: self.status.clone(),
            cells,
            previous_games,
            errors: self.errors.iter().rev().take(5).cloned().collect(),
        })
    }

    pub fn set_wallet(&mut self, w: WalletKind) { self.wallet = w; }
    pub fn select_next_roll(&mut self) { self.selected_roll = next_roll(self.selected_roll.clone()); }
    pub fn select_prev_roll(&mut self) { self.selected_roll = prev_roll(self.selected_roll.clone()); }
    pub fn inc_vrf(&mut self) { self.vrf_number = self.vrf_number.wrapping_add(1); }
    pub fn dec_vrf(&mut self) { self.vrf_number = self.vrf_number.wrapping_sub(1); }

    pub async fn place_chip_bet(&mut self, amount: u64) -> Result<()> {
        let me = self.clients.instance(self.wallet);
        let call = CallParameters::new(amount, self.clients.chip_asset_id, 1_000_000);
        me.methods()
            .place_bet(self.selected_roll.clone(), strapped::Bet::Chip, amount)
            .call_params(call)?
            .call()
            .await?;
        self.status = format!("Placed {} chip(s) on {:?}", amount, self.selected_roll);
        Ok(())
    }

    pub async fn place_strap_bet(&mut self, strap: strapped::Strap, amount: u64) -> Result<()> {
        let me = self.clients.instance(self.wallet);
        let sub = strapped_contract::strap_to_sub_id(&strap);
        let asset_id = self.clients.contract_id.asset_id(&sub);
        let call = CallParameters::new(amount, asset_id, 1_000_000);
        me.methods()
            .place_bet(self.selected_roll.clone(), strapped::Bet::Strap(strap.clone()), amount)
            .call_params(call)?
            .call()
            .await?;
        self.status = format!("Placed {} of {} on {:?}", amount, super_compact_strap(&strap), self.selected_roll);
        Ok(())
    }

    pub async fn purchase_triggered_modifier(&mut self, cost: u64) -> Result<()> {
        // Find a triggered modifier that targets the selected roll
        let me = self.clients.instance(self.wallet);
        let triggers = me.methods().modifier_triggers().call().await?.value;
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
        Ok(())
    }

    pub async fn set_vrf_number(&mut self, n: u64) -> Result<()> {
        // Only owner wallet holds the VRF instance (same provider though).
        self.clients
            .vrf
            .methods()
            .set_number(n)
            .call()
            .await?;
        self.vrf_number = n;
        self.status = format!("VRF set to {}", n);
        Ok(())
    }

    pub async fn roll(&mut self) -> Result<()> {
        // Roll using owner instance but allow any wallet to trigger.
        self.clients
            .owner
            .methods()
            .roll_dice()
            .with_contracts(&[&self.clients.vrf])
            .call()
            .await?;
        self.status = String::from("Rolled dice");
        Ok(())
    }

    pub async fn claim_previous_game(&mut self) -> Result<()> {
        // Naive: claim for previous game id with no modifiers enabled.
        let me = self.clients.instance(self.wallet);
        let mut errs: Vec<String> = Vec::new();
        let cur = me.methods().current_game_id().call().await?.value;
        if cur == 0 { self.status = String::from("No previous game"); return Ok(()); }
        let prev = cur - 1;
        // pre-claim balances
        let pre_chip = me.account().get_asset_balance(&self.clients.chip_asset_id).await.unwrap_or(0);
        let strap_list = self.strap_rewards_by_game.get(&prev).cloned().unwrap_or_default();
        let mut pre_straps: Vec<(strapped::Strap, u64)> = Vec::new();
        for (_roll, s) in &strap_list {
            let sub = strapped_contract::strap_to_sub_id(s);
            let aid = self.clients.contract_id.asset_id(&sub);
            let bal = me.account().get_asset_balance(&aid).await.unwrap_or(0);
            pre_straps.push((s.clone(), bal));
        }

        let outputs = self.expected_claim_outputs(prev, self.wallet);
        if outputs == 0 { self.status = format!("No winnings to claim for game {}", prev); return Ok(()); }
        let res_primary = me
            .methods()
            .claim_rewards(prev, Vec::new())
            .with_variable_output_policy(VariableOutputPolicy::Exactly(outputs))
            .call()
            .await;
        let mut claimed_ok = match res_primary {
            Ok(_) => true,
            Err(e) => { errs.push(format!("claim(prev={}) primary: {}", prev, e)); false }
        };
        // Fallbacks with slightly different counts
        if !claimed_ok && outputs > 1 {
            let res_fb1 = me
                .methods()
                .claim_rewards(prev, Vec::new())
                .with_variable_output_policy(VariableOutputPolicy::Exactly(outputs - 1))
                .call()
                .await;
            claimed_ok = match res_fb1 {
                Ok(_) => true,
                Err(e) => { errs.push(format!("claim(prev={}) fallback-1: {}", prev, e)); false }
            };
        }
        if !claimed_ok {
            let res_fb2 = me
                .methods()
                .claim_rewards(prev, Vec::new())
                .with_variable_output_policy(VariableOutputPolicy::Exactly(outputs + 1))
                .call()
                .await;
            claimed_ok = match res_fb2 {
                Ok(_) => true,
                Err(e) => { errs.push(format!("claim(prev={}) fallback+1: {}", prev, e)); false }
            };
        }
        if !claimed_ok { self.status = format!("Claim failed for game {}", prev); self.push_errors(errs); return Ok(()); }
        match self.wallet { WalletKind::Owner => { self.owner_claimed.insert(prev); }, WalletKind::Alice => { self.alice_claimed.insert(prev); } }
        // post-claim deltas
        let post_chip = me.account().get_asset_balance(&self.clients.chip_asset_id).await.unwrap_or(0);
        let chip_delta = post_chip.saturating_sub(pre_chip);
        let mut strap_deltas: Vec<String> = Vec::new();
        for (s, pre) in pre_straps {
            let sub = strapped_contract::strap_to_sub_id(&s);
            let aid = self.clients.contract_id.asset_id(&sub);
            let post = me.account().get_asset_balance(&aid).await.unwrap_or(0);
            let d = post.saturating_sub(pre);
            if d > 0 { strap_deltas.push(format!("{} x{}", super_compact_strap(&s), d)); }
        }
        let strap_part = if strap_deltas.is_empty() { String::from("") } else { format!(" | Straps: {}", strap_deltas.join(" ")) };
        self.status = format!("Claimed game {} | Chips +{}{}", prev, chip_delta, strap_part);
        self.push_errors(errs);
        Ok(())
    }

    pub async fn claim_game(&mut self, game_id: u64, enabled: Vec<(strapped::Roll, strapped::Modifier)>) -> Result<()> {
        let me = self.clients.instance(self.wallet);
        let mut errs: Vec<String> = Vec::new();
        // pre-claim balances
        let pre_chip = me.account().get_asset_balance(&self.clients.chip_asset_id).await.unwrap_or(0);
        let strap_list = self.strap_rewards_by_game.get(&game_id).cloned().unwrap_or_default();
        let mut pre_straps: Vec<(strapped::Strap, u64)> = Vec::new();
        for (_roll, s) in &strap_list {
            let sub = strapped_contract::strap_to_sub_id(s);
            let aid = self.clients.contract_id.asset_id(&sub);
            let bal = me.account().get_asset_balance(&aid).await.unwrap_or(0);
            pre_straps.push((s.clone(), bal));
        }

        let outputs = self.expected_claim_outputs(game_id, self.wallet);
        if outputs == 0 { self.status = format!("No winnings to claim for game {}", game_id); return Ok(()); }
        let res_primary = me
            .methods()
            .claim_rewards(game_id, enabled.clone())
            .with_variable_output_policy(VariableOutputPolicy::Exactly(outputs))
            .call()
            .await;
        let mut claimed_ok = match res_primary {
            Ok(_) => true,
            Err(e) => { errs.push(format!("claim(game_id={}) primary: {}", game_id, e)); false }
        };
        if !claimed_ok && outputs > 1 {
            let res_fb1 = me
                .methods()
                .claim_rewards(game_id, enabled.clone())
                .with_variable_output_policy(VariableOutputPolicy::Exactly(outputs - 1))
                .call()
                .await;
            claimed_ok = match res_fb1 {
                Ok(_) => true,
                Err(e) => { errs.push(format!("claim(game_id={}) fallback-1: {}", game_id, e)); false }
            };
        }
        if !claimed_ok {
            let res_fb2 = me
                .methods()
                .claim_rewards(game_id, enabled)
                .with_variable_output_policy(VariableOutputPolicy::Exactly(outputs + 1))
                .call()
                .await;
            claimed_ok = match res_fb2 {
                Ok(_) => true,
                Err(e) => { errs.push(format!("claim(game_id={}) fallback+1: {}", game_id, e)); false }
            };
        }
        if !claimed_ok { self.status = format!("Claim failed for game {}", game_id); self.push_errors(errs); return Ok(()); }
        // mark as claimed in local cache for the current user
        match self.wallet {
            WalletKind::Owner => { self.owner_claimed.insert(game_id); },
            WalletKind::Alice => { self.alice_claimed.insert(game_id); },
        }
        // post-claim deltas
        let post_chip = me.account().get_asset_balance(&self.clients.chip_asset_id).await.unwrap_or(0);
        let chip_delta = post_chip.saturating_sub(pre_chip);
        let mut strap_deltas: Vec<String> = Vec::new();
        for (s, pre) in pre_straps {
            let sub = strapped_contract::strap_to_sub_id(&s);
            let aid = self.clients.contract_id.asset_id(&sub);
            let post = me.account().get_asset_balance(&aid).await.unwrap_or(0);
            let d = post.saturating_sub(pre);
            if d > 0 { strap_deltas.push(format!("{} x{}", super_compact_strap(&s), d)); }
        }
        let strap_part = if strap_deltas.is_empty() { String::from("") } else { format!(" | Straps: {}", strap_deltas.join(" ")) };
        self.status = format!("Claimed game {} | Chips +{}{}", game_id, chip_delta, strap_part);
        self.push_errors(errs);
        Ok(())
    }

    pub async fn purchase_modifier_for(&mut self, target: strapped::Roll, modifier: strapped::Modifier, cost: u64) -> Result<()> {
        let me = self.clients.instance(self.wallet);
        let call = CallParameters::new(cost, self.clients.chip_asset_id, 1_000_000);
        me.methods()
            .purchase_modifier(target.clone(), modifier.clone())
            .call_params(call)?
            .call()
            .await?;
        self.status = format!("Purchased {:?} for {:?}", modifier, target);
        Ok(())
    }
}

impl AppController {
    fn expected_claim_outputs(&self, game_id: u64, who: WalletKind) -> usize {
        // Returns number of variable outputs required: 1 (chips if any), plus one per strap mint/level-up
        let shared = match self.shared_prev_games.iter().find(|g| g.game_id == game_id) { Some(g)=>g, None=> return 0 };
        let bets_by_roll = match who {
            WalletKind::Owner => self.owner_bets_hist.get(&game_id),
            WalletKind::Alice => self.alice_bets_hist.get(&game_id),
        };
        let Some(bets_by_roll) = bets_by_roll else { return 0; };
        let strap_rewards = self.strap_rewards_by_game.get(&game_id).cloned().unwrap_or_default();

        let mut strap_outputs = 0usize;
        let mut chip_output = 0usize;
        for (idx, r) in shared.rolls.iter().enumerate() {
            if let Some((_, bets)) = bets_by_roll.iter().find(|(rr, _)| rr == r) {
                let mut eligible_chip = false;
                let mut winning_chip = false;
                let mut strap_bet_count = 0usize;
                for (bet, _amt, roll_index) in bets.iter() {
                    if (*roll_index as usize) <= idx {
                        match bet {
                            strapped::Bet::Chip => {
                                eligible_chip = true;
                                match r { strapped::Roll::Six | strapped::Roll::Eight => winning_chip = true, _ => {} }
                            }
                            strapped::Bet::Strap(_) => { strap_bet_count += 1; }
                        }
                    }
                }
                if eligible_chip {
                    let rewards_for_this_roll = strap_rewards.iter().filter(|(rr, _)| rr == r).count();
                    strap_outputs += rewards_for_this_roll;
                }
                strap_outputs += strap_bet_count;
                if winning_chip && eligible_chip { chip_output = 1; }
            }
        }
        chip_output + strap_outputs
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
    };
    let kind_emoji = match s.kind {
        strapped::StrapKind::Shirt => "👕",
        strapped::StrapKind::Pants => "👖",
        strapped::StrapKind::Shoes => "👟",
        strapped::StrapKind::Hat => "🎩",
        strapped::StrapKind::Glasses => "👓",
        strapped::StrapKind::Watch => "⌚",
        strapped::StrapKind::Ring => "💍",
        strapped::StrapKind::Necklace => "📿",
        strapped::StrapKind::Earring => "🧷",
        strapped::StrapKind::Bracelet => "🧶",
        strapped::StrapKind::Tattoo => "🎨",
        strapped::StrapKind::Piercing => "📌",
        strapped::StrapKind::Coat => "🧥",
        strapped::StrapKind::Scarf => "🧣",
        strapped::StrapKind::Gloves => "🧤",
        strapped::StrapKind::Belt => "🧵",
    };
    format!("{}{}{}", mod_emoji, kind_emoji, s.level)
}

impl AppController {
    fn push_errors(&mut self, mut items: Vec<String>) {
        if items.is_empty() { return; }
        self.errors.append(&mut items);
        if self.errors.len() > 50 {
            let drain = self.errors.len() - 50;
            self.errors.drain(0..drain);
        }
    }
}

pub fn all_rolls() -> Vec<strapped::Roll> {
    use strapped::Roll::*;
    vec![Two, Three, Four, Five, Six, Seven, Eight, Nine, Ten, Eleven, Twelve]
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
    pub rewards: Vec<(strapped::Strap, u64)>,
}

#[derive(Clone, Debug)]
pub struct PreviousGame {
    pub game_id: u64,
    pub bets_by_roll: Vec<(strapped::Roll, Vec<(strapped::Bet, u64, u64)>)>,
    pub active_modifiers: Vec<(strapped::Roll, strapped::Modifier, u64)>,
    pub rolls: Vec<strapped::Roll>,
    pub claimed: bool,
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

impl PreviousGame {
    pub fn to_summary(&self) -> PreviousGameSummary {
        unreachable!("legacy path not used; summaries built from shared games")
    }
}

#[derive(Clone, Debug)]
struct SharedGame {
    game_id: u64,
    rolls: Vec<strapped::Roll>,
    modifiers: Vec<(strapped::Roll, strapped::Modifier, u64)>,
}

    pub async fn run_app() -> Result<()> {
    let mut controller = AppController::new_local().await?;
    let mut ui_state = ui::UiState::default();

    // UI bootstrap
    ui::terminal_enter(&mut ui_state)?;
    let res = run_loop(&mut controller, &mut ui_state).await;
    ui::terminal_exit()?;
    res
}

async fn run_loop(controller: &mut AppController, ui_state: &mut ui::UiState) -> Result<()> {
    let mut ticker = time::interval(Duration::from_millis(1000));
    let mut last_snapshot = controller.snapshot().await?;
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => { break; }
            _ = ticker.tick() => {
                last_snapshot = controller.snapshot().await?;
                ui::draw(ui_state, &last_snapshot)?;
            }
            ev = ui::next_event(ui_state) => {
                match ev? {
                    ui::UserEvent::Quit => break,
                    ui::UserEvent::NextRoll => controller.select_next_roll(),
                    ui::UserEvent::PrevRoll => controller.select_prev_roll(),
                    ui::UserEvent::Owner => controller.set_wallet(WalletKind::Owner),
                    ui::UserEvent::Alice => controller.set_wallet(WalletKind::Alice),
                    ui::UserEvent::PlaceBet => { let _ = controller.place_chip_bet(1).await; },
                    ui::UserEvent::PlaceBetAmount(amount) => { let _ = controller.place_chip_bet(amount).await; },
                    ui::UserEvent::Purchase => { let _ = controller.purchase_triggered_modifier(1).await; },
                    ui::UserEvent::ConfirmStrapBet { strap, amount } => { let _ = controller.place_strap_bet(strap, amount).await; },
                    ui::UserEvent::Roll => { let _ = controller.roll().await; },
                    ui::UserEvent::VRFInc => { controller.inc_vrf(); let _ = controller.set_vrf_number(controller.vrf_number).await; },
                    ui::UserEvent::VRFDec => { controller.dec_vrf(); let _ = controller.set_vrf_number(controller.vrf_number).await; },
                    ui::UserEvent::SetVrf(n) => { let _ = controller.set_vrf_number(n).await; },
                    ui::UserEvent::Claim => { let _ = controller.claim_previous_game().await; },
                    ui::UserEvent::ConfirmClaim { game_id, enabled } => { let _ = controller.claim_game(game_id, enabled).await; },
                    ui::UserEvent::OpenShop => { ui::draw(ui_state, &last_snapshot)?; continue; },
                    ui::UserEvent::ConfirmShopPurchase { roll, modifier } => { let _ = controller.purchase_modifier_for(roll, modifier, 1).await; },
                    ui::UserEvent::OpenBetModal | ui::UserEvent::OpenClaimModal | ui::UserEvent::OpenVrfModal | ui::UserEvent::Redraw => {
                        // UI-only update; redraw without hitting the chain
                        ui::draw(ui_state, &last_snapshot)?;
                        continue;
                    }
                    _ => {}
                }
                last_snapshot = controller.snapshot().await?;
                ui::draw(ui_state, &last_snapshot)?;
            }
        }
    }
    Ok(())
}
