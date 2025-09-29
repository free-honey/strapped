contract;

use vrf_abi::VRF;

storage {
    number: u64 = 42,
}

abi Set {
    #[storage(write)]
    fn set_number(num: u64);
}

impl VRF for Contract {
    #[storage(read)]
    fn get_random(_block_height: u32) -> u64 {
        storage.number.read()
    }
}

impl Set for Contract {
    #[storage(write)]
    fn set_number(num: u64) {
        storage.number.write(num);
    }
}
