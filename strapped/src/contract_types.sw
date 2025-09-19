library;

use std::storage::storage_vec::*;
use std::hash::*;
use std::array_conversions::b256::*;

pub enum Roll {
    Two: (),
    Three: (),
    Four: (),
    Five: (),
    Six: (),
    Seven: (),
    Eight: (),
    Nine: (),
    Ten: (),
    Eleven: (),
    Twelve: (),
}

pub enum StrapKind {
    Shirt: (),
    Pants: (),
    Shoes: (),
    Hat: (),
    Glasses: (),
    Watch: (),
    Ring: (),
    Necklace: (),
    Earring: (),
    Bracelet: (),
    Tattoo: (),
    Piercing: (),
    Coat: (),
    Scarf: (),
    Gloves: (),
    Belt: (),
}

pub enum Modifier {
    Nothing: (),
    Burnt: (),
    Lucky: (),
    Holy: (),
    Holey: (),
    Scotch: (),
    Soaked: (),
    Moldy: (),
    Starched: (),
    Evil: (),
}

pub struct Strap {
    pub level: u8,
    pub kind: StrapKind,
    pub modifier: Modifier,
}

impl Strap {
    pub fn new(level: u8, kind: StrapKind, modifier: Modifier) -> Strap {
        Strap {
            level,
            kind,
            modifier,
        }
    }
}

pub enum Bet {
    Chip: (),
    Strap: Strap
}

impl Hash for Roll {
    fn hash(self, ref mut state: Hasher) {
        match self {
            Roll::Two => { 2_u8.hash(state); }
            Roll::Three => { 3_u8.hash(state); }
            Roll::Four => { 4_u8.hash(state); }
            Roll::Five => { 5_u8.hash(state); }
            Roll::Six => { 6_u8.hash(state); }
            Roll::Seven => { 7_u8.hash(state); }
            Roll::Eight => { 8_u8.hash(state); }
            Roll::Nine => { 9_u8.hash(state); }
            Roll::Ten => { 10_u8.hash(state); }
            Roll::Eleven => { 11_u8.hash(state); }
            Roll::Twelve => { 12_u8.hash(state); }
        }
    }
}

impl PartialEq for Roll {
    fn eq(self, other: Roll) -> bool {
        match (self, other) {
            (Roll::Two, Roll::Two) => true,
            (Roll::Three, Roll::Three) => true,
            (Roll::Four, Roll::Four) => true,
            (Roll::Five, Roll::Five) => true,
            (Roll::Six, Roll::Six) => true,
            (Roll::Seven, Roll::Seven) => true,
            (Roll::Eight, Roll::Eight) => true,
            (Roll::Nine, Roll::Nine) => true,
            (Roll::Ten, Roll::Ten) => true,
            (Roll::Eleven, Roll::Eleven) => true,
            (Roll::Twelve, Roll::Twelve) => true,
            _ => false,
        }
    }
}

impl Hash for StrapKind {
    fn hash(self, ref mut state: Hasher) {
        match self {
            StrapKind::Shirt => { 0_u8.hash(state); }
            StrapKind::Pants => { 1_u8.hash(state); }
            StrapKind::Shoes => { 2_u8.hash(state); }
            StrapKind::Hat => { 3_u8.hash(state); }
            StrapKind::Glasses => { 4_u8.hash(state); }
            StrapKind::Watch => { 5_u8.hash(state); }
            StrapKind::Ring => { 6_u8.hash(state); }
            StrapKind::Necklace => { 7_u8.hash(state); }
            StrapKind::Earring => { 8_u8.hash(state); }
            StrapKind::Bracelet => { 9_u8.hash(state); }
            StrapKind::Tattoo => { 10_u8.hash(state); }
            StrapKind::Piercing => { 11_u8.hash(state); }
            StrapKind::Coat => { 12_u8.hash(state); }
            StrapKind::Scarf => { 13_u8.hash(state); }
            StrapKind::Gloves => { 14_u8.hash(state); }
            StrapKind::Belt => { 15_u8.hash(state); }
        }
    }
}

impl Hash for Modifier {
    fn hash(self, ref mut state: Hasher) {
        match self {
            Modifier::Nothing => { 0_u8.hash(state); }
            Modifier::Burnt => { 1_u8.hash(state); }
            Modifier::Lucky => { 2_u8.hash(state); }
            Modifier::Holy => { 3_u8.hash(state); }
            Modifier::Holey => { 4_u8.hash(state); }
            Modifier::Scotch => { 5_u8.hash(state); }
            Modifier::Soaked => { 6_u8.hash(state); }
            Modifier::Moldy => { 7_u8.hash(state); }
            Modifier::Starched => { 8_u8.hash(state); }
            Modifier::Evil => { 9_u8.hash(state); }
        }
    }
}

impl Hash for Strap {
    fn hash(self, ref mut state: Hasher) {
        self.level.hash(state);
        self.kind.hash(state);
        self.modifier.hash(state);
    }
}

impl Hash for Bet {
    fn hash(self, ref mut state: Hasher) {
        match self {
            Bet::Chip => {
                0_u8.hash(state);
            }
            Bet::Strap(strap) => {
                1_u8.hash(state);
                strap.hash(state);
            }
        }
    }
}

impl Strap {
    pub fn into_sub_id(self) -> SubId {
        let mut sub_id = [0; 32];
        sub_id[0] = self.level;
        sub_id[1] = match self.kind {
            StrapKind::Shirt => 0_u8,
            StrapKind::Pants => 1_u8,
            StrapKind::Shoes => 2_u8,
            StrapKind::Hat => 3_u8,
            StrapKind::Glasses => 4_u8,
            StrapKind::Watch => 5_u8,
            StrapKind::Ring => 6_u8,
            StrapKind::Necklace => 7_u8,
            StrapKind::Earring => 8_u8,
            StrapKind::Bracelet => 9_u8,
            StrapKind::Tattoo => 10_u8,
            StrapKind::Piercing => 11_u8,
            StrapKind::Coat => 12_u8,
            StrapKind::Scarf => 13_u8,
            StrapKind::Gloves => 14_u8,
            StrapKind::Belt => 15_u8,
        };
        sub_id[2] = match self.modifier {
            Modifier::Nothing => 0_u8,
            Modifier::Burnt => 1_u8,
            Modifier::Lucky => 2_u8,
            Modifier::Holy => 3_u8,
            Modifier::Holey => 4_u8,
            Modifier::Scotch => 5_u8,
            Modifier::Soaked => 6_u8,
            Modifier::Moldy => 7_u8,
            Modifier::Starched => 8_u8,
            Modifier::Evil => 9_u8,
        };
        b256::from_be_bytes(sub_id)
    }
}

