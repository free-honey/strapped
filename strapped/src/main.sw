contract;

pub mod contract_types;
pub mod helpers;

use std::storage::storage_vec::*;
use std::call_frames::msg_asset_id;
use std::context::msg_amount;

use vrf_abi::VRF;

use ::contract_types::*;
use ::helpers::*;

type GameId = u64;

storage {
    roll_history: StorageMap<GameId, StorageVec<Roll>> = StorageMap {},
    vrf_contract_id: b256 = 0x0000000000000000000000000000000000000000000000000000000000000000,
    chip_asset_id: AssetId = AssetId::zero(),
    current_game: GameId = 0,
    bets: StorageMap<(GameId, Identity, Roll), StorageVec<(Bet, u64)>> = StorageMap {},
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
    fn get_my_bets(roll: Roll) -> Vec<(Bet, u64)>;

    #[storage(read)]
    fn current_game_id() -> GameId;

    #[storage(read, write)]
    fn claim_rewards(game_id: GameId);
}

impl Strapped for Contract {
    #[storage(read, write)]
    fn roll_dice() {
        let rng_contract_id = storage.vrf_contract_id.read();
        let rng_abi = abi(VRF, rng_contract_id);
        let random_number = rng_abi.get_random();
        let roll = u64_to_roll(random_number);
        let current_game = storage.current_game.read();
        match roll {
            Roll::Seven => {
                storage.current_game.write(current_game + 1);
            }
            _ => {
                storage.roll_history.get(current_game).push(roll);
            }
        }
    }

    #[storage(read)]
    fn roll_history() -> Vec<Roll> {
        let current_game = storage.current_game.read();
        storage.roll_history.get(current_game).load_vec()
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
        let current_game = storage.current_game.read();
        match roll {
            Roll::Six => {
                let key = (current_game, caller, Roll::Six);
                storage.bets.get(key).push((bet, amount));
            },
            _ => {}
        }
    }

    #[storage(read)]
    fn get_my_bets(roll: Roll) -> Vec<(Bet, u64)> {
        let caller = msg_sender().unwrap();
        match roll {
            Roll::Six => {
                let key = (storage.current_game.read(), caller, Roll::Six);
                storage.bets.get(key).load_vec()
            },
            _ => {
                Vec::new()
            }
        }
    }

    #[storage(read)]
    fn current_game_id() -> GameId {
        storage.current_game.read()
    }

    #[storage(read, write)]
    fn claim_rewards(game_id: GameId) {
       // TODO
    }
}
