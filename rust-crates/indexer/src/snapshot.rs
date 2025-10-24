use crate::events::{
    Modifier,
    Roll,
    Strap,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverviewSnapshot {
    pub(crate) game_id: u32,
    pub(crate) rolls: Vec<Roll>,
    pub(crate) pot_size: u64,
    pub(crate) rewards: Vec<(Roll, Strap, u64)>,
    pub(crate) total_bets: [(u64, Vec<(Strap, u64)>); 10],
    pub(crate) modifiers_active: [bool; 10],
    pub(crate) modifier_shop: Vec<(Roll, Roll, Modifier, bool)>,
}

impl OverviewSnapshot {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for OverviewSnapshot {
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
        OverviewSnapshot {
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

// Used for current game, as well as historical games
// Historical snapshots can be used to claim rewards for past games
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AccountSnapshot {
    pub total_chip_bet: u64,
    pub strap_bets: Vec<(Strap, u64)>,
    pub total_chip_won: u64,
    pub claimed_rewards: Option<(u64, Vec<(Strap, u64)>)>,
}

#[allow(dead_code)]
// Historical shapshot that is persisted after current game ends. Updated as each event occurs
pub struct HistoricalSnapshot {
    game_id: u32,
    rolls: Vec<Roll>,
    // The roll for which a modifier was activated, and the roll index at which it was activated
    // This allows the player to see which modifiers are available for their bet straps
    modifiers_active: Vec<(Roll, u32)>,
    // TODO: we can add additional interesting data here that isn't necessary, like how much was
    //   bet, won, lost, etc.
}
