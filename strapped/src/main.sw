contract;

pub mod contract_types;
pub mod helpers;

use std::storage::storage_vec::*;
use std::call_frames::msg_asset_id;
use std::context::msg_amount;
use std::asset::transfer;
use std::asset::mint_to;

use vrf_abi::VRF;

use ::contract_types::*;
use ::helpers::*;

type GameId = u64;
type Amount = u64;
type RollIndex = u64;

storage {
    /// History of rolls for each game
    roll_history: StorageMap<GameId, StorageVec<Roll>> = StorageMap {},
    /// Current roll of the active game
    roll_index: RollIndex = 0,
    /// ID of the VRF contract to use for randomness
    vrf_contract_id: b256 = 0x0000000000000000000000000000000000000000000000000000000000000000,
    /// Asset ID of the chips used for betting
    chip_asset_id: AssetId = AssetId::zero(),
    /// Current game ID
    current_game_id: GameId = 0,
    /// Bets placed by (game_id, identity, roll) -> Vec<(bet, amount, roll_index)>
    bets: StorageMap<(GameId, Identity, Roll), StorageVec<(Bet, Amount, RollIndex)>> = StorageMap {},
    /// Total chips in the house pot
    house_pot: u64 = 0,
    /// Straps to be rewarded for the current game when it ends
    strap_rewards: StorageVec<(Roll, Strap)> = StorageVec {},

}

abi Strapped {
    /// Roll the dice and process the results
    #[storage(read, write)]
    fn roll_dice();

    /// Get the history of rolls for the current game
    #[storage(read)]
    fn roll_history() -> Vec<Roll>;

    /// Set the VRF contract ID
    #[storage(write)]
    fn set_vrf_contract_id(id: b256);

    /// Set the chip asset ID
    #[storage(write)]
    fn set_chip_asset_id(id: AssetId);

    /// Place a bet on a specific roll with a specific bet type and amount
    #[storage(read, write), payable]
    fn place_bet(roll: Roll, bet: Bet, amount: u64);

    /// Get the caller's bets for a specific roll in the current game
    #[storage(read)]
    fn get_my_bets(roll: Roll) -> Vec<(Bet, u64, RollIndex)>;

    /// Get the current game ID
    #[storage(read)]
    fn current_game_id() -> GameId;

    /// Claim rewards for a specific past game
    #[storage(read, write)]
    fn claim_rewards(game_id: GameId);

    /// Fund the house pot with chips
    #[storage(read, write), payable]
    fn fund();

    /// Get the straps to be rewarded for the current game
    #[storage(read)]
    fn strap_rewards() -> Vec<(Roll, Strap)>;
}

impl Strapped for Contract {
    #[storage(read, write)]
    fn roll_dice() {
        let rng_contract_id = storage.vrf_contract_id.read();
        let rng_abi = abi(VRF, rng_contract_id);
        let random_number = rng_abi.get_random();
        let roll = u64_to_roll(random_number);
        let current_game_id = storage.current_game_id.read();
        let old_roll_index = storage.roll_index.read();
        storage.roll_index.write(old_roll_index + 1);
        match roll {
            Roll::Seven => {
                storage.current_game_id.write(current_game_id + 1);
                let new_straps = generate_straps(random_number);
                storage.strap_rewards.clear();
                storage.roll_index.write(0);
                for (roll, strap) in new_straps.iter() {
                    storage.strap_rewards.push((roll, strap));
                }
            }
            _ => {
                storage.roll_history.get(current_game_id).push(roll);
            }
        }
    }

    #[storage(read)]
    fn roll_history() -> Vec<Roll> {
        let current_game_id = storage.current_game_id.read();
        storage.roll_history.get(current_game_id).load_vec()
    }

    #[storage(write)]
    fn set_vrf_contract_id(id: b256) {
        storage.vrf_contract_id.write(id);
    }

    #[storage(write)]
    fn set_chip_asset_id(id: AssetId) {
        storage.chip_asset_id.write(id);
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
        let caller = msg_sender().unwrap();
        let current_game_id = storage.current_game_id.read();
        let roll_index = storage.roll_index.read();
        let key = (current_game_id, caller, roll);
        storage.bets.get(key).push((bet, amount, roll_index));
    }

    #[storage(read)]
    fn get_my_bets(roll: Roll) -> Vec<(Bet, Amount, RollIndex)> {
        let caller = msg_sender().unwrap();
        let key = (storage.current_game_id.read(), caller, roll);
        storage.bets.get(key).load_vec()
    }

    #[storage(read)]
    fn current_game_id() -> GameId {
        storage.current_game_id.read()
    }

    #[storage(read, write)]
    fn claim_rewards(game_id: GameId) {
        let current_game_id = storage.current_game_id.read();
        require(game_id < current_game_id, "Can only claim rewards for past games");
        let identity = msg_sender().unwrap();
        let rolls = storage.roll_history.get(game_id).load_vec();
        let mut total_chips_winnings = 0_u64;
        let mut index = 0;
        let mut rewards: Vec<SubId> = Vec::new();
        for roll in rolls.iter() {
            let bets = storage.bets.get((game_id, identity, roll)).load_vec();
            let mut received_reward_for_roll = false;
            for (bet, amount, roll_index) in bets.iter() {
                if roll_index <= index {
                    match bet {
                        Bet::Chip => {
                            if !received_reward_for_roll {
                                let roll_rewards = rewards_for_roll(storage.strap_rewards.load_vec(), roll);
                                for sub_id in roll_rewards.iter() {
                                    rewards.push(sub_id);
                                    received_reward_for_roll = true;
                                }
                            }
                            // let roll_rewards = rewards_for_roll(storage.strap_rewards.load_vec(), roll);
                            // for sub_id in roll_rewards.iter() {
                            //     rewards.push(sub_id);
                            // }
                            let bet_winnings = match roll {
                                Roll::Six => six_payout(amount),
                                Roll::Eight => eight_payout(amount),
                                _ => 0,
                            };
                            total_chips_winnings += bet_winnings; 
                            total_chips_winnings -= amount; 
                        },
                        Bet::Strap(strap) => {
                            // TODO: implement strap betting
                        }
                    }
                }
            }
            storage.bets.get((game_id, identity, Roll::Six)).clear();
            index += 1;
        }
        if total_chips_winnings > 0 {
            let chip_asset_id = storage.chip_asset_id.read();
            transfer(identity, chip_asset_id, total_chips_winnings);
            for sub_id in rewards.iter() {
                mint_to(identity, sub_id, 1);
            }
        
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
    }

    #[storage(read)]
    fn strap_rewards() -> Vec<(Roll, Strap)> {
        storage.strap_rewards.load_vec()
    }
}

fn six_payout(principal: u64) -> u64 {
    principal * 2
}

fn eight_payout(principal: u64) -> u64 {
    principal * 2
}

fn generate_straps(seed: u64) -> Vec<(Roll, Strap)> {
    let mut straps: Vec<(Roll, Strap)> = Vec::new();
    let roll = Roll::Eight;
    let strap = Strap::new(1, StrapKind::Shirt, Modifier::Nothing);
    straps.push((roll, strap));
    straps
}

fn rewards_for_roll(available_straps: Vec<(Roll, Strap)>, roll: Roll) -> Vec<SubId> {
    let mut rewards: Vec<SubId> = Vec::new();
    for (reward_roll, strap) in available_straps.iter() {
        if reward_roll == roll {
            let sub_id = strap.into_sub_id();
            rewards.push(sub_id);
        }
    }
    rewards
}