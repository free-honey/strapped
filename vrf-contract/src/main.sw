contract;

use vrf_abi::VRF;

storage {
    number: u64 = 42,
}

impl VRF for Contract {
    #[storage(read)]
    fn get_random(_block_height: u32) -> u64 {
        storage.number.read()
    }

    #[storage(write)]
    fn set_number(num: u64) {
        storage.number.write(num);
    }
}
