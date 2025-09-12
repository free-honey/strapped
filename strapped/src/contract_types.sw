library;

use std::hash::*;

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
    level: u8,
    kind: StrapKind,
    modifier: Modifier,
}

pub enum Bet {
    Base: (),
    Strap: Strap
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

impl Hash for Bet {
    fn hash(self, ref mut state: Hasher) {
        match self {
            Bet::Base => {
                0_u8.hash(state);
            }
            Bet::Strap(strap) => {
                1_u8.hash(state);
                strap.level.hash(state);
                strap.kind.hash(state);
                strap.modifier.hash(state);
            }
        }
    }
}

