use crate::events::{
    Modifier,
    Roll,
    Strap,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverviewSnapshot {
    pub(crate) game_id: u32,
    pub(crate) rolls: Vec<Roll>,
    pub(crate) pot_size: u64,
    pub(crate) rewards: Vec<(Roll, Strap, u64)>,
    pub(crate) total_bets: [(u64, Vec<(Strap, u64)>); 10],
    pub(crate) modifiers_active: [Option<Modifier>; 10],
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
            modifiers_active: [None; 10],
            modifier_shop: Vec::new(),
        }
    }
}

// Used for current game, as well as historical games
// Historical snapshots can be used to claim rewards for past games
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountSnapshot {
    pub total_chip_bet: u64,
    pub strap_bets: Vec<(Strap, u64)>,
    pub total_chip_won: u64,
    pub claimed_rewards: Option<(u64, Vec<(Strap, u64)>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoricalSnapshot {
    pub game_id: u32,
    pub rolls: Vec<Roll>,
    pub modifiers: Vec<ActiveModifier>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveModifier {
    pub roll_index: u32,
    pub modifier: Modifier,
    pub modifier_roll: Roll,
}

impl ActiveModifier {
    pub fn new(roll_height: u32, modifier: Modifier, modifier_roll: Roll) -> Self {
        Self {
            roll_index: roll_height,
            modifier,
            modifier_roll,
        }
    }
}

impl HistoricalSnapshot {
    pub fn new(game_id: u32, rolls: Vec<Roll>, modifiers: Vec<ActiveModifier>) -> Self {
        Self {
            game_id,
            rolls,
            modifiers,
        }
    }
}
