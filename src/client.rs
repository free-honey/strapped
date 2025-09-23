use strapped_contract::strapped_types as strapped;
use strapped_contract::vrf_types as vrf;
use strapped_contract::strap_to_sub_id;
use crate::ui;
use crate::ui::UserEvent;
use color_eyre::eyre::{eyre, Result};
use fuels::prelude::{
    launch_custom_provider_and_get_wallets, AssetConfig, AssetId, Bech32ContractId, CallParameters,
    Contract, ContractId, LoadConfiguration, Provider, TxPolicies, WalletUnlocked, WalletsConfig,
};
use fuels::accounts::ViewOnlyAccount;
use fuels::tx::ContractIdExt;
use fuels::types::{Bits256, Bytes32};
use itertools::Itertools;
use std::time::{Duration, Instant};
use std::collections::HashSet;
use tokio::sync::mpsc;
use tokio::time;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
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
    pub pot_balance: u64,
    pub selected_roll: strapped::Roll,
    pub vrf_number: u64,
    pub status: String,
    pub cells: Vec<RollCell>,
    pub previous_games: Vec<PreviousGameSummary>,
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
    last_seen_game_id: Option<u64>,
    last_bets_by_roll: Vec<(strapped::Roll, Vec<(strapped::Bet, u64, u64)>)>,
    last_active_mods: Vec<(strapped::Roll, strapped::Modifier, u64)>,
    last_roll_history: Vec<strapped::Roll>,
    previous_games: Vec<PreviousGame>,
}

impl AppController {
    pub async fn new_local() -> Result<Self> {
        let clients = init_local().await?;
        Ok(Self {
            clients,
            wallet: WalletKind::Alice,
            selected_roll: strapped::Roll::Six,
            vrf_number: 19,
            status: String::from("Ready"),
            last_seen_game_id: None,
            last_bets_by_roll: Vec::new(),
            last_active_mods: Vec::new(),
            last_roll_history: Vec::new(),
            previous_games: Vec::new(),
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

        // detect rollover
        if let Some(prev) = self.last_seen_game_id {
            if current_game_id > prev {
                self.previous_games.insert(0, PreviousGame {
                    game_id: prev,
                    bets_by_roll: self.last_bets_by_roll.clone(),
                    active_modifiers: self.last_active_mods.clone(),
                    rolls: self.last_roll_history.clone(),
                    claimed: false,
                });
            }
        }
        self.last_seen_game_id = Some(current_game_id);

        let chip_balance = me.account().get_asset_balance(&self.clients.chip_asset_id).await?;
        let pot_balance = get_contract_asset_balance(
            &provider,
            &self.clients.contract_id,
            &self.clients.chip_asset_id,
        )
        .await?;

        // update last tracking
        self.last_bets_by_roll = my_bets.clone();
        self.last_active_mods = active_modifiers.clone();
        self.last_roll_history = roll_history.clone();

        // build cells for UI
        let mut cells = Vec::new();
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
            }
            cells.push(RollCell { roll: r.clone(), chip_total, strap_total, straps, rewards });
        }

        let previous_games = self.previous_games.iter().map(|g| g.to_summary()).collect();

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
            pot_balance,
            selected_roll: self.selected_roll.clone(),
            vrf_number: self.vrf_number,
            status: self.status.clone(),
            cells,
            previous_games,
        })
    }

    pub fn set_wallet(&mut self, w: WalletKind) {
        self.wallet = w;
        // Reset cached previous games for new user context
        self.previous_games.clear();
        self.last_seen_game_id = None;
        self.last_bets_by_roll.clear();
        self.last_active_mods.clear();
        self.last_roll_history.clear();
    }
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
        let cur = me.methods().current_game_id().call().await?.value;
        if cur == 0 { self.status = String::from("No previous game"); return Ok(()); }
        let prev = cur - 1;
        me.methods()
            .claim_rewards(prev, Vec::new())
            .call()
            .await?;
        self.status = format!("Claimed rewards for game {}", prev);
        Ok(())
    }

    pub async fn claim_game(&mut self, game_id: u64, enabled: Vec<(strapped::Roll, strapped::Modifier)>) -> Result<()> {
        let me = self.clients.instance(self.wallet);
        me.methods()
            .claim_rewards(game_id, enabled)
            .call()
            .await?;
        // mark as claimed in local cache
        if let Some(g) = self.previous_games.iter_mut().find(|g| g.game_id == game_id) {
            g.claimed = true;
        }
        self.status = format!("Claimed rewards for game {}", game_id);
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
        let rolls = all_rolls();
        let mut cells = Vec::new();
        for r in &rolls {
            let bets = self
                .bets_by_roll
                .iter()
                .find(|(rr, _)| rr == r)
                .map(|(_, b)| b)
                .cloned()
                .unwrap_or_default();
            let chip_total: u64 = bets
                .iter()
                .filter_map(|(b, amt, _)| match b {
                    strapped::Bet::Chip => Some(*amt),
                    _ => None,
                })
                .sum();
            let strap_total: u64 = bets
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
        let modifiers = self.active_modifiers.clone();
        PreviousGameSummary { game_id: self.game_id, cells, modifiers, rolls: self.rolls.clone(), bets_by_roll: self.bets_by_roll.clone(), claimed: self.claimed }
    }
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
