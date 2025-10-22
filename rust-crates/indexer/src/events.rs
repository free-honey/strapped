use fuels::types::*;

use generated_abi::strapped_types::{
    ClaimRewardsEvent,
    FundPotEvent,
    InitializedEvent,
    ModifierTriggeredEvent,
    NewGameEvent,
    PlaceChipBetEvent,
    PlaceStrapBetEvent,
    PurchaseModifierEvent,
    Roll,
    RollEvent,
};

pub enum Event {
    BlockchainEvent,
    ContractEvent(ContractEvent),
}

pub enum ContractEvent {
    Initialized(InitializedEvent),
    Roll(RollEvent),
    NewGame(NewGameEvent),
    ModifierTriggered(ModifierTriggeredEvent),
    PlaceChipBet(PlaceChipBetEvent),
    PlaceStrapBet(PlaceStrapBetEvent),
    ClaimRewards(ClaimRewardsEvent),
    FundPot(FundPotEvent),
    PurchaseModifier(PurchaseModifierEvent),
}

// pub struct InitializedEvent {
//     vrf_contract_id: b256,
//     chip_asset_id: AssetId,
//     roll_frequency: u32,
//     first_height: u32,
//     }
//
// pub struct RollEvent {
//     game_id: u64,
//     roll_index: u64,
//     rolled_value: Roll,
// }
//
// pub struct NewGameEvent {
//     game_id: u64,
//     new_straps: Vec<(Roll, Strap, u64)>,
//     new_modifiers: Vec<(Roll, Roll, Modifier)>,
// }
//
// pub struct ModifierTriggeredEvent {
//     game_id: u64,
//     roll_index: u64,
//     trigger_roll: Roll,
//     modifier_roll: Roll,
//     modifier: Modifier,
// }
//
// pub struct PlaceChipBetEvent {
//     game_id: u64,
//     // latest roll index when the bet was placed
//     bet_roll_index: u64,
//     player: Identity,
//     roll: Roll,
//     amount: u64,
// }
//
// pub struct PlaceStrapBetEvent {
//     game_id: u64,
//     // latest roll index when the bet was placed
//     bet_roll_index: u64,
//     player: Identity,
//     strap: Strap,
//     amount: u64,
// }
//
// pub struct ClaimRewardsEvent {
//     game_id: u64,
//     player: Identity,
//     enabled_modifiers: Vec<(Roll, Modifier)>,
//     total_chips_winnings: u64,
//     total_strap_winnings: Vec<(SubId, u64)>,
// }
//
// pub struct FundPotEvent {
//     chips_amount: u64,
//     funder: Identity,
// }
//
// pub struct PurchaseModifierEvent {
//     expected_roll: Roll,
//     expected_modifier: Modifier,
//     purchaser: Identity,
// }
impl Event {
    pub fn init_event(
        vrf_contract_id: [u8; 32],
        chip_asset_id: [u8; 32],
        roll_frequency: u32,
        first_height: u32,
    ) -> Self {
        let inner = InitializedEvent {
            vrf_contract_id: fuels::types::Bits256(vrf_contract_id),
            chip_asset_id: chip_asset_id.into(),
            roll_frequency,
            first_height,
        };
        Event::ContractEvent(ContractEvent::Initialized(inner))
    }

    pub fn roll_event(game_id: u32, roll_index: u32, rolled_value: Roll) -> Self {
        let inner = RollEvent {
            game_id,
            roll_index,
            rolled_value,
        };
        Event::ContractEvent(ContractEvent::Roll(inner))
    }
}
