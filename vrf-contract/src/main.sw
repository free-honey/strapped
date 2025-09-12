contract;

use vrf_abi::VRF;

storage {
    number: u64 = 42,
}

impl VRF for Contract {
    #[storage(read)]
    fn get_random() -> u64 {
        storage.number.read()
    }

    #[storage(write)]
    fn set_number(num: u64) {
        storage.number.write(num);
    }
}
