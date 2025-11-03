contract;

pub mod contract_types;
pub mod helpers;
pub mod events;

use std::storage::storage_vec::*;
use std::storage::storage_map::*;
use std::call_frames::msg_asset_id;
use std::context::msg_amount;
use std::asset::transfer;
use std::asset::mint_to;
use std::block::height;

use vrf_abi::VRF;

pub use ::contract_types::*;
use ::helpers::*;
use ::events::*;

type GameId = u32;
type Amount = u64;
type RollIndex = u32;


pub struct PayoutConfig {
    two_payout_multiplier: u64,
    three_payout_multiplier: u64,
    four_payout_multiplier: u64,
    five_payout_multiplier: u64,
    six_payout_multiplier: u64,
    seven_payout_multiplier: u64,
    eight_payout_multiplier: u64,
    nine_payout_multiplier: u64,
    ten_payout_multiplier: u64,
    eleven_payout_multiplier: u64,
    twelve_payout_multiplier: u64,
}

impl PayoutConfig {
    pub fn calculate_payout(self, principal: u64, roll: Roll) -> u64 {
        principal * self.multiplier_for_roll(roll)
    }

    fn multiplier_for_roll(self, roll: Roll) -> u64 {
        match roll {
            Roll::Two => self.two_payout_multiplier,
            Roll::Three => self.three_payout_multiplier,
            Roll::Four => self.four_payout_multiplier,
            Roll::Five => self.five_payout_multiplier,
            Roll::Six => self.six_payout_multiplier,
            Roll::Seven => self.seven_payout_multiplier,
            Roll::Eight => self.eight_payout_multiplier,
            Roll::Nine => self.nine_payout_multiplier,
            Roll::Ten => self.ten_payout_multiplier,
            Roll::Eleven => self.eleven_payout_multiplier,
            Roll::Twelve => self.twelve_payout_multiplier,
        }
    }
}

storage {
    /// History of rolls for each game
    roll_history: StorageMap<GameId, StorageVec<Roll>> = StorageMap {},
    /// Current roll of the active game
    roll_index: RollIndex = 0,
    /// next roll block height
    next_roll_block_height: Option<u32> = None,
    /// Number of blocks between rolls
    roll_frequency: u32 = 1,

    /// ID of the VRF contract to use for randomness
    vrf_contract_id: b256 = 0x0000000000000000000000000000000000000000000000000000000000000000,
    /// Asset ID of the chips used for betting
    chip_asset_id: AssetId = AssetId::zero(),
    /// Current game ID
    current_game_id: GameId = 0,
    /// Bets placed by (game_id, identity, roll) -> Vec<(bet, amount, roll_index)>
    bets: StorageMap<(GameId, Identity, Roll), StorageVec<(Bet, Amount, RollIndex)>> = StorageMap {},
    /// Straps to be rewarded for the current game when it ends, and the cost in the chip asset per strap
    strap_rewards: StorageMap<GameId, StorageVec<(Roll, Strap, u64)>> = StorageMap {},
    /// Triggers to add modifiers to shop, and whether they have been triggered this game
    /// 1. Roll that triggers the modifier
    /// 2. Roll that will add the modifier once purchased
    /// 3. Modifier to add
    /// 4. Whether it has been triggered this game
    modifier_triggers: StorageVec<(Roll, Roll, Modifier, bool)> = StorageVec {},
    /// Prices for each modifier
    /// 1. Price in chips
    /// 2. Whether it was purchased this game
    /// If the modifier was purchased last time it was available, the price will double next time it is available
    modifier_prices: StorageMap<Modifier, (u64, bool)> = StorageMap {},
    // Active modifiers for the current game
    active_modifiers: StorageMap<GameId, StorageVec<(Roll, Modifier, RollIndex)>> = StorageMap {},


    /// Total chips in the house pot
    house_pot: u64 = 0,
    /// Total chips owed to players (to ensure solvency)
    chips_owed: u64 = 0,
    /// Max owed percentage
    max_owed_percentage: u64 = 90,

    // Payout configuration
    payouts: PayoutConfig = PayoutConfig {
        two_payout_multiplier: 6,
        three_payout_multiplier: 5,
        four_payout_multiplier: 4,
        five_payout_multiplier: 3,
        six_payout_multiplier: 2,
        seven_payout_multiplier: 0,
        eight_payout_multiplier: 2,
        nine_payout_multiplier: 3,
        ten_payout_multiplier: 4,
        eleven_payout_multiplier: 5,
        twelve_payout_multiplier: 6,
    },
}

abi Strapped {
    /// Initialize the contract with the VRF contract ID, chip asset ID, and roll frequency
    #[storage(write)]
    fn initialize(vrf_contract_id: b256, chip_asset_id: AssetId, roll_frequency: u32);

    #[storage(read)]
    fn next_roll_height() -> Option<u32>;

    /// Roll the dice and process the results
    #[storage(read, write)]
    fn roll_dice();

    /// Get the history of rolls for the current game
    #[storage(read)]
    fn roll_history() -> Vec<Roll>;
    #[storage(read)]
    fn roll_history_for_game(game_id: GameId) -> Vec<Roll>;

    /// Place a bet on a specific roll with a specific bet type and amount
    #[storage(read, write), payable]
    fn place_bet(roll: Roll, bet: Bet, amount: u64);

    /// Get the caller's bets for a specific roll in the current game
    #[storage(read)]
    fn get_my_bets(roll: Roll) -> Vec<(Bet, u64, RollIndex)>;
    #[storage(read)]
    fn get_my_bets_for_game(game_id: GameId) -> Vec<(Roll, Vec<(Bet, Amount, RollIndex)>)>;

    /// Get the current game ID
    #[storage(read)]
    fn current_game_id() -> GameId;

    /// Claim rewards for a specific past game
    #[storage(read, write)]
    fn claim_rewards(game_id: GameId, enabled_modifiers: Vec<(Roll, Modifier)>);

    /// Fund the house pot with chips
    #[storage(read, write), payable]
    fn fund();

    /// Get the straps to be rewarded for the current game
    #[storage(read)]
    fn strap_rewards() -> Vec<(Roll, Strap, u64)>;
    #[storage(read)]
    fn strap_rewards_for_game(game_id: GameId) -> Vec<(Roll, Strap, u64)>;

    /// Get the chip asset id used for betting
    #[storage(read)]
    fn current_chip_asset_id() -> AssetId;

    /// Get the configured VRF contract id
    #[storage(read)]
    fn current_vrf_contract_id() -> b256;

    /// Get the modifier triggers
    #[storage(read)]
    fn modifier_triggers() -> Vec<(Roll, Roll, Modifier, bool)>;

    /// Purchase a modifier that has been triggered
    #[storage(read, write), payable]
    fn purchase_modifier(roll: Roll, modifier: Modifier);

    /// Get the active modifiers for the current game
    #[storage(read)]
    fn active_modifiers() -> Vec<(Roll, Modifier, RollIndex)>;
    #[storage(read)]
    fn active_modifiers_for_game(game_id: GameId) -> Vec<(Roll, Modifier, RollIndex)>;

    /// Get payout configuration
    #[storage(read)]
    fn payouts() -> PayoutConfig;
}

impl Strapped for Contract {
    #[storage(write)]
    fn initialize(vrf_contract_id: b256, chip_asset_id: AssetId, roll_frequency: u32) {
        storage.vrf_contract_id.write(vrf_contract_id);
        storage.chip_asset_id.write(chip_asset_id);
        storage.roll_frequency.write(roll_frequency);
        let current_height = height();
        storage.next_roll_block_height.write(Some(current_height + roll_frequency));
        log_initialized_event(vrf_contract_id, chip_asset_id, roll_frequency, current_height);
    }

    #[storage(read)]
    fn next_roll_height() -> Option<u32> {
        storage.next_roll_block_height.read()
    }

    #[storage(read, write)]
    fn roll_dice() {
        let roll_height = if let Some(h) = storage.next_roll_block_height.read() {
            let current_height = height();
            require(current_height >= h, "Too early to roll the dice");
            h
        } else {
            require(false, "must initialize contract before rolling");
            0
        };
        let rng_contract_id = storage.vrf_contract_id.read();
        let current_game_id = storage.current_game_id.read();
        let old_roll_index = storage.roll_index.read();
        let new_roll_index = old_roll_index + 1;
        storage.roll_index.write(new_roll_index);
        let rng_abi = abi(VRF, rng_contract_id);
        let random_number = rng_abi.get_random(roll_height);
        let roll = u64_to_roll(random_number);
        storage.roll_history.get(current_game_id).push(roll);
        log_roll_event(current_game_id, new_roll_index, roll);
        match roll {
            Roll::Seven => {
                let new_game_id = current_game_id + 1;
                storage.current_game_id.write(new_game_id);
                let new_straps = generate_straps(random_number);
                storage.roll_index.write(0);
                storage.modifier_triggers.clear();
                let new_modifiers = modifier_triggers_for_roll(random_number);
                for (roll, strap, cost) in new_straps.iter() {
                    storage.strap_rewards.get(new_game_id).push((roll, strap, cost));
                }
                for (trigger_roll, modifier_roll, modifier) in new_modifiers.iter() {
                    storage.modifier_triggers.push((trigger_roll, modifier_roll, modifier, false));
                }
                log_new_game_event(new_game_id, new_straps, new_modifiers);
            }
            _ => {
                let modifier_triggers = storage.modifier_triggers.load_vec();
                let mut index = 0;
                for (trigger_roll, modifier_roll, modifier, triggered) in modifier_triggers.iter() {
                    if !triggered && trigger_roll == roll {
                        storage.modifier_triggers.set(index, (trigger_roll, modifier_roll, modifier, true));
                        log_modifier_triggered(current_game_id, new_roll_index, roll, modifier_roll, modifier);
                    }
                    index += 1;
                }
            }
        }
        // set next roll block height to 10 blocks in the future
        let frequency = storage.roll_frequency.read();
        let next_height = roll_height + frequency;
        storage.next_roll_block_height.write(Some(next_height));
    }

    #[storage(read)]
    fn roll_history() -> Vec<Roll> {
        let current_game_id = storage.current_game_id.read();
        storage.roll_history.get(current_game_id).load_vec()
    }

    #[storage(read)]
    fn roll_history_for_game(game_id: GameId) -> Vec<Roll> {
        storage.roll_history.get(game_id).load_vec()
    }

    #[storage(read, write), payable]
    fn place_bet(roll: Roll, bet: Bet, amount: u64) {
        // check
        match bet {
            Bet::Chip => {
                let chip_asset_id = storage.chip_asset_id.read();
                require(msg_asset_id() == chip_asset_id, "Must bet with chips");

            },
            Bet::Strap(strap) => {
                let strap_sub_id = strap.into_sub_id();
                let contract_id = ContractId::this();
                let asset_id = AssetId::new(contract_id, strap_sub_id);
                require(msg_asset_id() == asset_id, "Must bet with the correct strap");

            }
        }
        require(msg_amount() == amount, "Must send the correct amount of chips");
        let caller = match msg_sender() {
            Ok(id) => id,
            Err(_) => {
                require(false, "bet placement must originate from a known sender");
                return;
            }
        };
        let current_game_id = storage.current_game_id.read();
        let roll_index = storage.roll_index.read();
        let key = (current_game_id, caller, roll);
        storage.bets.get(key).push((bet, amount, roll_index));
        match bet {
            Bet::Chip => {
                log_place_chip_bet_event(current_game_id, roll_index, caller, roll, amount);
            },
            Bet::Strap(strap) => {
                log_place_strap_bet_event(current_game_id, roll_index, caller, roll, strap, amount);
            }
        }
    }

    #[storage(read)]
    fn get_my_bets(roll: Roll) -> Vec<(Bet, Amount, RollIndex)> {
        let caller = match msg_sender() {
            Ok(id) => id,
            Err(_) => {
                require(false, "querying current bets requires a known sender");
                return Vec::new();
            }
        };
        let key = (storage.current_game_id.read(), caller, roll);
        storage.bets.get(key).load_vec()
    }

    #[storage(read)]
    fn get_my_bets_for_game(game_id: GameId) -> Vec<(Roll, Vec<(Bet, Amount, RollIndex)>)> {
        let caller = match msg_sender() {
            Ok(id) => id,
            Err(_) => {
                require(false, "querying historical bets requires a known sender");
                return Vec::new();
            }
        };
        let mut result: Vec<(Roll, Vec<(Bet, Amount, RollIndex)>)> = Vec::new();
        result.push((
            Roll::Two,
            storage
                .bets
                .get((game_id, caller, Roll::Two))
                .load_vec(),
        ));
        result.push((
            Roll::Three,
            storage
                .bets
                .get((game_id, caller, Roll::Three))
                .load_vec(),
        ));
        result.push((
            Roll::Four,
            storage
                .bets
                .get((game_id, caller, Roll::Four))
                .load_vec(),
        ));
        result.push((
            Roll::Five,
            storage
                .bets
                .get((game_id, caller, Roll::Five))
                .load_vec(),
        ));
        result.push((
            Roll::Six,
            storage
                .bets
                .get((game_id, caller, Roll::Six))
                .load_vec(),
        ));
        result.push((
            Roll::Seven,
            storage
                .bets
                .get((game_id, caller, Roll::Seven))
                .load_vec(),
        ));
        result.push((
            Roll::Eight,
            storage
                .bets
                .get((game_id, caller, Roll::Eight))
                .load_vec(),
        ));
        result.push((
            Roll::Nine,
            storage
                .bets
                .get((game_id, caller, Roll::Nine))
                .load_vec(),
        ));
        result.push((
            Roll::Ten,
            storage
                .bets
                .get((game_id, caller, Roll::Ten))
                .load_vec(),
        ));
        result.push((
            Roll::Eleven,
            storage
                .bets
                .get((game_id, caller, Roll::Eleven))
                .load_vec(),
        ));
        result.push((
            Roll::Twelve,
            storage
                .bets
                .get((game_id, caller, Roll::Twelve))
                .load_vec(),
        ));
        result
    }

    #[storage(read)]
    fn current_game_id() -> GameId {
        storage.current_game_id.read()
    }

    #[storage(read, write)]
    fn claim_rewards(game_id: GameId, enabled_modifiers: Vec<(Roll, Modifier)>) {
        let current_game_id = storage.current_game_id.read();
        require(game_id < current_game_id, "Can only claim rewards for past games");
        let identity = match msg_sender() {
            Ok(id) => id,
            Err(_) => {
                require(false, "claiming rewards requires a known sender");
                return;
            }
        };
        let rolls = storage.roll_history.get(game_id).load_vec();
        let mut total_chips_winnings = 0_u64;
        let mut index = 0;
        let mut rewards: Vec<(Strap, u64)> = Vec::new();
        for roll in rolls.iter() {
            let bets = storage.bets.get((game_id, identity, roll)).load_vec();
            let mut strap_indices_to_remove: Vec<u64> = Vec::new();
            for (bet, amount, roll_index) in bets.iter() {
                if roll_index <= index {
                    match bet {
                        Bet::Chip => {
                            let straps = storage.strap_rewards.get(game_id).load_vec();
                            let roll_rewards = rewards_for_roll(straps, roll, amount);
                            for (sub_id, amount) in roll_rewards.iter() {
                                // rewards.push((sub_id, amount));
                                let mut i = 0;
                                let mut found = false;
                                while i < rewards.len() {
                                    if let Some((existing_id, existing_amount)) = rewards.get(i)
                                    {
                                        if existing_id == sub_id {
                                            rewards.set(i, (existing_id, existing_amount + amount));
                                            found = true;
                                        }
                                    } else {
                                        require(false, "reward aggregation encountered an invalid index");
                                        break;
                                    }
                                    i += 1;
                                }
                                if !found {
                                    rewards.push((sub_id, amount));
                                }
                            }
                            let bet_winnings = storage.payouts.read().calculate_payout(amount, roll);
                            total_chips_winnings += bet_winnings; 
                        },
                        Bet::Strap(strap) => {
                            let Strap { level, kind, modifier } = strap;
                            let new_level = saturating_succ(level);
                            let active_modifiers = storage.active_modifiers.get(game_id).load_vec();
                            let modifier_for_roll = modifier_for_roll(active_modifiers, roll, roll_index, enabled_modifiers).unwrap_or(modifier);
                            let new_strap = Strap::new(new_level, kind, modifier_for_roll);
                            let contract_id = ContractId::this();
                            let mut i = 0;
                            let mut found = false; 
                                while i < rewards.len() {
                                    if let Some((existing_id, existing_amount)) = rewards.get(i)
                                    {
                                        if existing_id == new_strap {
                                            rewards.set(i, (existing_id, existing_amount + amount));
                                            found = true;
                                        }
                                    } else {
                                        require(false, "strap aggregation encountered an invalid index");
                                        break;
                                    }
                                    i += 1;
                                }
                            if !found {
                                rewards.push((new_strap, amount));
                            }
                        }
                    }
                }
            }
            index += 1;
        }
        // clear all bets for this game
    
        storage.bets.get((game_id, identity, Roll::Two)).clear();
        storage.bets.get((game_id, identity, Roll::Three)).clear();
        storage.bets.get((game_id, identity, Roll::Four)).clear();
        storage.bets.get((game_id, identity, Roll::Five)).clear();
        storage.bets.get((game_id, identity, Roll::Six)).clear();
        storage.bets.get((game_id, identity, Roll::Seven)).clear();
        storage.bets.get((game_id, identity, Roll::Eight)).clear();
        storage.bets.get((game_id, identity, Roll::Nine)).clear();
        storage.bets.get((game_id, identity, Roll::Ten)).clear();
        storage.bets.get((game_id, identity, Roll::Eleven)).clear();
        storage.bets.get((game_id, identity, Roll::Twelve)).clear();

        if total_chips_winnings > 0 || rewards.len() > 0 {
            let chip_asset_id = storage.chip_asset_id.read();
            if total_chips_winnings > 0 {
               transfer(identity, chip_asset_id, total_chips_winnings);
            }
            for (strap, amount) in rewards.iter() {
                let sub_id = strap.into_sub_id();
                mint_to(identity, sub_id, amount);
            }
            log_claim_rewards_event(game_id, identity, enabled_modifiers, total_chips_winnings, rewards);
        } else {
            require(false, "No winnings to claim");
        }
    }

    #[storage(read, write), payable]
    fn fund() {
        let chip_asset_id = storage.chip_asset_id.read();
        require(msg_asset_id() == chip_asset_id, "Must fund with chips");
        let amount = msg_amount();
        require(amount > 0, "Must send some amount to fund the house pot");
        storage.house_pot.write(storage.house_pot.read() + amount);
        let funder = match msg_sender() {
            Ok(id) => id,
            Err(_) => {
                require(false, "fund calls require a known sender");
                return;
            }
        };
        log_fund_pot_event(amount, funder);
    }

    #[storage(read)]
    fn strap_rewards() -> Vec<(Roll, Strap, u64)> {
        let current_game_id = storage.current_game_id.read();
        storage.strap_rewards.get(current_game_id).load_vec()
    }

    #[storage(read)]
    fn strap_rewards_for_game(game_id: GameId) -> Vec<(Roll, Strap, u64)> {
        storage.strap_rewards.get(game_id).load_vec()
    }

    #[storage(read)]
    fn current_chip_asset_id() -> AssetId {
        storage.chip_asset_id.read()
    }

    #[storage(read)]
    fn current_vrf_contract_id() -> b256 {
        storage.vrf_contract_id.read()
    }

    #[storage(read)]
    fn modifier_triggers() -> Vec<(Roll, Roll, Modifier, bool)> {
        storage.modifier_triggers.load_vec()
    }

    #[storage(read, write), payable]
    fn purchase_modifier(expected_roll: Roll, expected_modifier: Modifier) {
        let mut is_triggered = false;
        for (_, roll, modifier, triggered) in storage.modifier_triggers.load_vec().iter() {
            if roll == expected_roll && modifier == expected_modifier && triggered {
                is_triggered = true;
                break;
            }
        };
        require(is_triggered, "Modifier not available for purchase");
        let (price, _) = storage.modifier_prices.get(expected_modifier).try_read().unwrap_or((1, false));
        let chip_asset_id = storage.chip_asset_id.read();
        require(msg_asset_id() == chip_asset_id, "Must purchase with chips");
        let amount = msg_amount();
        require(amount >= price, "Must send the correct amount of chips");
        let roll_index = storage.roll_index.read();
        let game_id = storage.current_game_id.read();
        storage.active_modifiers.get(game_id).push((expected_roll, expected_modifier, roll_index));
        let purchaser = match msg_sender() {
            Ok(id) => id,
            Err(_) => {
                require(false, "modifier purchases require a known sender");
                return;
            }
        };
        log_purchase_modifier_event(expected_roll, expected_modifier, purchaser);
    }

    #[storage(read)]
    fn active_modifiers() -> Vec<(Roll, Modifier, RollIndex)> {
        let game_id = storage.current_game_id.read();
        storage.active_modifiers.get(game_id).load_vec()
    }

    #[storage(read)]
    fn active_modifiers_for_game(game_id: GameId) -> Vec<(Roll, Modifier, RollIndex)> {
        storage.active_modifiers.get(game_id).load_vec()
    }

    #[storage(read)]
    fn payouts() -> PayoutConfig {
        storage.payouts.read()
    }
}

fn six_payout(principal: u64) -> u64 {
    principal * 2
}

fn eight_payout(principal: u64) -> u64 {
    principal * 2
}

fn generate_straps(seed: u64) -> Vec<(Roll, Strap, u64)> {
    let mut straps: Vec<(Roll, Strap, u64)> = Vec::new();
    let mut multiple = 1;
    while seed % multiple == 0 && seed != 0{
        let inner = seed / multiple;
        let strap = u64_to_strap(inner);
        let slot = u64_to_slot(inner);
        let cost = strap_to_cost(strap);
        straps.push((slot, strap, cost));
        multiple = multiple * 2;
    }
    straps
}

fn u64_to_strap(num: u64) -> Strap {
    let level = 1;
    let modifier = Modifier::Nothing;
    let modulo = num % 141;
    let kind = if modulo < 20 {
        StrapKind::Shirt // weight 20
    } else if modulo < 40 {
        StrapKind::Pants // weight 20
    } else if modulo < 60 {
        StrapKind::Shoes // weight 20
    } else if modulo < 70 {
        StrapKind::Hat // weight 10
    } else if modulo < 80 {
        StrapKind::Glasses // weight 10
    } else if modulo < 90 {
        StrapKind::Watch // weight 10
    } else if modulo < 100 {
        StrapKind::Ring // weight 10
    } else if modulo < 105 {
        StrapKind::Necklace // weight 5
    } else if modulo < 110 {
        StrapKind::Earring // weight 5
    } else if modulo < 115 {
        StrapKind::Bracelet // weight 5
    } else if modulo < 120 {
        StrapKind::Tattoo // weight 5
    } else if modulo < 125 {
        StrapKind::Skirt // weight 5
    } else if modulo < 130 {
        StrapKind::Piercing // weight 5
    } else if modulo < 135 {
        StrapKind::Coat // weight 2
    } else if modulo < 137 {
        StrapKind::Scarf // weight 2
    } else if modulo < 139 {
        StrapKind::Gloves // weight 2
    } else if modulo < 141 {
        StrapKind::Gown // weight 2
    } else {
        StrapKind::Belt // weight 1
    };
    Strap::new(level, kind, modifier)
}

// two -> twelve, never seven
fn u64_to_slot(num: u64) -> Roll {
    let modulo = num % 10;

    match modulo {
        0 => Roll::Two,
        1 => Roll::Three,
        2 => Roll::Four,
        3 => Roll::Five,
        4 => Roll::Six,
        5 => Roll::Eight,
        6 => Roll::Nine,
        7 => Roll::Ten,
        8 => Roll::Eleven,
        _ => Roll::Twelve,
    }
}

fn strap_to_cost(strap: Strap) -> u64 {
    match strap.kind {
        StrapKind::Shirt => 10,
        StrapKind::Pants => 10,
        StrapKind::Shoes => 10,
        StrapKind::Dress => 10,
        StrapKind::Hat => 20,
        StrapKind::Glasses => 20,
        StrapKind::Watch => 20,
        StrapKind::Ring => 20,
        StrapKind::Necklace => 50,
        StrapKind::Earring => 50,
        StrapKind::Bracelet => 50,
        StrapKind::Tattoo => 50,
        StrapKind::Skirt => 50,
        StrapKind::Piercing => 50,
        StrapKind::Coat => 100,
        StrapKind::Scarf => 100,
        StrapKind::Gloves => 100,
        StrapKind::Gown => 100,
        StrapKind::Belt => 200,
    }
}

fn rewards_for_roll(available_straps: Vec<(Roll, Strap, u64)>, roll: Roll, bet_amount: u64) -> Vec<(Strap, u64)> {
    let mut rewards: Vec<(Strap, u64)> = Vec::new();
    for (reward_roll, strap, cost) in available_straps.iter() {
        if reward_roll == roll {
            let amount = bet_amount / cost;
            rewards.push((strap, amount));
        }
    }
    rewards
}

fn modifier_triggers_for_roll(roll: u64) -> Vec<(Roll, Roll, Modifier)> {
    let mut two = false;
    let mut three = false;
    let mut four = false;
    let mut five = false;
    let mut six = false;
    let mut seven = false;
    let mut eight = false;
    let mut nine = false;
    let mut ten = false;
    let mut eleven = false;
    let mut twelve = false;
    let mut triggers = Vec::new();
    let mut multiple = 1;
    while roll % multiple == 0 && roll != 0 {
        let inner = roll / multiple;
        let (modifier_roll, modifier) = u64_to_modifier(inner);
        let trigger_roll = u64_to_trigger_roll(inner);
        let add = match modifier_roll {
            Roll::Two => {
                let was = two;
                two = true;
                !was
            },
            Roll::Three => {
                let was = three;
                three = true;
                !was
            },
            Roll::Four => {
                let was = four;
                four = true;
                !was
            },
            Roll::Five => {
                let was = five;
                five = true;
                !was
            },
            Roll::Six => {
                let was = six;
                six = true;
                !was
            },
            Roll::Seven => {
                let was = seven;
                seven = true;
                !was
            },
            Roll::Eight => {
                let was = eight;
                eight = true;
                !was
            },
            Roll::Nine => {
                let was = nine;
                nine = true;
                !was
            },
            Roll::Ten => {
                let was = ten;
                ten = true;
                !was
            },
            Roll::Eleven => {
                let was = eleven;
                eleven = true;
                !was
            },
            Roll::Twelve => {
                let was = twelve;
                twelve = true;
                !was
            },
        };
        if add {
            triggers.push((trigger_roll, modifier_roll, modifier));
        }
        multiple = multiple * 3;
    }
    triggers
}

fn u64_to_modifier(num: u64) -> (Roll, Modifier) {
    let modulo = num % 11;

    match modulo {
        0 => (Roll::Two, Modifier::Burnt),
        1 => (Roll::Three, Modifier::Lucky),
        2 => (Roll::Four, Modifier::Holy),
        3 => (Roll::Five, Modifier::Holey),
        4 => (Roll::Six, Modifier::Scotch),
        8 => (Roll::Seven, Modifier::Evil),
        5 => (Roll::Eight, Modifier::Soaked),
        6 => (Roll::Nine, Modifier::Moldy),
        7 => (Roll::Ten, Modifier::Starched),
        9 => (Roll::Eleven, Modifier::Groovy),
        _ => (Roll::Twelve, Modifier::Delicate),
    }
}

fn u64_to_trigger_roll(num: u64) -> Roll {
    let modulo = num % 4;

    match modulo {
        0 => Roll::Two,
        1 => Roll::Three,
        2 => Roll::Eleven,
        _ => Roll::Twelve,
    }
}

fn modifier_for_roll(active_modifiers: Vec<(Roll, Modifier, RollIndex)>, roll: Roll, roll_index: RollIndex, enabled_modifiers: Vec<(Roll, Modifier)>) -> Option<Modifier> {
    for (modifier_roll, modifier, activated_roll_index) in active_modifiers.iter() {
        if modifier_roll == roll && activated_roll_index <= roll_index {
            let mut contains_modifier = false;
            for (enabled_roll, enabled_modifier) in enabled_modifiers.iter() {
                if enabled_roll == modifier_roll && enabled_modifier == modifier {
                    contains_modifier = true;
                    break;
                }
            }
            if !contains_modifier {
                return None;
            } else {
                return Some(modifier);
            }
        }
    }
    None
}
