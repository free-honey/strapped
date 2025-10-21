use generated_abi::strapped_types::{
    ClaimRewardsEvent,
    FundPotEvent,
    InitializedEvent,
    ModifierTriggered,
    NewGameEvent,
    PlaceChipBetEvent,
    PlaceStrapBetEvent,
    PurchaseModifierEvent,
    RollEvent,
};

pub enum Event {
    BlockchainEvent,
    ContractEvent(),
}

pub enum ContractEvent {
    Initialized(InitializedEvent),
    Roll(RollEvent),
    NewGame(NewGameEvent),
    ModifierTriggered(ModifierTriggered),
    PlaceChipBet(PlaceChipBetEvent),
    PlaceStrapBet(PlaceStrapBetEvent),
    ClaimRewards(ClaimRewardsEvent),
    FundPot(FundPotEvent),
    PurchaseModifier(PurchaseModifierEvent),
}
