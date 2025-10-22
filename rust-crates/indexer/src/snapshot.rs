use generated_abi::strapped_types::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub(crate) game_id: u32,
    pub(crate) rolls: Vec<Roll>,
    pub(crate) pot_size: u64,
    pub(crate) rewards: Vec<(Roll, Strap)>,
    pub(crate) total_bets: [(u64, Vec<(Strap, u64)>); 10],
    pub(crate) modifiers_active: [bool; 10],
    pub(crate) modifier_shop: Vec<(Modifier, Roll)>,
}

impl Snapshot {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for Snapshot {
    fn default() -> Self {
        let total_bets: [(u64, Vec<(Strap, u64)>); 10] = [
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
            // GameId: 0,
            // RollIndex: 0,
            // PotSize: 0,
            // Rewards: vec![],
            // TotalBets: total_bets,
            // ModifiersActive: [false; 10],
            // ModifierShop: Vec::new(),
            game_id: 0,
            rolls: Vec::new(),
            pot_size: 0,
            rewards: Vec::new(),
            total_bets,
            modifiers_active: [false; 10],
            modifier_shop: Vec::new(),
        }
    }
}

pub struct AccountSnapshot {
    Bets: Vec<(Bet, u32)>,
}
