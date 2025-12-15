library;

use ::contract_types::*;

pub struct InitializedEvent {
    vrf_contract_id: b256,
    chip_asset_id: AssetId,
    roll_frequency: u32,
    first_height: u32,
}

pub fn log_initialized_event(
    vrf_contract_id: b256,
    chip_asset_id: AssetId,
    roll_frequency: u32,
    first_height: u32,
) {
    let event = InitializedEvent {
        vrf_contract_id,
        chip_asset_id,
        roll_frequency,
        first_height,
    };
    log(event);
}

pub struct RollEvent {
    game_id: u32,
    roll_index: u32,
    rolled_value: Roll,
    roll_total_chips: u64,
    chips_owed_total: u64,
    house_pot_total: u64,
    next_roll_height: u32,
}

pub fn log_roll_event(
    game_id: u32,
    roll_index: u32,
    rolled_value: Roll,
    roll_total_chips: u64,
    chips_owed_total: u64,
    house_pot_total: u64,
    next_roll_height: u32,
) {
    let event = RollEvent {
        game_id,
        roll_index,
        rolled_value,
        roll_total_chips,
        chips_owed_total,
        house_pot_total,
        next_roll_height,
    };
    log(event);
}

pub struct NewGameEvent {
    game_id: u32,
    new_straps: Vec<(Roll, Strap, u64)>,
    new_modifiers: Vec<(Roll, Roll, Modifier, u64)>,
    pot_size: u64,
    chips_owed_total: u64,
}

pub fn log_new_game_event(
    game_id: u32,
    new_straps: Vec<(Roll, Strap, u64)>,
    new_modifiers: Vec<(Roll, Roll, Modifier, u64)>,
    pot_size: u64,
    chips_owed_total: u64,
) {
    let event = NewGameEvent {
        game_id,
        new_straps,
        new_modifiers,
        pot_size,
        chips_owed_total,
    };
    log(event);
}

pub struct ModifierTriggeredEvent {
    game_id: u32,
    roll_index: u32,
    trigger_roll: Roll,
    modifier_roll: Roll,
    modifier: Modifier,
}

pub fn log_modifier_triggered(
    game_id: u32,
    roll_index: u32,
    trigger_roll: Roll,
    modifier_roll: Roll,
    modifier: Modifier,
) {
    let event = ModifierTriggeredEvent {
        game_id,
        roll_index,
        trigger_roll,
        modifier_roll,
        modifier,
    };
    log(event);
}

pub struct PlaceChipBetEvent {
    game_id: u32,
    // latest roll index when the bet was placed
    bet_roll_index: u32,
    player: Identity,
    roll: Roll,
    amount: u64,
}

pub fn log_place_chip_bet_event(
    game_id: u32,
    bet_roll_index: u32,
    player: Identity,
    roll: Roll,
    amount: u64,
) {
    let event = PlaceChipBetEvent {
        game_id,
        bet_roll_index,
        player,
        roll,
        amount,
    };
    log(event);
}

pub struct PlaceStrapBetEvent {
    game_id: u32,
    // latest roll index when the bet was placed
    bet_roll_index: u32,
    player: Identity,
    roll: Roll,
    strap: Strap,
    amount: u64,
}

pub fn log_place_strap_bet_event(
    game_id: u32,
    roll_index: u32,
    player: Identity,
    roll: Roll,
    strap: Strap,
    amount: u64,
) {
    let event = PlaceStrapBetEvent {
        game_id,
        bet_roll_index: roll_index,
        player,
        roll,
        strap,
        amount,
    };
    log(event);
}

pub struct ClaimRewardsEvent {
    game_id: u32,
    player: Identity,
    enabled_modifiers: Vec<(Roll, Modifier)>,
    total_chips_winnings: u64,
    total_strap_winnings: Vec<(Strap, u64)>,
}

pub fn log_claim_rewards_event(
    game_id: u32,
    player: Identity,
    enabled_modifiers: Vec<(Roll, Modifier)>,
    total_chips_winnings: u64,
    total_strap_winnings: Vec<(Strap, u64)>,
) {
    let event = ClaimRewardsEvent {
        game_id,
        player,
        enabled_modifiers,
        total_chips_winnings,
        total_strap_winnings,
    };
    log(event);
}

pub struct FundPotEvent {
    chips_amount: u64,
    funder: Identity,
}

pub fn log_fund_pot_event(chips_amount: u64, funder: Identity) {
    let event = FundPotEvent {
        chips_amount,
        funder,
    };
    log(event);
}

pub struct PurchaseModifierEvent {
    expected_roll: Roll,
    expected_modifier: Modifier,
    purchaser: Identity,
}

pub fn log_purchase_modifier_event(
    expected_roll: Roll,
    expected_modifier: Modifier,
    purchaser: Identity,
) {
    let event = PurchaseModifierEvent {
        expected_roll,
        expected_modifier,
        purchaser,
    };
    log(event);
}

pub struct WithdrawHousePotEvent {
    amount: u64,
    to: Identity,
}

pub fn log_house_withdrawal_event(amount: u64, to: Identity) {
    let event = WithdrawHousePotEvent {
        amount,
        to,
    };
    log(event);
}

pub struct InsufficientHouseWithdrawalEvent {
    requested_amount: u64,
    available_amount: u64,
    to: Identity,
}

pub fn log_insufficient_house_withdrawal_event(
    requested_amount: u64,
    available_amount: u64,
    to: Identity,
) {
    let event = InsufficientHouseWithdrawalEvent {
        requested_amount,
        available_amount,
        to,
    };
    log(event);
}
