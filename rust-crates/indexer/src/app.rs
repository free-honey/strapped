use crate::{
    Result,
    app::{
        event_source::EventSource,
        query_api::{
            AccountSnapshotQuery,
            BetHistoryGame,
            BetHistoryPage,
            BetHistoryQuery,
            EquipmentSnapshotQuery,
            HistoricalAccountSnapshotQuery,
            HistoricalSnapshotQuery,
            Query,
            QueryAPI,
            UnclaimedGame,
            UnclaimedGamesPage,
            UnclaimedGamesQuery,
        },
        snapshot_storage::{
            MetadataStorage,
            SnapshotStorage,
        },
    },
    events::{
        ClaimRewardsEvent,
        ContractEvent,
        Event,
        FundPotEvent,
        InitializedEvent,
        EquipmentAccessoryAddedEvent,
        EquipmentAccessoryRemovedEvent,
        EquipmentBaseClearedEvent,
        EquipmentBaseSetEvent,
        Modifier,
        ModifierTriggeredEvent,
        NewGameEvent,
        PlaceChipBetEvent,
        PlaceStrapBetEvent,
        PurchaseModifierEvent,
        Roll,
        RollEvent,
        Strap,
        StrapKind,
    },
    snapshot::{
        ALL_ROLLS,
        AccountBetKind,
        AccountBetPlacement,
        AccountRollBets,
        AccountSnapshot,
        ActiveModifier,
        EquipmentSnapshot,
        HistoricalSnapshot,
        ModifierShopEntry,
        OverviewSnapshot,
    },
};
use anyhow::anyhow;
use fuels::{
    tx::ContractIdExt,
    types::{
        Address,
        ContractId,
        Identity,
    },
};
use std::str::FromStr;

#[cfg(test)]
mod tests;

pub mod fuel_indexer_event_source;

pub mod actix_query_api;

pub mod in_memory_snapshot_storage;

pub mod in_memory_metadata_storage;

pub mod sled_storage;

pub mod event_source;
pub mod query_api;
pub mod snapshot_storage;

pub struct App<Events, API, Snapshots, Metadata> {
    events: Events,
    api: API,
    snapshots: Snapshots,
    metadata: Metadata,
    contract_id: ContractId,
    historical_modifiers: Vec<ActiveModifier>,
    roll_frequency: Option<u32>,
    first_roll_height: Option<u32>,
    modifier_triggered: Vec<Modifier>,
    modifier_purchased: Vec<Modifier>,
}

fn roll_to_index(roll: &Roll) -> usize {
    use Roll::*;
    match roll {
        Two => 0,
        Three => 1,
        Four => 2,
        Five => 3,
        Six => 4,
        Seven => 5,
        Eight => 6,
        Nine => 7,
        Ten => 8,
        Eleven => 9,
        Twelve => 10,
    }
}

fn accumulate_strap(bets: &mut Vec<(Strap, u64)>, strap: &Strap, amount: u64) {
    if let Some(idx) = bets.iter().position(|(existing, _)| existing == strap) {
        bets[idx].1 = bets[idx].1.saturating_add(amount);
    } else {
        bets.push((strap.clone(), amount));
    }
}

fn account_has_bets(snapshot: &AccountSnapshot) -> bool {
    if snapshot.total_chip_bet > 0 || !snapshot.strap_bets.is_empty() {
        return true;
    }
    snapshot
        .per_roll_bets
        .iter()
        .any(|entry| !entry.bets.is_empty())
}

fn roll_hit_after_bet(target_roll: &Roll, bet_roll_index: u32, rolls: &[Roll]) -> bool {
    rolls
        .iter()
        .enumerate()
        .any(|(idx, r)| r == target_roll && bet_roll_index <= idx as u32)
}

fn account_has_claimable_bets(snapshot: &AccountSnapshot, rolls: &[Roll]) -> bool {
    if rolls.is_empty() {
        return false;
    }
    for roll_entry in &snapshot.per_roll_bets {
        for bet in &roll_entry.bets {
            if roll_hit_after_bet(&roll_entry.roll, bet.bet_roll_index, rolls) {
                return true;
            }
        }
    }
    false
}

impl<Events, API, Snapshots, Metadata> App<Events, API, Snapshots, Metadata>
where
    Snapshots: SnapshotStorage,
{
    pub fn new(
        events: Events,
        api: API,
        snapshots: Snapshots,
        metadata: Metadata,
        contract_id: ContractId,
    ) -> Self {
        let (roll_frequency, first_roll_height) = snapshots
            .latest_snapshot()
            .ok()
            .map(|(snapshot, _)| (snapshot.roll_frequency, snapshot.first_roll_height))
            .unwrap_or((None, None));
        Self {
            events,
            api,
            snapshots,
            metadata,
            contract_id,
            historical_modifiers: Vec::new(),
            roll_frequency,
            first_roll_height,
            modifier_triggered: Vec::new(),
            modifier_purchased: Vec::new(),
        }
    }

    fn refresh_height(&self, snapshot: &mut OverviewSnapshot, height: u32) {
        snapshot.current_block_height = height;
    }

    fn ensure_account_roll_template(snapshot: &mut AccountSnapshot) {
        if snapshot.per_roll_bets.len() == ALL_ROLLS.len() {
            return;
        }

        let mut existing = std::mem::take(&mut snapshot.per_roll_bets);
        let mut rebuilt = Vec::with_capacity(ALL_ROLLS.len());
        for roll in ALL_ROLLS {
            if let Some(pos) = existing.iter().position(|entry| entry.roll == roll) {
                rebuilt.push(existing.swap_remove(pos));
            } else {
                rebuilt.push(AccountRollBets {
                    roll,
                    bets: Vec::new(),
                });
            }
        }
        snapshot.per_roll_bets = rebuilt;
    }

    fn append_bet_to_account(
        snapshot: &mut AccountSnapshot,
        roll: Roll,
        placement: AccountBetPlacement,
    ) {
        Self::ensure_account_roll_template(snapshot);
        if let Some(entry) = snapshot
            .per_roll_bets
            .iter_mut()
            .find(|entry| entry.roll == roll)
        {
            entry.bets.push(placement);
        }
    }

    fn upsert_table_bets(
        snapshot: &mut OverviewSnapshot,
        identity: &Identity,
        per_roll_bets: &[AccountRollBets],
    ) {
        if let Some(existing) = snapshot
            .table_bets
            .iter_mut()
            .find(|entry| entry.identity == *identity)
        {
            existing.per_roll_bets = per_roll_bets.to_vec();
        } else {
            snapshot.table_bets.push(crate::snapshot::TableAccountBets {
                identity: *identity,
                per_roll_bets: per_roll_bets.to_vec(),
            });
        }
    }

    fn bump_height_if_newer(&mut self, height: u32) -> Result<()> {
        let Ok((mut snapshot, current_height)) = self.snapshots.latest_snapshot() else {
            return Ok(());
        };
        if height <= current_height {
            return Ok(());
        }
        self.refresh_height(&mut snapshot, height);
        self.snapshots.update_snapshot(&snapshot, height)
    }
}

pub fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();
}

pub enum RunState {
    Exit,
    Continue,
}

impl<
    Events: EventSource,
    API: QueryAPI,
    Snapshots: SnapshotStorage,
    Metadata: MetadataStorage,
> App<Events, API, Snapshots, Metadata>
{
    pub async fn run<I: Future<Output = ()>>(
        &mut self,
        interrupt: I,
    ) -> Result<RunState> {
        tokio::select! {
            batch = self.events.next_event_batch() => {
                match batch {
                    Ok(Some((events, height))) => {
                        for event in events {
                            self.handle_event(event, height)?;
                        }
                        self.bump_height_if_newer(height)?;
                        Ok(RunState::Continue)
                    }
                    Ok(None) => {
                        Ok(RunState::Continue)
                    }
                    Err(e) => {
                        Err(e)
                    }
                }
            }
            query = self.api.query() => {
                match query {
                    Ok(Some(inner)) => {
                        self.handle_query(inner)?;
                        Ok(RunState::Continue)
                    }
                    Ok(None) => {
                        tracing::info!("Query server closed, exiting");
                        Ok(RunState::Exit)
                    }
                    Err(e) => Err(e),
                }
            }
            _ = interrupt => {
                tracing::info!("Interrupt received, exiting");
                Ok(RunState::Exit)
            }
        }
    }

    fn remember_strap(&mut self, strap: &Strap) {
        let sub_id = strap.sub_id();
        let asset_id = self.contract_id.asset_id(&sub_id);
        let _ = self.metadata.record_new_asset_id(&asset_id, strap);
    }

    fn handle_event(&mut self, event: Event, height: u32) -> Result<()> {
        match event {
            Event::BlockchainEvent => {
                todo!()
            }
            Event::ContractEvent(contract_event) => match contract_event {
                ContractEvent::Initialized(event) => {
                    self.handle_initialized_event(event, height)
                }
                ContractEvent::Roll(roll_event) => {
                    self.handle_roll_event(roll_event, height)
                }
                ContractEvent::ModifierTriggered(event) => {
                    self.handle_modifier_triggered_event(event, height)
                }
                ContractEvent::NewGame(event) => {
                    self.handle_new_game_event(event, height)
                }
                ContractEvent::PlaceChipBet(event) => {
                    self.handle_place_chip_bet_event(event, height)
                }
                ContractEvent::PlaceStrapBet(event) => {
                    self.handle_place_strap_bet_event(event, height)
                }
                ContractEvent::ClaimRewards(event) => {
                    self.handle_claim_rewards_event(event, height)
                }
                ContractEvent::FundPot(event) => {
                    self.handle_fund_pot_event(event, height)
                }
                ContractEvent::PurchaseModifier(event) => {
                    self.handle_purchase_modifier_event(event, height)
                }
                ContractEvent::EquipmentBaseSet(event) => {
                    self.handle_equipment_base_set_event(event, height)
                }
                ContractEvent::EquipmentBaseCleared(event) => {
                    self.handle_equipment_base_cleared_event(event, height)
                }
                ContractEvent::EquipmentAccessoryAdded(event) => {
                    self.handle_equipment_accessory_added_event(event, height)
                }
                ContractEvent::EquipmentAccessoryRemoved(event) => {
                    self.handle_equipment_accessory_removed_event(event, height)
                }
            },
        }
    }

    fn handle_query(&self, query: Query) -> Result<()> {
        tracing::info!("Handling query {:?}", query);
        match query {
            Query::LatestSnapshot(sender) => {
                let snapshot = self.snapshots.latest_snapshot()?;
                sender.send(snapshot).unwrap();
                Ok(())
            }
            Query::LatestAccountSnapshot(inner) => {
                let AccountSnapshotQuery { identity, sender } = inner;
                let snapshot = self.snapshots.latest_account_snapshot(&identity)?;
                sender.send(snapshot)
                    .map_err(
                        |maybe_snapshot|
                            match maybe_snapshot {
                                Some((snapshot, height)) => {
                                    anyhow!("Could not send `LatestAccountSnapshot` response for {identity:?}: {snapshot:?} at {height:?}")
                                }
                                None => {
                                    anyhow!("Could not send `LatestAccountSnapshot` response for {identity:?}, also it was `None` btw")
                                }
                        }
                    )?;
                Ok(())
            }
            Query::LatestEquipmentSnapshot(inner) => {
                let EquipmentSnapshotQuery { identity, sender } = inner;
                let snapshot = self.snapshots.latest_equipment_snapshot(&identity)?;
                sender
                    .send(snapshot)
                    .map_err(|maybe_snapshot| match maybe_snapshot {
                        Some((snapshot, height)) => anyhow!(
                            "Could not send `LatestEquipmentSnapshot` response for {identity:?}: {snapshot:?} at {height:?}"
                        ),
                        None => anyhow!(
                            "Could not send `LatestEquipmentSnapshot` response for {identity:?}, also it was `None` btw"
                        ),
                    })?;
                Ok(())
            }
            Query::HistoricalSnapshot(inner) => {
                let HistoricalSnapshotQuery { game_id, sender } = inner;
                let snapshot = self.snapshots.historical_snapshots(game_id)?;
                sender.send(Some(snapshot))
                    .map_err(
                        |maybe_snapshot|
                            match maybe_snapshot {
                                Some(snapshot) => {
                                    anyhow!("Could not send `HistoricalSnapshot` response for {game_id:?}: {snapshot:?}")
                                }
                                None => {
                                    anyhow!("Could not send `HistoricalSnapshot` response for {game_id:?}, also it was `None` btw")
                                }
                            }
                    )?;
                Ok(())
            }
            Query::HistoricalAccountSnapshot(inner) => {
                let HistoricalAccountSnapshotQuery {
                    identity,
                    game_id,
                    sender,
                } = inner;
                let snapshot = self.snapshots.account_snapshot_at(&identity, game_id)?;
                sender
                    .send(snapshot)
                    .map_err(|maybe_snapshot| match maybe_snapshot {
                        Some((snapshot, height)) => anyhow!(
                            "Could not send `HistoricalAccountSnapshot` response for {identity:?} at {game_id:?}: {snapshot:?} at {height:?}"
                        ),
                        None => anyhow!(
                            "Could not send `HistoricalAccountSnapshot` response for {identity:?} at {game_id:?}, also it was `None` btw"
                        ),
                    })?;
                Ok(())
            }
            Query::UnclaimedGames(inner) => {
                let UnclaimedGamesQuery {
                    identity,
                    order,
                    limit,
                    cursor,
                    sender,
                } = inner;
                let result = (|| {
                    let page = self
                        .snapshots
                        .unclaimed_game_ids(&identity, order, limit, cursor)?;
                    let mut games = Vec::with_capacity(page.game_ids.len());
                    for game_id in page.game_ids {
                        let Some((account_snapshot, _)) =
                            self.snapshots.account_snapshot_at(&identity, game_id)?
                        else {
                            continue;
                        };
                        if account_snapshot.claimed_rewards.is_some() {
                            continue;
                        }
                        if !account_has_bets(&account_snapshot) {
                            continue;
                        }
                        let historical_snapshot =
                            match self.snapshots.historical_snapshots(game_id) {
                                Ok(snapshot) => snapshot,
                                Err(_) => continue,
                            };
                        games.push(UnclaimedGame {
                            game_id,
                            account_snapshot,
                            historical_snapshot,
                        });
                    }
                    Ok(UnclaimedGamesPage {
                        games,
                        next_cursor: page.next_cursor,
                    })
                })();

                sender.send(result).map_err(|_| {
                    anyhow!("Could not send `UnclaimedGames` response for {identity:?}")
                })?;
                Ok(())
            }
            Query::BetHistory(inner) => {
                let BetHistoryQuery {
                    identity,
                    order,
                    limit,
                    cursor,
                    sender,
                } = inner;
                let result = (|| {
                    let page = self
                        .snapshots
                        .bet_history_game_ids(&identity, order, limit, cursor)?;
                    let mut games = Vec::with_capacity(page.game_ids.len());
                    for game_id in page.game_ids {
                        let Some((account_snapshot, _)) =
                            self.snapshots.account_snapshot_at(&identity, game_id)?
                        else {
                            continue;
                        };
                        let historical_snapshot =
                            match self.snapshots.historical_snapshots(game_id) {
                                Ok(snapshot) => snapshot,
                                Err(_) => continue,
                            };
                        games.push(BetHistoryGame {
                            game_id,
                            account_snapshot,
                            historical_snapshot,
                        });
                    }
                    Ok(BetHistoryPage {
                        games,
                        next_cursor: page.next_cursor,
                    })
                })();

                sender.send(result).map_err(|_| {
                    anyhow!("Could not send `BetHistory` response for {identity:?}")
                })?;
                Ok(())
            }
            Query::AllKnownStraps(sender) => {
                let straps = self.metadata.all_known_straps()?;
                sender.send(straps).map_err(|straps| {
                    anyhow!("Could not send `AllKnownStraps` response: {:?}", straps)
                })?;
                Ok(())
            }
        }
    }

    fn handle_initialized_event(
        &mut self,
        event: InitializedEvent,
        height: u32,
    ) -> Result<()> {
        tracing::info!("Handling InitializedEvent at height {}", height);
        self.roll_frequency = Some(event.roll_frequency);
        self.first_roll_height = Some(event.first_height);

        let mut snapshot = OverviewSnapshot::new();
        let frequency = event.roll_frequency;
        snapshot.next_roll_height = Some(event.first_height + frequency);
        snapshot.roll_frequency = Some(frequency);
        snapshot.first_roll_height = Some(event.first_height);
        snapshot.current_block_height = height;
        self.snapshots.update_snapshot(&snapshot, height)?;
        Ok(())
    }

    fn handle_roll_event(&mut self, event: RollEvent, height: u32) -> Result<()> {
        tracing::info!("Handling RollEvent at height {}", height);
        let (mut snapshot, _) = self.snapshots.latest_snapshot()?;
        snapshot.rolls.push(event.rolled_value);
        snapshot.chips_owed = event.chips_owed_total;
        snapshot.pot_size = event.house_pot_total;
        self.refresh_height(&mut snapshot, height);
        snapshot.next_roll_height = Some(event.next_roll_height);
        self.snapshots.update_snapshot(&snapshot, height)
    }

    fn handle_modifier_triggered_event(
        &mut self,
        event: ModifierTriggeredEvent,
        height: u32,
    ) -> Result<()> {
        tracing::info!("Handling ModifierTriggeredEvent at height {}", height);
        if !self.modifier_triggered.contains(&event.modifier) {
            self.modifier_triggered.push(event.modifier);
        }
        let (mut snapshot, _) = self.snapshots.latest_snapshot()?;
        let idx = roll_to_index(&event.modifier_roll);
        snapshot.modifiers_active[idx] = Some(event.modifier);

        for entry in &mut snapshot.modifier_shop {
            if entry.modifier_roll == event.modifier_roll
                && entry.modifier == event.modifier
            {
                entry.triggered = true;
            }
        }
        let roll_index = event.roll_index;

        let active_modifier = ActiveModifier {
            roll_index,
            modifier: event.modifier,
            modifier_roll: event.modifier_roll,
        };
        self.historical_modifiers.push(active_modifier);
        self.refresh_height(&mut snapshot, height);
        self.snapshots.update_snapshot(&snapshot, height)
    }

    fn handle_new_game_event(&mut self, event: NewGameEvent, height: u32) -> Result<()> {
        tracing::info!("Handling NewGameEvent at height {}", height);
        let NewGameEvent {
            game_id,
            new_straps,
            new_modifiers,
            pot_size,
            chips_owed_total,
        } = event;

        let (previous_snapshot, _) = self.snapshots.latest_snapshot()?;
        let mut historical = HistoricalSnapshot::new(
            previous_snapshot.game_id,
            previous_snapshot.rolls.clone(),
            self.historical_modifiers.clone(),
        );
        historical.strap_rewards = previous_snapshot.rewards.clone();
        self.historical_modifiers.clear();
        let _ = self
            .snapshots
            .write_historical_snapshot(previous_snapshot.game_id, &historical);

        let bettors = self.snapshots.bettors_for_game(previous_snapshot.game_id)?;
        for bettor in bettors {
            let Ok(address) = Address::from_str(&bettor) else {
                continue;
            };
            let identity = Identity::Address(address);
            let Some((account_snapshot, _)) = self
                .snapshots
                .account_snapshot_at(&identity, previous_snapshot.game_id)?
            else {
                continue;
            };
            if account_snapshot.claimed_rewards.is_some() {
                continue;
            }
            if account_has_claimable_bets(&account_snapshot, &historical.rolls) {
                self.snapshots.mark_unclaimed(
                    &identity,
                    previous_snapshot.game_id,
                    height,
                )?;
            }
        }

        // Reset per-game tracking
        self.modifier_triggered.clear();
        self.modifier_purchased.clear();

        let mut snapshot = OverviewSnapshot {
            pot_size,
            chips_owed: chips_owed_total,
            total_chip_bets: 0,
            game_id,
            roll_frequency: self.roll_frequency,
            first_roll_height: self.first_roll_height,
            next_roll_height: previous_snapshot.next_roll_height,
            rewards: new_straps.clone(),
            modifier_shop: new_modifiers
                .into_iter()
                .map(
                    |(trigger_roll, modifier_roll, modifier, price)| ModifierShopEntry {
                        trigger_roll,
                        modifier_roll,
                        modifier,
                        triggered: false,
                        purchased: false,
                        price,
                    },
                )
                .collect(),
            ..Default::default()
        };
        self.refresh_height(&mut snapshot, height);
        self.snapshots.update_snapshot(&snapshot, height)?;
        for (_, strap, _) in new_straps {
            self.remember_strap(&strap);
        }
        Ok(())
    }

    fn handle_place_chip_bet_event(
        &mut self,
        event: PlaceChipBetEvent,
        height: u32,
    ) -> Result<()> {
        tracing::info!("Handling PlaceChipBetEvent at height {}", height);
        let PlaceChipBetEvent {
            game_id,
            player,
            amount,
            roll,
            bet_roll_index,
            ..
        } = event;
        let (mut snapshot, _) = self.snapshots.latest_snapshot()?;
        snapshot.pot_size = snapshot.pot_size.saturating_add(amount);
        snapshot.total_chip_bets = snapshot.total_chip_bets.saturating_add(amount);
        let idx = roll_to_index(&roll);
        snapshot.current_block_height = height;

        let entry = &mut snapshot.specific_bets[idx];
        entry.0 = entry.0.saturating_add(amount);

        let mut account_snapshot = self
            .snapshots
            .latest_account_snapshot(&player)?
            .map(|(snap, _)| snap)
            .unwrap_or_default();
        account_snapshot.total_chip_bet =
            account_snapshot.total_chip_bet.saturating_add(amount);
        let placement = AccountBetPlacement {
            bet_roll_index,
            amount,
            kind: AccountBetKind::Chip,
        };
        Self::append_bet_to_account(&mut account_snapshot, roll, placement);
        Self::upsert_table_bets(&mut snapshot, &player, &account_snapshot.per_roll_bets);

        self.snapshots.update_snapshot(&snapshot, height)?;
        self.snapshots.update_account_snapshot(
            &player,
            game_id,
            &account_snapshot,
            height,
        )?;
        self.snapshots
            .record_bet_history(&player, game_id, height)?;
        Ok(())
    }

    fn handle_place_strap_bet_event(
        &mut self,
        event: PlaceStrapBetEvent,
        height: u32,
    ) -> Result<()> {
        tracing::info!("Handling PlaceStrapBetEvent at height {}", height);
        let PlaceStrapBetEvent {
            game_id,
            player,
            amount,
            bet_roll_index,
            roll,
            strap,
            ..
        } = event;
        let (mut snapshot, _) = self.snapshots.latest_snapshot()?;
        let idx = roll_to_index(&roll);
        if idx < snapshot.specific_bets.len() {
            accumulate_strap(&mut snapshot.specific_bets[idx].1, &strap, amount);
        }
        self.refresh_height(&mut snapshot, height);

        let mut account_snapshot = self
            .snapshots
            .latest_account_snapshot(&player)?
            .map(|(snap, _)| snap)
            .unwrap_or_default();
        accumulate_strap(&mut account_snapshot.strap_bets, &strap, amount);
        let placement = AccountBetPlacement {
            bet_roll_index,
            amount,
            kind: AccountBetKind::Strap(strap.clone()),
        };
        Self::append_bet_to_account(&mut account_snapshot, roll, placement);
        Self::upsert_table_bets(&mut snapshot, &player, &account_snapshot.per_roll_bets);
        self.remember_strap(&strap);
        self.snapshots.update_snapshot(&snapshot, height)?;
        self.snapshots.update_account_snapshot(
            &player,
            game_id,
            &account_snapshot,
            height,
        )?;
        self.snapshots
            .record_bet_history(&player, game_id, height)?;
        Ok(())
    }

    fn handle_claim_rewards_event(
        &mut self,
        event: ClaimRewardsEvent,
        height: u32,
    ) -> Result<()> {
        tracing::info!("Handling ClaimRewardsEvent at height {}", height);
        let ClaimRewardsEvent {
            game_id,
            player,
            total_chips_winnings,
            total_strap_winnings,
            ..
        } = event;
        let (mut snapshot, _) = self.snapshots.latest_snapshot()?;
        snapshot.pot_size = snapshot.pot_size.saturating_sub(total_chips_winnings);
        snapshot.chips_owed = snapshot.chips_owed.saturating_sub(total_chips_winnings);
        self.refresh_height(&mut snapshot, height);
        self.snapshots.update_snapshot(&snapshot, height)?;

        let mut account_snapshot = self
            .snapshots
            .latest_account_snapshot(&player)?
            .map(|(snap, _)| snap)
            .unwrap_or_default();
        Self::ensure_account_roll_template(&mut account_snapshot);
        account_snapshot.total_chip_won = account_snapshot
            .total_chip_won
            .saturating_add(total_chips_winnings);
        let strap_rewards: Vec<(Strap, u64)> = total_strap_winnings.clone();
        for (strap, _) in &strap_rewards {
            self.remember_strap(strap);
        }
        account_snapshot.claimed_rewards = Some((total_chips_winnings, strap_rewards));
        self.snapshots.update_account_snapshot(
            &player,
            game_id,
            &account_snapshot,
            height,
        )?;
        self.snapshots.clear_unclaimed(&player, game_id)?;
        Ok(())
    }

    fn handle_fund_pot_event(&mut self, event: FundPotEvent, height: u32) -> Result<()> {
        tracing::info!("Handling FundPotEvent at height {}", height);
        let (mut snapshot, _) = self.snapshots.latest_snapshot()?;
        snapshot.pot_size = snapshot.pot_size.saturating_add(event.chips_amount);
        self.refresh_height(&mut snapshot, height);
        self.snapshots.update_snapshot(&snapshot, height)
    }

    fn handle_purchase_modifier_event(
        &mut self,
        event: PurchaseModifierEvent,
        height: u32,
    ) -> Result<()> {
        tracing::info!("Handling PurchaseModifierEvent at height {}", height);
        if !self.modifier_purchased.contains(&event.expected_modifier) {
            self.modifier_purchased.push(event.expected_modifier);
        }
        let (mut snapshot, _) = self.snapshots.latest_snapshot()?;
        let modifier = event.expected_modifier;
        let idx = roll_to_index(&event.expected_roll);
        snapshot.modifiers_active[idx] = Some(modifier);

        for entry in &mut snapshot.modifier_shop {
            if entry.modifier_roll == event.expected_roll
                && entry.modifier == event.expected_modifier
            {
                entry.purchased = true;
                entry.triggered = true;
            }
        }
        self.refresh_height(&mut snapshot, height);
        self.snapshots.update_snapshot(&snapshot, height)
    }

    fn handle_equipment_base_set_event(
        &mut self,
        event: EquipmentBaseSetEvent,
        height: u32,
    ) -> Result<()> {
        tracing::info!("Handling EquipmentBaseSetEvent at height {}", height);
        let mut equipment = self
            .snapshots
            .latest_equipment_snapshot(&event.player)?
            .map(|(snapshot, _)| snapshot)
            .unwrap_or_else(EquipmentSnapshot::empty);
        match event.slot {
            StrapKind::Shirt => equipment.shirt = Some(event.strap.clone()),
            StrapKind::Pants => equipment.pants = Some(event.strap.clone()),
            StrapKind::Shoes => equipment.shoes = Some(event.strap.clone()),
            _ => {}
        }
        self.remember_strap(&event.strap);
        if let Some(replaced) = event.replaced {
            self.remember_strap(&replaced);
        }
        self.snapshots.update_equipment_snapshot(
            &event.player,
            &equipment,
            height,
        )
    }

    fn handle_equipment_base_cleared_event(
        &mut self,
        event: EquipmentBaseClearedEvent,
        height: u32,
    ) -> Result<()> {
        tracing::info!("Handling EquipmentBaseClearedEvent at height {}", height);
        let mut equipment = self
            .snapshots
            .latest_equipment_snapshot(&event.player)?
            .map(|(snapshot, _)| snapshot)
            .unwrap_or_else(EquipmentSnapshot::empty);
        match event.slot {
            StrapKind::Shirt => equipment.shirt = None,
            StrapKind::Pants => equipment.pants = None,
            StrapKind::Shoes => equipment.shoes = None,
            _ => {}
        }
        self.remember_strap(&event.strap);
        self.snapshots.update_equipment_snapshot(
            &event.player,
            &equipment,
            height,
        )
    }

    fn handle_equipment_accessory_added_event(
        &mut self,
        event: EquipmentAccessoryAddedEvent,
        height: u32,
    ) -> Result<()> {
        tracing::info!(
            "Handling EquipmentAccessoryAddedEvent at height {}",
            height
        );
        let mut equipment = self
            .snapshots
            .latest_equipment_snapshot(&event.player)?
            .map(|(snapshot, _)| snapshot)
            .unwrap_or_else(EquipmentSnapshot::empty);
        let index = event.index as usize;
        if index >= equipment.accessories.len() {
            equipment.accessories.push(event.strap.clone());
        } else {
            equipment.accessories.insert(index, event.strap.clone());
        }
        self.remember_strap(&event.strap);
        self.snapshots.update_equipment_snapshot(
            &event.player,
            &equipment,
            height,
        )
    }

    fn handle_equipment_accessory_removed_event(
        &mut self,
        event: EquipmentAccessoryRemovedEvent,
        height: u32,
    ) -> Result<()> {
        tracing::info!(
            "Handling EquipmentAccessoryRemovedEvent at height {}",
            height
        );
        let mut equipment = self
            .snapshots
            .latest_equipment_snapshot(&event.player)?
            .map(|(snapshot, _)| snapshot)
            .unwrap_or_else(EquipmentSnapshot::empty);
        let index = event.index as usize;
        if index < equipment.accessories.len() {
            equipment.accessories.remove(index);
        }
        self.remember_strap(&event.strap);
        self.snapshots.update_equipment_snapshot(
            &event.player,
            &equipment,
            height,
        )
    }
}
