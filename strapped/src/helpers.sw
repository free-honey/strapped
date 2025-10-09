library;

use ::contract_types::*;

// Convert a u64 to a Roll based on 2-d6 probabilities
// i.e. 7 is the most likely roll, 2 and 12 are the least likely
// starting at the bottom give 1/36 chance to roll a 2, 2/36 chance to roll a 3, etc
pub fn u64_to_roll(num: u64) -> Roll {
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

pub fn saturating_succ(level: u8) -> u8 {
    if level == 255 {
        255
    } else {
        level + 1
    }
}
