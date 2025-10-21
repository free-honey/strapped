contract;

use vrf_abi::VRF;
use std::hash::*;
use std::bytes::Bytes;
use std::array_conversions::u64::*;

storage {
    entropy: u64 = 9876543210,
}

abi Entropy {
    #[storage(write)]
    fn set_entropy(num: u64);
}

impl VRF for Contract {
    #[storage(read)]
    fn get_random(block_height: u32) -> u64 {
        let entropy_sum = storage.entropy.read() + u64::from(block_height);
        let hash = sha256(entropy_sum);
        let bytes = Bytes::from(hash);
        let mut index = 0;
        let mut array = [0u8; 8];
        for byte in bytes.iter() {
            array[index] = byte;
            if index == 7 {
                break;
            } else {
                index += 1;
            }
        }
        u64::from_le_bytes(array)
    }
}

impl Entropy for Contract {
    #[storage(write)]
    fn set_entropy(entropy: u64) {
        storage.entropy.write(entropy);
    }
}
