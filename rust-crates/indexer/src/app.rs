use crate::{
    Result,
    app::{
        event_source::EventSource,
        query_api::{
            AccountSnapshotQuery,
            HistoricalAccountSnapshotQuery,
            HistoricalSnapshotQuery,
            Query,
            QueryAPI,
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
        ModifierTriggeredEvent,
        NewGameEvent,
        PlaceChipBetEvent,
        PlaceStrapBetEvent,
        PurchaseModifierEvent,
        Roll,
        RollEvent,
        Strap,
    },
    snapshot::{
        ALL_ROLLS,
        AccountBetKind,
        AccountBetPlacement,
        AccountRollBets,
        AccountSnapshot,
        ActiveModifier,
        HistoricalSnapshot,
        OverviewSnapshot,
    },
};
use anyhow::anyhow;
use fuels::{
    tx::ContractIdExt,
    types::ContractId,
};

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
                    Ok((events, height)) => {
                        for event in events {
                            self.handle_event(event, height)?;
                        }
                        Ok(RunState::Continue)
                    }
                    Err(e) => {
                        Err(e)
                    }
                }
            }
            query = self.api.query() => {
                match query {
                    Ok(inner) => {
                        self.handle_query(inner)?;
                        Ok(RunState::Continue)
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
        let (mut snapshot, _) = self.snapshots.latest_snapshot()?;
        let idx = roll_to_index(&event.modifier_roll);
        snapshot.modifiers_active[idx] = Some(event.modifier);

        for entry in &mut snapshot.modifier_shop {
            let (_trigger_roll, modifier_roll, modifier, is_active) = entry;
            if *modifier_roll == event.modifier_roll && *modifier == event.modifier {
                *is_active = true;
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

        let mut snapshot = OverviewSnapshot::default();
        snapshot.pot_size = pot_size;
        snapshot.chips_owed = chips_owed_total;
        snapshot.total_chip_bets = 0;
        snapshot.game_id = game_id;
        snapshot.roll_frequency = self.roll_frequency;
        snapshot.first_roll_height = self.first_roll_height;
        snapshot.next_roll_height = previous_snapshot.next_roll_height;
        snapshot.rewards = new_straps.clone();
        snapshot.modifier_shop = new_modifiers
            .into_iter()
            .map(|(trigger_roll, modifier_roll, modifier)| {
                (trigger_roll, modifier_roll, modifier, false)
            })
            .collect();
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

        self.snapshots.update_snapshot(&snapshot, height)?;

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
        self.snapshots.update_account_snapshot(
            &player,
            game_id,
            &account_snapshot,
            height,
        )?;
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
        self.snapshots.update_snapshot(&snapshot, height)?;

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
        self.remember_strap(&strap);
        self.snapshots.update_account_snapshot(
            &player,
            game_id,
            &account_snapshot,
            height,
        )
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
        )
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
        let (mut snapshot, _) = self.snapshots.latest_snapshot()?;
        let modifier = event.expected_modifier;
        let idx = roll_to_index(&event.expected_roll);
        snapshot.modifiers_active[idx] = Some(modifier);

        for entry in &mut snapshot.modifier_shop {
            let (_trigger_roll, modifier_roll, modifier, purchased) = entry;
            if *modifier_roll == event.expected_roll
                && *modifier == event.expected_modifier
            {
                *purchased = true;
            }
        }
        self.refresh_height(&mut snapshot, height);
        self.snapshots.update_snapshot(&snapshot, height)
    }
}
