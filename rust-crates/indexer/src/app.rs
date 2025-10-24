use crate::{
    Result,
    app::{
        event_source::EventSource,
        query_api::{
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
        ActiveModifier,
        HistoricalSnapshot,
        OverviewSnapshot,
    },
};
use fuels::{
    tx::ContractIdExt,
    types::ContractId,
};
use std::cmp;

#[cfg(test)]
mod tests;

pub mod fuel_indexer_event_source;

pub mod actix_query_api;

pub mod in_memory_snapshot_storage;

pub mod in_memory_metadata_storage;

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
}

fn roll_to_index(roll: &Roll) -> Option<usize> {
    use Roll::*;
    match roll {
        Two => Some(0),
        Three => Some(1),
        Four => Some(2),
        Five => Some(3),
        Six => Some(4),
        Seven => Some(5),
        Eight => Some(6),
        Nine => Some(7),
        Ten => Some(8),
        Eleven => Some(9),
        Twelve => Some(10),
    }
}

fn accumulate_strap(bets: &mut Vec<(Strap, u64)>, strap: &Strap, amount: u64) {
    if let Some(idx) = bets.iter().position(|(existing, _)| existing == strap) {
        bets[idx].1 = bets[idx].1.saturating_add(amount);
    } else {
        bets.push((strap.clone(), amount));
    }
}

impl<Events, API, Snapshots, Metadata> App<Events, API, Snapshots, Metadata> {
    pub fn new(
        events: Events,
        api: API,
        snapshots: Snapshots,
        metadata: Metadata,
        contract_id: ContractId,
    ) -> Self {
        Self {
            events,
            api,
            snapshots,
            metadata,
            contract_id,
            historical_modifiers: Vec::new(),
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
        init_tracing();
        tracing::info!("Starting indexer");
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
                    let RollEvent { rolled_value, .. } = roll_event;
                    self.handle_roll_event(rolled_value, height)
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
        }
    }

    fn handle_initialized_event(
        &mut self,
        _event: InitializedEvent,
        height: u32,
    ) -> Result<()> {
        tracing::info!("Handling InitializedEvent at height {}", height);
        let snapshot = OverviewSnapshot::new();
        self.snapshots.update_snapshot(&snapshot, height)
    }

    fn handle_roll_event(&mut self, roll: Roll, height: u32) -> Result<()> {
        tracing::info!("Handling RollEvent at height {}", height);
        let (mut snapshot, _) = self.snapshots.latest_snapshot()?;
        snapshot.rolls.push(roll);
        self.snapshots.update_snapshot(&snapshot, height)
    }

    fn handle_modifier_triggered_event(
        &mut self,
        event: ModifierTriggeredEvent,
        height: u32,
    ) -> Result<()> {
        tracing::info!("Handling ModifierTriggeredEvent at height {}", height);
        let (mut snapshot, _) = self.snapshots.latest_snapshot()?;
        if let Some(idx) = roll_to_index(&event.modifier_roll) {
            snapshot.modifiers_active[idx] = Some(event.modifier);
        }
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
        self.snapshots.update_snapshot(&snapshot, height)
    }

    fn handle_new_game_event(&mut self, event: NewGameEvent, height: u32) -> Result<()> {
        tracing::info!("Handling NewGameEvent at height {}", height);
        let NewGameEvent {
            game_id,
            new_straps,
            new_modifiers,
        } = event;

        if let Ok((previous_snapshot, _)) = self.snapshots.latest_snapshot() {
            let historical = HistoricalSnapshot::new(
                previous_snapshot.game_id,
                previous_snapshot.rolls.clone(),
                self.historical_modifiers.clone(),
            );
            self.historical_modifiers.clear();
            let _ = self
                .snapshots
                .write_historical_snapshot(previous_snapshot.game_id, &historical);
        }

        let mut snapshot = OverviewSnapshot::default();
        snapshot.game_id = game_id;
        snapshot.rewards = new_straps.clone();
        snapshot.modifier_shop = new_modifiers
            .into_iter()
            .map(|(trigger_roll, modifier_roll, modifier)| {
                (trigger_roll, modifier_roll, modifier, false)
            })
            .collect();

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
            player,
            amount,
            roll,
            ..
        } = event;
        let (mut snapshot, _) = self.snapshots.latest_snapshot()?;
        snapshot.pot_size = snapshot.pot_size.saturating_add(amount);
        if let Some(idx) = roll_to_index(&roll) {
            let entry = &mut snapshot.total_bets[idx];
            entry.0 = entry.0.saturating_add(amount);
        }
        self.snapshots.update_snapshot(&snapshot, height)?;

        let mut account_snapshot = self
            .snapshots
            .latest_account_snapshot(&player)
            .map(|(snap, _)| snap)
            .unwrap_or_default();
        account_snapshot.total_chip_bet =
            account_snapshot.total_chip_bet.saturating_add(amount);
        self.snapshots
            .update_account_snapshot(&player, &account_snapshot, height)
    }

    fn handle_place_strap_bet_event(
        &mut self,
        event: PlaceStrapBetEvent,
        height: u32,
    ) -> Result<()> {
        tracing::info!("Handling PlaceStrapBetEvent at height {}", height);
        let PlaceStrapBetEvent {
            player,
            amount,
            bet_roll_index,
            strap,
            ..
        } = event;
        let (mut snapshot, _) = self.snapshots.latest_snapshot()?;
        if !snapshot.total_bets.is_empty() {
            let idx = cmp::min(
                bet_roll_index as usize,
                snapshot.total_bets.len().saturating_sub(1),
            );
            accumulate_strap(&mut snapshot.total_bets[idx].1, &strap, amount);
        }
        self.snapshots.update_snapshot(&snapshot, height)?;

        let mut account_snapshot = self
            .snapshots
            .latest_account_snapshot(&player)
            .map(|(snap, _)| snap)
            .unwrap_or_default();
        accumulate_strap(&mut account_snapshot.strap_bets, &strap, amount);
        self.remember_strap(&strap);
        self.snapshots
            .update_account_snapshot(&player, &account_snapshot, height)
    }

    fn handle_claim_rewards_event(
        &mut self,
        event: ClaimRewardsEvent,
        height: u32,
    ) -> Result<()> {
        tracing::info!("Handling ClaimRewardsEvent at height {}", height);
        let ClaimRewardsEvent {
            player,
            total_chips_winnings,
            total_strap_winnings,
            ..
        } = event;
        let (mut snapshot, _) = self.snapshots.latest_snapshot()?;
        snapshot.pot_size = snapshot.pot_size.saturating_sub(total_chips_winnings);
        self.snapshots.update_snapshot(&snapshot, height)?;

        let mut account_snapshot = self
            .snapshots
            .latest_account_snapshot(&player)
            .map(|(snap, _)| snap)
            .unwrap_or_default();
        account_snapshot.total_chip_won = account_snapshot
            .total_chip_won
            .saturating_add(total_chips_winnings);
        let strap_rewards: Vec<(Strap, u64)> = total_strap_winnings.clone();
        for (strap, _) in &strap_rewards {
            self.remember_strap(strap);
        }
        account_snapshot.claimed_rewards = Some((total_chips_winnings, strap_rewards));
        self.snapshots
            .update_account_snapshot(&player, &account_snapshot, height)
    }

    fn handle_fund_pot_event(&mut self, event: FundPotEvent, height: u32) -> Result<()> {
        tracing::info!("Handling FundPotEvent at height {}", height);
        let (mut snapshot, _) = self.snapshots.latest_snapshot()?;
        snapshot.pot_size = snapshot.pot_size.saturating_add(event.chips_amount);
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
        if let Some(idx) = roll_to_index(&event.expected_roll) {
            snapshot.modifiers_active[idx] = Some(modifier);
        }
        for entry in &mut snapshot.modifier_shop {
            let (_trigger_roll, modifier_roll, modifier, purchased) = entry;
            if *modifier_roll == event.expected_roll
                && *modifier == event.expected_modifier
            {
                *purchased = true;
            }
        }
        self.snapshots.update_snapshot(&snapshot, height)
    }
}
