contract;

mod contract_types;

use std::storage::storage_vec::*;
use vrf_abi::VRF;
use ::contract_types::*;


storage {
    last_roll: Roll = Roll::Seven,
    vrf_contract_id: b256 = 0x0000000000000000000000000000000000000000000000000000000000000000,

    // two_bets: StorageMap<(Identity, Bet), u64> = StorageMap::<(Identity, Bet), u64> {},
    // three_bets: StorageMap<(Identity, Bet), u64> = StorageMap::<(Identity, Bet), u64> {},
    // four_bets: StorageMap<(Identity, Bet), u64> = StorageMap::<(Identity, Bet), u64> {},
    // five_bets: StorageMap<(Identity, Bet), u64> = StorageMap::<(Identity, Bet), u64> {},
    six_bets: StorageMap<Identity, StorageVec<(Bet, u64)>> = StorageMap::<Identity, StorageVec<(Bet, u64)>> {},
    seven_bets: StorageMap<(Identity, Bet), u64> = StorageMap::<(Identity, Bet), u64> {},
    // eight_bets: StorageMap<(Identity, Bet), u64> = StorageMap::<(Identity, Bet), u64> {},
    // nine_bets: StorageMap<(Identity, Bet), u64> = StorageMap::<(Identity, Bet), u64> {},
    // ten_bets: StorageMap<(Identity, Bet), u64> = StorageMap::<(Identity, Bet), u64> {},
    // eleven_bets: StorageMap<(Identity, Bet), u64> = StorageMap::<(Identity, Bet), u64> {},
    // twelve_bets: StorageMap<(Identity, Bet), u64> = StorageMap::<(Identity, Bet), u64> {},
}

abi Strapped {
    #[storage(read, write)]
    fn roll_dice();

    #[storage(read)]
    fn last_roll() -> Roll;

    #[storage(write)]
    fn set_vrf_contract_id(id: b256);

    #[storage(read, write)]
    fn place_bet(roll: Roll, bet: Bet, amount: u64);

    #[storage(read)]
    fn get_my_bets(roll: Roll) -> Vec<(Bet, u64)>;
}

impl Strapped for Contract {
    #[storage(read, write)]
    fn roll_dice() {
        let rng_contract_id = storage.vrf_contract_id.read();
        let rng_abi = abi(VRF, rng_contract_id);
        let random_number = rng_abi.get_random();
        let roll = u64_to_roll(random_number);
        storage.last_roll.write(roll)
    }

    #[storage(read)]
    fn last_roll() -> Roll {
        storage.last_roll.read()
    }

    #[storage(write)]
    fn set_vrf_contract_id(id: b256) {
        storage.vrf_contract_id.write(id);
    }

    #[storage(read, write)]
    fn place_bet(roll: Roll, bet: Bet, amount: u64) {
        let caller = msg_sender().unwrap();
        match roll {
            Roll::Six => {
                storage.six_bets.get(caller).push((bet, amount));
            },
            _ => {}
        }
    }

    #[storage(read)]
    fn get_my_bets(roll: Roll) -> Vec<(Bet, u64)> {
        let mut results = Vec::new();
        let caller = msg_sender().unwrap();
        match roll {
            Roll::Six => {
                let list = storage.six_bets.get(caller); 
                for entry in list.iter() {
                    results.push(entry.read());
                }
            },
            _ => {}
        }
        results
    }
}

// Convert a u64 to a Roll based on 2-d6 probabilities
// i.e. 7 is the most likely roll, 2 and 12 are the least likely
// starting at the bottom give 1/36 chance to roll a 2, 2/36 chance to roll a 3, etc
fn u64_to_roll(num: u64) -> Roll {
    let modulo = num % 36;

    if modulo == 0 {
        Roll::Two
    } else if modulo <= 2 {
        Roll::Three
    } else if modulo <= 5 {
        Roll::Four
    } else if modulo <= 9 {
        Roll::Five
    } else if modulo <= 14 {
        Roll::Six
    } else if modulo <= 20 {
        Roll::Seven
    } else if modulo <= 25 {
        Roll::Eight
    } else if modulo <= 29 {
        Roll::Nine
    } else if modulo <= 32 {
        Roll::Ten
    } else if modulo <= 34 {
        Roll::Eleven
    } else {
        Roll::Twelve
    }
}