library;

abi VRF {
    #[storage(read)]
    fn get_random(block_height: u32) -> u64;
}

