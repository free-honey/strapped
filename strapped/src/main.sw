contract;

pub mod contract_types;
pub mod helpers;

use std::storage::storage_vec::*;
use std::call_frames::msg_asset_id;
use std::context::msg_amount;
use std::asset::transfer;

use vrf_abi::VRF;

use ::contract_types::*;
use ::helpers::*;

type GameId = u64;
type Amount = u64;
type RollIndex = u64;

storage {
    roll_history: StorageMap<GameId, StorageVec<Roll>> = StorageMap {},
    roll_index: RollIndex = 0,
    vrf_contract_id: b256 = 0x0000000000000000000000000000000000000000000000000000000000000000,
    chip_asset_id: AssetId = AssetId::zero(),
    current_game_id: GameId = 0,
    bets: StorageMap<(GameId, Identity, Roll), StorageVec<(Bet, Amount, RollIndex)>> = StorageMap {},
    house_pot: u64 = 0,

}

abi Strapped {
    #[storage(read, write)]
    fn roll_dice();

    #[storage(read)]
    fn roll_history() -> Vec<Roll>;

    #[storage(write)]
    fn set_vrf_contract_id(id: b256);

    #[storage(write)]
    fn set_chip_asset_id(id: AssetId);

    #[storage(read, write), payable]
    fn place_bet(roll: Roll, bet: Bet, amount: u64);

    #[storage(read)]
    fn get_my_bets(roll: Roll) -> Vec<(Bet, u64, RollIndex)>;

    #[storage(read)]
    fn current_game_id() -> GameId;

    #[storage(read, write)]
    fn claim_rewards(game_id: GameId);

    #[storage(read, write), payable]
    fn fund();
}

impl Strapped for Contract {
    #[storage(read, write)]
    fn roll_dice() {
        let rng_contract_id = storage.vrf_contract_id.read();
        let rng_abi = abi(VRF, rng_contract_id);
        let random_number = rng_abi.get_random();
        let roll = u64_to_roll(random_number);
        let current_game_id = storage.current_game_id.read();
        match roll {
            Roll::Seven => {
                storage.current_game_id.write(current_game_id + 1);
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
                require(msg_amount() == amount, "Must send the correct amount of chips");
            },
            Bet::Strap(strap) => {
            }
        }
        let caller = msg_sender().unwrap();
        let current_game_id = storage.current_game_id.read();
        let roll_index = storage.roll_index.read();
        match roll {
            Roll::Six => {
                let key = (current_game_id, caller, Roll::Six);
                storage.bets.get(key).push((bet, amount, roll_index));
            },
            _ => {}
        }
    }

    #[storage(read)]
    fn get_my_bets(roll: Roll) -> Vec<(Bet, Amount, RollIndex)> {
        let caller = msg_sender().unwrap();
        match roll {
            Roll::Six => {
                let key = (storage.current_game_id.read(), caller, Roll::Six);
                storage.bets.get(key).load_vec()
            },
            _ => {
                Vec::new()
            }
        }
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
        // TODO: handle other rolls besides Six
        let rolls = storage.roll_history.get(game_id).load_vec();
        let mut total_chips_winnings = 0_u64;
        let mut index = 0;
        for roll in rolls.iter() {
            match roll {
                Roll::Six => {
                    let six_bets = storage.bets.get((game_id, identity, Roll::Six)).load_vec();
                    for (bet, amount, roll_index) in six_bets.iter() {
                        if roll_index < index{
                            match bet {
                                Bet::Chip => {
                                    total_chips_winnings += six_payout(amount);
                                    total_chips_winnings -= amount; 
                                },
                                Bet::Strap(strap) => {
                                    // TODO: implement strap betting
                                }
                            }
                        }
                    }
                },
                _ => {}
            }
            index += 1;
        }
        if total_chips_winnings > 0 {
            let chip_asset_id = storage.chip_asset_id.read();
            transfer(identity, chip_asset_id, total_chips_winnings);
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
}

fn six_payout(principal: u64) -> u64 {
    principal * 2
}
