use crate::{
    Result,
    app::event_source::EventSource,
    events::{
        ClaimRewardsEvent,
        ContractEvent,
        Event,
        FundPotEvent,
        Modifier as AppModifier,
        ModifierTriggeredEvent,
        NewGameEvent as AppNewGameEvent,
        PlaceChipBetEvent,
        PlaceStrapBetEvent,
        PurchaseModifierEvent,
        Roll as AppRoll,
        Strap as AppStrap,
        StrapKind as AppStrapKind,
    },
};
use anyhow::anyhow;
use fuel_core::{
    service::ServiceTrait,
    state::rocks_db::DatabaseConfig,
    types::fuel_types::BlockHeight,
};
use fuel_core_services::{
    ServiceRunner,
    stream::BoxStream,
};
use fuel_indexer::{
    adapters::SimplerProcessorAdapter,
    fuel_events_manager,
    fuel_events_manager::{
        port::StorableEvent,
        service::UnstableEvent,
    },
    fuel_receipts_manager,
    indexer::Task,
    processors::simple_processor::FnReceiptParser,
    try_parse_events,
};
use fuels::{
    core::codec::DecoderConfig,
    prelude::{
        AssetId,
        ContractId,
        Receipt,
    },
    types::Identity,
};
use generated_abi::strapped_types::{
    ClaimRewardsEvent as AbiClaimRewardsEvent,
    FundPotEvent as AbiFundPotEvent,
    InitializedEvent,
    Modifier as AbiModifier,
    ModifierTriggeredEvent as AbiModifierTriggeredEvent,
    NewGameEvent as AbiNewGameEvent,
    PlaceChipBetEvent as AbiPlaceChipBetEvent,
    PlaceStrapBetEvent as AbiPlaceStrapBetEvent,
    PurchaseModifierEvent as AbiPurchaseModifierEvent,
    Roll as AbiRoll,
    RollEvent as AbiRollEvent,
    Strap as AbiStrap,
    StrapKind as AbiStrapKind,
};
use std::convert::TryFrom;
use tokio_stream::StreamExt;

#[cfg(test)]
mod tests;

pub struct FuelIndexerEventSource<Fn>
where
    Fn: FnOnce(DecoderConfig, &Receipt) -> Option<Event> + Copy + Send + Sync + 'static,
{
    _service: ServiceRunner<
        Task<
            SimplerProcessorAdapter<FnReceiptParser<Fn>>,
            fuel_receipts_manager::rocksdb::Storage,
            fuel_events_manager::rocksdb::Storage,
        >,
    >,
    stream: BoxStream<Result<UnstableEvent<Event>>>,
}

impl StorableEvent for Event {}

impl<Fn> EventSource for FuelIndexerEventSource<Fn>
where
    Fn: FnOnce(DecoderConfig, &Receipt) -> Option<Event> + Copy + Send + Sync + 'static,
{
    async fn next_event_batch(&mut self) -> Result<(Vec<Event>, u32)> {
        loop {
            let unstable_event = self
                .stream
                .next()
                .await
                .ok_or(anyhow::anyhow!("no event"))?
                .map_err(|e| anyhow!("failed retrieving next events: {e:?}"))?;
            tracing::debug!("next unstable event: {:?}", unstable_event);
            match unstable_event {
                UnstableEvent::Events((height, events)) => {
                    return Ok((events, *height));
                }
                UnstableEvent::Checkpoint(checkpoint) => {
                    tracing::trace!(
                        "skipping checkpoint at height {} ({} events)",
                        checkpoint.block_height,
                        checkpoint.events_count
                    );
                    continue;
                }
                UnstableEvent::Rollback(_) => {
                    todo!()
                }
            }
        }
    }
}

impl<Fn> FuelIndexerEventSource<Fn>
where
    Fn: FnOnce(DecoderConfig, &Receipt) -> Option<Event> + Copy + Send + Sync + 'static,
{
    pub async fn new(
        handler: Fn,
        temp_dir: std::path::PathBuf,
        database_config: DatabaseConfig,
        indexer_config: fuel_indexer::indexer::IndexerConfig,
        starting_height: BlockHeight,
    ) -> Result<Self> {
        let service = fuel_indexer::indexer::new_logs_indexer(
            handler,
            temp_dir,
            database_config,
            indexer_config,
        )?;
        service.start_and_await().await?;
        let stream = service.shared.events_starting_from(starting_height).await?;
        let new = Self {
            _service: service,
            stream,
        };
        Ok(new)
    }
}

fn map_roll(roll: AbiRoll) -> AppRoll {
    match roll {
        AbiRoll::Two => AppRoll::Two,
        AbiRoll::Three => AppRoll::Three,
        AbiRoll::Four => AppRoll::Four,
        AbiRoll::Five => AppRoll::Five,
        AbiRoll::Six => AppRoll::Six,
        AbiRoll::Seven => AppRoll::Seven,
        AbiRoll::Eight => AppRoll::Eight,
        AbiRoll::Nine => AppRoll::Nine,
        AbiRoll::Ten => AppRoll::Ten,
        AbiRoll::Eleven => AppRoll::Eleven,
        AbiRoll::Twelve => AppRoll::Twelve,
    }
}

fn map_modifier(modifier: AbiModifier) -> AppModifier {
    match modifier {
        AbiModifier::Nothing => AppModifier::Nothing,
        AbiModifier::Burnt => AppModifier::Burnt,
        AbiModifier::Lucky => AppModifier::Lucky,
        AbiModifier::Holy => AppModifier::Holy,
        AbiModifier::Holey => AppModifier::Holey,
        AbiModifier::Scotch => AppModifier::Scotch,
        AbiModifier::Soaked => AppModifier::Soaked,
        AbiModifier::Moldy => AppModifier::Moldy,
        AbiModifier::Starched => AppModifier::Starched,
        AbiModifier::Evil => AppModifier::Evil,
        AbiModifier::Groovy => AppModifier::Groovy,
        AbiModifier::Delicate => AppModifier::Delicate,
    }
}

fn map_strap_kind(kind: AbiStrapKind) -> AppStrapKind {
    match kind {
        AbiStrapKind::Shirt => AppStrapKind::Shirt,
        AbiStrapKind::Pants => AppStrapKind::Pants,
        AbiStrapKind::Shoes => AppStrapKind::Shoes,
        AbiStrapKind::Dress => AppStrapKind::Dress,
        AbiStrapKind::Hat => AppStrapKind::Hat,
        AbiStrapKind::Glasses => AppStrapKind::Glasses,
        AbiStrapKind::Watch => AppStrapKind::Watch,
        AbiStrapKind::Ring => AppStrapKind::Ring,
        AbiStrapKind::Necklace => AppStrapKind::Necklace,
        AbiStrapKind::Earring => AppStrapKind::Earring,
        AbiStrapKind::Bracelet => AppStrapKind::Bracelet,
        AbiStrapKind::Tattoo => AppStrapKind::Tattoo,
        AbiStrapKind::Skirt => AppStrapKind::Skirt,
        AbiStrapKind::Piercing => AppStrapKind::Piercing,
        AbiStrapKind::Coat => AppStrapKind::Coat,
        AbiStrapKind::Scarf => AppStrapKind::Scarf,
        AbiStrapKind::Gloves => AppStrapKind::Gloves,
        AbiStrapKind::Gown => AppStrapKind::Gown,
        AbiStrapKind::Belt => AppStrapKind::Belt,
    }
}

fn map_strap(strap: AbiStrap) -> AppStrap {
    AppStrap::new(
        strap.level,
        map_strap_kind(strap.kind),
        map_modifier(strap.modifier),
    )
}

fn map_identity(identity: Identity) -> Identity {
    identity
}

pub fn parse_event_logs(decoder: DecoderConfig, receipt: &Receipt) -> Option<Event> {
    try_parse_events!(
        [decoder, receipt]
        InitializedEvent => |event| {
            let inner = Event::init_event(
                ContractId::from(event.vrf_contract_id.0),
                AssetId::from(event.chip_asset_id),
                event.roll_frequency,
                event.first_height,
            );
            Some(inner)
        },
        AbiRollEvent => |event| {
            tracing::info!("roll event: {:?}", event);
            let game_id = u32::try_from(event.game_id).ok()?;
            let roll_index = u32::try_from(event.roll_index).ok()?;
            let rolled_value = map_roll(event.rolled_value);
            Some(Event::roll_event(game_id, roll_index, rolled_value))
        },
        AbiNewGameEvent => |event| {
            let game_id = u32::try_from(event.game_id).ok()?;
            let new_straps = event
                .new_straps
                .into_iter()
                .map(|(roll, strap, cost)| (map_roll(roll), map_strap(strap), cost))
                .collect::<Vec<_>>();
            let new_modifiers = event
                .new_modifiers
                .into_iter()
                .map(|(trigger_roll, modifier_roll, modifier)| {
                    (
                        map_roll(trigger_roll),
                        map_roll(modifier_roll),
                        map_modifier(modifier),
                    )
                })
                .collect::<Vec<_>>();
            let inner = AppNewGameEvent {
                game_id,
                new_straps,
                new_modifiers,
            };
            Some(Event::ContractEvent(ContractEvent::NewGame(inner)))
        },
        AbiModifierTriggeredEvent => |event| {
            let inner = ModifierTriggeredEvent {
                game_id: u32::try_from(event.game_id).ok()?,
                roll_index: u32::try_from(event.roll_index).ok()?,
                trigger_roll: map_roll(event.trigger_roll),
                modifier_roll: map_roll(event.modifier_roll),
                modifier: map_modifier(event.modifier),
            };
            Some(Event::ContractEvent(ContractEvent::ModifierTriggered(inner)))
        },
        AbiPlaceChipBetEvent => |event| {
            tracing::info!("bet event: {:?}", event);
            let inner = PlaceChipBetEvent {
                game_id: u32::try_from(event.game_id).ok()?,
                bet_roll_index: u32::try_from(event.bet_roll_index).ok()?,
                player: map_identity(event.player),
                roll: map_roll(event.roll),
                amount: event.amount,
            };
            Some(Event::ContractEvent(ContractEvent::PlaceChipBet(inner)))
        },
        AbiPlaceStrapBetEvent => |event| {
            let inner = PlaceStrapBetEvent {
                game_id: u32::try_from(event.game_id).ok()?,
                bet_roll_index: u32::try_from(event.bet_roll_index).ok()?,
                player: map_identity(event.player),
                roll: map_roll(event.roll),
                strap: map_strap(event.strap),
                amount: event.amount,
            };
            Some(Event::ContractEvent(ContractEvent::PlaceStrapBet(inner)))
        },
        AbiClaimRewardsEvent => |event| {
            let enabled_modifiers = event
                .enabled_modifiers
                .into_iter()
                .map(|(roll, modifier)| (map_roll(roll), map_modifier(modifier)))
                .collect::<Vec<_>>();
            let total_strap_winnings = event
                .total_strap_winnings
                .into_iter()
                .map(|(strap, amount)| (map_strap(strap), amount))
                .collect::<Vec<_>>();
            let inner = ClaimRewardsEvent {
                game_id: u32::try_from(event.game_id).ok()?,
                player: map_identity(event.player),
                enabled_modifiers,
                total_chips_winnings: event.total_chips_winnings,
                total_strap_winnings,
            };
            Some(Event::ContractEvent(ContractEvent::ClaimRewards(inner)))
        },
        AbiFundPotEvent => |event| {
            let inner = FundPotEvent {
                chips_amount: event.chips_amount,
                funder: map_identity(event.funder),
            };
            Some(Event::ContractEvent(ContractEvent::FundPot(inner)))
        },
        AbiPurchaseModifierEvent => |event| {
            let inner = PurchaseModifierEvent {
                expected_roll: map_roll(event.expected_roll),
                expected_modifier: map_modifier(event.expected_modifier),
                purchaser: map_identity(event.purchaser),
            };
            Some(Event::ContractEvent(ContractEvent::PurchaseModifier(inner)))
        }

    )
}
