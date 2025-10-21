use generated_abi::strapped_types::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    GameId: u32,
    RollIndex: u32,
    PotSize: u64,
    Rewards: Vec<(Roll, Strap)>,
    TotalBets: [(u64, Vec<Strap>); 10],
    ModifiersActive: [bool; 10],
    ModifierShop: Vec<(Modifier, Roll)>,
}

impl Snapshot {
    pub fn new() -> Self {
        let total_bets = [
            (0, Vec::new()),
            (0, Vec::new()),
            (0, Vec::new()),
            (0, Vec::new()),
            (0, Vec::new()),
            (0, Vec::new()),
            (0, Vec::new()),
            (0, Vec::new()),
            (0, Vec::new()),
            (0, Vec::new()),
        ];
        Snapshot {
            GameId: 0,
            RollIndex: 0,
            PotSize: 0,
            Rewards: vec![],
            TotalBets: total_bets,
            ModifiersActive: [false; 10],
            ModifierShop: Vec::new(),
        }
    }
}

pub struct AccountSnapshot {
    Bets: Vec<(Bet, u32)>,
}
