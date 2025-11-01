use crate::events::{
    Modifier,
    Roll,
    Strap,
};
use fuels::types::Identity;
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverviewSnapshot {
    pub(crate) game_id: u32,
    pub(crate) rolls: Vec<Roll>,
    pub(crate) pot_size: u64,
    pub(crate) current_block_height: u32,
    pub(crate) next_roll_height: Option<u32>,
    #[serde(default)]
    pub(crate) roll_frequency: Option<u32>,
    #[serde(default)]
    pub(crate) first_roll_height: Option<u32>,
    pub(crate) rewards: Vec<(Roll, Strap, u64)>,
    pub(crate) total_bets: [(u64, Vec<(Strap, u64)>); 11],
    pub(crate) modifiers_active: [Option<Modifier>; 11],
    pub(crate) modifier_shop: Vec<(Roll, Roll, Modifier, bool)>,
}

impl OverviewSnapshot {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for OverviewSnapshot {
    fn default() -> Self {
        let total_bets: [(u64, Vec<(Strap, u64)>); 11] = [
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
            current_block_height: 0,
            next_roll_height: None,
            roll_frequency: None,
            first_roll_height: None,
            rewards: Vec::new(),
            total_bets,
            modifiers_active: [None; 11],
            modifier_shop: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountBetKind {
    Chip,
    Strap(Strap),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountBetPlacement {
    pub bet_roll_index: u32,
    pub amount: u64,
    pub kind: AccountBetKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountRollBets {
    pub roll: Roll,
    pub bets: Vec<AccountBetPlacement>,
}

pub const ALL_ROLLS: [Roll; 11] = [
    Roll::Two,
    Roll::Three,
    Roll::Four,
    Roll::Five,
    Roll::Six,
    Roll::Seven,
    Roll::Eight,
    Roll::Nine,
    Roll::Ten,
    Roll::Eleven,
    Roll::Twelve,
];

// Used for current game, as well as historical games
// Historical snapshots can be used to claim rewards for past games
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountSnapshot {
    pub total_chip_bet: u64,
    pub strap_bets: Vec<(Strap, u64)>,
    pub total_chip_won: u64,
    pub claimed_rewards: Option<(u64, Vec<(Strap, u64)>)>,
    pub per_roll_bets: Vec<AccountRollBets>,
}

impl AccountSnapshot {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for AccountSnapshot {
    fn default() -> Self {
        let per_roll_bets = ALL_ROLLS
            .iter()
            .copied()
            .map(|roll| AccountRollBets {
                roll,
                bets: Vec::new(),
            })
            .collect();

        Self {
            total_chip_bet: 0,
            strap_bets: Vec::new(),
            total_chip_won: 0,
            claimed_rewards: None,
            per_roll_bets,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoricalSnapshot {
    pub game_id: u32,
    pub rolls: Vec<Roll>,
    pub modifiers: Vec<ActiveModifier>,
    pub strap_rewards: Vec<(Roll, Strap, u64)>,
    pub accounts: Vec<HistoricalAccountSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoricalAccountSnapshot {
    pub identity: Identity,
    pub snapshot: AccountSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
            strap_rewards: Vec::new(),
            accounts: Vec::new(),
        }
    }
}

pub fn all_rolls() -> Vec<Roll> {
    ALL_ROLLS.to_vec()
}
