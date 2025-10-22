use crate::{
    Result,
    app::{
        event_source::EventSource,
        query_api::QueryAPI,
        snapshot_storage::{
            MetadataStorage,
            SnapshotStorage,
        },
    },
    events::{
        ContractEvent,
        Event,
    },
    snapshot::OverviewSnapshot,
};
use anyhow::anyhow;
use generated_abi::strapped_types::{
    ClaimRewardsEvent,
    FundPotEvent,
    ModifierTriggeredEvent,
    NewGameEvent,
    PlaceChipBetEvent,
    PlaceStrapBetEvent,
    PurchaseModifierEvent,
    Roll,
    RollEvent,
    Strap,
};
use std::cmp;

pub mod event_source;
pub mod query_api;
pub mod snapshot_storage;

pub struct App<Events, API, Snapshots, Metadata> {
    events: Events,
    api: API,
    snapshots: Snapshots,
    metadata: Metadata,
}

fn roll_to_index(roll: &Roll) -> Option<usize> {
    use Roll::*;
    match roll {
        Two => Some(0),
        Three => Some(1),
        Four => Some(2),
        Five => Some(3),
        Six => Some(4),
        Eight => Some(5),
        Nine => Some(6),
        Ten => Some(7),
        Eleven => Some(8),
        Twelve => Some(9),
        Seven => None,
    }
}

fn add_strap_bet(bets: &mut Vec<(Strap, u64)>, strap: Strap, amount: u64) {
    if let Some(idx) = bets.iter().position(|(existing, _)| *existing == strap) {
        let (_, existing_amount) = &mut bets[idx];
        *existing_amount = existing_amount.saturating_add(amount);
    } else {
        bets.push((strap, amount));
    }
}

#[cfg(test)]
mod tests;

impl<Events, API, Snapshots, Metadata> App<Events, API, Snapshots, Metadata> {
    pub fn new(
        events: Events,
        api: API,
        snapshots: Snapshots,
        metadata: Metadata,
    ) -> Self {
        Self {
            events,
            api,
            snapshots,
            metadata,
        }
    }
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();
}

impl<
    Events: EventSource,
    API: QueryAPI,
    Snapshots: SnapshotStorage,
    Metadata: MetadataStorage,
> App<Events, API, Snapshots, Metadata>
{
    pub async fn run(&mut self) -> Result<()> {
        init_tracing();
        tokio::select! {
            event = self.events.next_event() => {
                match event {
                    Ok((ev, height)) => {
                        self.handle_event(ev, height)
                    }
                    Err(e) => {
                        Err(e)
                    }
                }
            }
            query = self.api.query() => {
                match query {
                    Ok(_) => Ok(()),
                    Err(e) => Err(e),
                }
            }
        }
    }

    fn handle_event(&mut self, event: Event, height: u32) -> Result<()> {
        match event {
            Event::BlockchainEvent => {
                todo!()
            }
            Event::ContractEvent(contract_event) => match contract_event {
                ContractEvent::Initialized(_) => {
                    self.handle_initialized_event(contract_event, height)
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

    fn handle_initialized_event(
        &mut self,
        _event: ContractEvent,
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
            snapshot.modifiers_active[idx] = true;
        }
        for entry in &mut snapshot.modifier_shop {
            let (_trigger_roll, modifier_roll, modifier, is_active) = entry;
            if *modifier_roll == event.modifier_roll && *modifier == event.modifier {
                *is_active = true;
            }
        }
        self.snapshots.update_snapshot(&snapshot, height)
    }

    fn handle_new_game_event(&mut self, event: NewGameEvent, height: u32) -> Result<()> {
        tracing::info!("Handling NewGameEvent at height {}", height);
        let game_id: u32 = event
            .game_id
            .try_into()
            .map_err(|_| anyhow!("game id {} overflows u32", event.game_id))?;

        let mut snapshot = OverviewSnapshot::default();
        snapshot.game_id = game_id;
        snapshot.rewards = event.new_straps.into_iter().collect();
        snapshot.modifier_shop = event
            .new_modifiers
            .into_iter()
            .map(|(trigger_roll, modifier_roll, modifier)| {
                (trigger_roll, modifier_roll, modifier, false)
            })
            .collect();

        self.snapshots.update_snapshot(&snapshot, height)
    }

    fn handle_place_chip_bet_event(
        &mut self,
        event: PlaceChipBetEvent,
        height: u32,
    ) -> Result<()> {
        tracing::info!("Handling PlaceChipBetEvent at height {}", height);
        let (mut snapshot, _) = self.snapshots.latest_snapshot()?;
        snapshot.pot_size = snapshot.pot_size.saturating_add(event.amount);
        if let Some(idx) = roll_to_index(&event.roll) {
            let entry = &mut snapshot.total_bets[idx];
            entry.0 = entry.0.saturating_add(event.amount);
        }
        self.snapshots.update_snapshot(&snapshot, height)
    }

    fn handle_place_strap_bet_event(
        &mut self,
        event: PlaceStrapBetEvent,
        height: u32,
    ) -> Result<()> {
        tracing::info!("Handling PlaceStrapBetEvent at height {}", height);
        let (mut snapshot, _) = self.snapshots.latest_snapshot()?;
        if !snapshot.total_bets.is_empty() {
            let idx = cmp::min(
                event.bet_roll_index as usize,
                snapshot.total_bets.len().saturating_sub(1),
            );
            add_strap_bet(&mut snapshot.total_bets[idx].1, event.strap, event.amount);
        }
        self.snapshots.update_snapshot(&snapshot, height)
    }

    fn handle_claim_rewards_event(
        &mut self,
        event: ClaimRewardsEvent,
        height: u32,
    ) -> Result<()> {
        tracing::info!("Handling ClaimRewardsEvent at height {}", height);
        let (mut snapshot, _) = self.snapshots.latest_snapshot()?;
        snapshot.pot_size = snapshot
            .pot_size
            .saturating_sub(event.total_chips_winnings);
        self.snapshots.update_snapshot(&snapshot, height)
    }

    fn handle_fund_pot_event(
        &mut self,
        event: FundPotEvent,
        height: u32,
    ) -> Result<()> {
        tracing::info!("Handling FundPotEvent at height {}", height);
        let (mut snapshot, _) = self.snapshots.latest_snapshot()?;
        snapshot.pot_size = snapshot
            .pot_size
            .saturating_add(event.chips_amount);
        self.snapshots.update_snapshot(&snapshot, height)
    }

    fn handle_purchase_modifier_event(
        &mut self,
        event: PurchaseModifierEvent,
        height: u32,
    ) -> Result<()> {
        tracing::info!("Handling PurchaseModifierEvent at height {}", height);
        let (mut snapshot, _) = self.snapshots.latest_snapshot()?;
        if let Some(idx) = roll_to_index(&event.expected_roll) {
            snapshot.modifiers_active[idx] = true;
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
