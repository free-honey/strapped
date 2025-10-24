use fuel_core::types::fuel_tx::SubAssetId;
use fuels::types::{
    AssetId,
    ContractId,
    Identity,
};

use serde::{
    Deserialize,
    Serialize,
};

#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    BlockchainEvent,
    ContractEvent(ContractEvent),
}

#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
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

#[derive(PartialEq, Eq, Debug, Copy, Clone, Serialize, Deserialize)]
pub enum Roll {
    Two,
    Three,
    Four,
    Five,
    Six,
    Seven,
    Eight,
    Nine,
    Ten,
    Eleven,
    Twelve,
}

#[derive(PartialEq, Eq, Debug, Copy, Clone, Serialize, Deserialize)]
pub enum StrapKind {
    Shirt,
    Pants,
    Shoes,
    Dress,
    Hat,
    Glasses,
    Watch,
    Ring,
    Necklace,
    Earring,
    Bracelet,
    Tattoo,
    Skirt,
    Piercing,
    Coat,
    Scarf,
    Gloves,
    Gown,
    Belt,
}

#[derive(PartialEq, Eq, Debug, Copy, Clone, Serialize, Deserialize)]
pub enum Modifier {
    Nothing,
    Burnt,
    Lucky,
    Holy,
    Holey,
    Scotch,
    Soaked,
    Moldy,
    Starched,
    Evil,
    Groovy,
    Delicate,
}

#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
pub struct Strap {
    pub level: u8,
    pub kind: StrapKind,
    pub modifier: Modifier,
}

pub fn strap_to_sub_id(strap: &Strap) -> SubAssetId {
    let level_bytes = strap.level;
    let kind_bytes = match strap.kind {
        StrapKind::Shirt => 0u8,
        StrapKind::Pants => 1u8,
        StrapKind::Shoes => 2u8,
        StrapKind::Dress => 3u8,
        StrapKind::Hat => 4u8,
        StrapKind::Glasses => 5u8,
        StrapKind::Watch => 6u8,
        StrapKind::Ring => 7u8,
        StrapKind::Necklace => 8u8,
        StrapKind::Earring => 9u8,
        StrapKind::Bracelet => 10u8,
        StrapKind::Tattoo => 11u8,
        StrapKind::Skirt => 12u8,
        StrapKind::Piercing => 13u8,
        StrapKind::Coat => 14u8,
        StrapKind::Scarf => 15u8,
        StrapKind::Gloves => 16u8,
        StrapKind::Gown => 17u8,
        StrapKind::Belt => 18u8,
    };
    let modifier_bytes = match strap.modifier {
        Modifier::Nothing => 0u8,
        Modifier::Burnt => 1u8,
        Modifier::Lucky => 2u8,
        Modifier::Holy => 3u8,
        Modifier::Holey => 4u8,
        Modifier::Scotch => 5u8,
        Modifier::Soaked => 6u8,
        Modifier::Moldy => 7u8,
        Modifier::Starched => 8u8,
        Modifier::Evil => 9u8,
        Modifier::Groovy => 10u8,
        Modifier::Delicate => 11u8,
    };
    let mut sub_id = [0u8; 32];
    sub_id[0] = level_bytes;
    sub_id[1] = kind_bytes;
    sub_id[2] = modifier_bytes;
    SubAssetId::from(sub_id)
}

impl Strap {
    pub fn new(level: u8, kind: StrapKind, modifier: Modifier) -> Self {
        Self {
            level,
            kind,
            modifier,
        }
    }

    pub fn sub_id(&self) -> SubAssetId {
        strap_to_sub_id(self)
    }
}

#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
pub struct InitializedEvent {
    pub vrf_contract_id: ContractId,
    pub chip_asset_id: AssetId,
    pub roll_frequency: u32,
    pub first_height: u32,
}

#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
pub struct RollEvent {
    pub game_id: u32,
    pub roll_index: u32,
    pub rolled_value: Roll,
}

#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
pub struct NewGameEvent {
    pub game_id: u32,
    pub new_straps: Vec<(Roll, Strap, u64)>,
    pub new_modifiers: Vec<(Roll, Roll, Modifier)>,
}

#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
pub struct ModifierTriggeredEvent {
    pub game_id: u32,
    pub roll_index: u32,
    pub trigger_roll: Roll,
    pub modifier_roll: Roll,
    pub modifier: Modifier,
}

#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
pub struct PlaceChipBetEvent {
    pub game_id: u32,
    // latest roll index when the bet was placed
    pub bet_roll_index: u32,
    pub player: Identity,
    pub roll: Roll,
    pub amount: u64,
}

#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
pub struct PlaceStrapBetEvent {
    pub game_id: u32,
    // latest roll index when the bet was placed
    pub bet_roll_index: u32,
    pub player: Identity,
    pub strap: Strap,
    pub amount: u64,
}

#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
pub struct ClaimRewardsEvent {
    pub game_id: u32,
    pub player: Identity,
    pub enabled_modifiers: Vec<(Roll, Modifier)>,
    pub total_chips_winnings: u64,
    pub total_strap_winnings: Vec<(Strap, u64)>,
}

#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
pub struct FundPotEvent {
    pub chips_amount: u64,
    pub funder: Identity,
}

#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
pub struct PurchaseModifierEvent {
    pub expected_roll: Roll,
    pub expected_modifier: Modifier,
    pub purchaser: Identity,
}

impl Event {
    pub fn init_event(
        vrf_contract_id: ContractId,
        chip_asset_id: AssetId,
        roll_frequency: u32,
        first_height: u32,
    ) -> Self {
        let inner = InitializedEvent {
            vrf_contract_id,
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
