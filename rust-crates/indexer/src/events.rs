use fuel_core::{
    schema::scalars::SubId,
    types::fuel_tx::SubAssetId,
};
use fuels::types::{
    AssetId,
    Bits256,
    ContractId,
    Identity,
};

use generated_abi::strapped_types;
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

#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
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

#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
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

#[derive(PartialEq, Eq, Debug, Clone, Serialize, Deserialize)]
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

impl From<generated_abi::strapped_types::Strap> for Strap {
    fn from(value: generated_abi::strapped_types::Strap) -> Self {
        let generated_abi::strapped_types::Strap {
            level,
            kind,
            modifier,
        } = value;
        let kind = match kind {
            generated_abi::strapped_types::StrapKind::Shirt => StrapKind::Shirt,
            generated_abi::strapped_types::StrapKind::Pants => StrapKind::Pants,
            generated_abi::strapped_types::StrapKind::Shoes => StrapKind::Shoes,
            generated_abi::strapped_types::StrapKind::Dress => StrapKind::Dress,
            generated_abi::strapped_types::StrapKind::Hat => StrapKind::Hat,
            generated_abi::strapped_types::StrapKind::Glasses => StrapKind::Glasses,
            generated_abi::strapped_types::StrapKind::Watch => StrapKind::Watch,
            generated_abi::strapped_types::StrapKind::Ring => StrapKind::Ring,
            generated_abi::strapped_types::StrapKind::Necklace => StrapKind::Necklace,
            generated_abi::strapped_types::StrapKind::Earring => StrapKind::Earring,
            generated_abi::strapped_types::StrapKind::Bracelet => StrapKind::Bracelet,
            generated_abi::strapped_types::StrapKind::Tattoo => StrapKind::Tattoo,
            generated_abi::strapped_types::StrapKind::Skirt => StrapKind::Skirt,
            generated_abi::strapped_types::StrapKind::Piercing => StrapKind::Piercing,
            generated_abi::strapped_types::StrapKind::Coat => StrapKind::Coat,
            generated_abi::strapped_types::StrapKind::Scarf => StrapKind::Scarf,
            generated_abi::strapped_types::StrapKind::Gloves => StrapKind::Gloves,
            generated_abi::strapped_types::StrapKind::Gown => StrapKind::Gown,
            generated_abi::strapped_types::StrapKind::Belt => StrapKind::Belt,
        };
        let modifier = match modifier {
            generated_abi::strapped_types::Modifier::Nothing => Modifier::Nothing,
            generated_abi::strapped_types::Modifier::Burnt => Modifier::Burnt,
            generated_abi::strapped_types::Modifier::Lucky => Modifier::Lucky,
            generated_abi::strapped_types::Modifier::Holy => Modifier::Holy,
            generated_abi::strapped_types::Modifier::Holey => Modifier::Holey,
            generated_abi::strapped_types::Modifier::Scotch => Modifier::Scotch,
            generated_abi::strapped_types::Modifier::Soaked => Modifier::Soaked,
            generated_abi::strapped_types::Modifier::Moldy => Modifier::Moldy,
            generated_abi::strapped_types::Modifier::Starched => Modifier::Starched,
            generated_abi::strapped_types::Modifier::Evil => Modifier::Evil,
            generated_abi::strapped_types::Modifier::Groovy => Modifier::Groovy,
            generated_abi::strapped_types::Modifier::Delicate => Modifier::Delicate,
        };
        Strap::new(level, kind, modifier)
    }
}
