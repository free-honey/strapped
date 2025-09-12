library;

abi VRF {
    #[storage(read)]
    fn get_random() -> u64;

    #[storage(write)]
    fn set_number(num: u64);
}

