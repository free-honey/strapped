use crate::{
    app::snapshot_storage::SortOrder,
    events::Strap,
    snapshot::{
        AccountSnapshot,
        EquipmentSnapshot,
        HistoricalSnapshot,
        OverviewSnapshot,
    },
};
use fuels::types::{
    AssetId,
    Identity,
};
use std::future::Future;
use tokio::sync::oneshot;

pub trait QueryAPI {
    /// Returns the next query if the API is still running.
    /// Ok(None) indicates the server has shut down and no more queries will arrive.
    fn query(&mut self) -> impl Future<Output = crate::Result<Option<Query>>>;
}

#[derive(Debug)]
pub enum Query {
    LatestSnapshot(oneshot::Sender<(OverviewSnapshot, u32)>),
    LatestAccountSnapshot(AccountSnapshotQuery),
    LatestEquipmentSnapshot(EquipmentSnapshotQuery),
    HistoricalSnapshot(HistoricalSnapshotQuery),
    HistoricalAccountSnapshot(HistoricalAccountSnapshotQuery),
    UnclaimedGames(UnclaimedGamesQuery),
    BetHistory(BetHistoryQuery),
    AllKnownStraps(oneshot::Sender<Vec<(AssetId, Strap)>>),
}

impl Query {
    pub fn latest_account_summary(
        identity: Identity,
        sender: oneshot::Sender<Option<(AccountSnapshot, u32)>>,
    ) -> Query {
        let inner = AccountSnapshotQuery { identity, sender };
        Query::LatestAccountSnapshot(inner)
    }

    pub fn latest_equipment_snapshot(
        identity: Identity,
        sender: oneshot::Sender<Option<(EquipmentSnapshot, u32)>>,
    ) -> Query {
        let inner = EquipmentSnapshotQuery { identity, sender };
        Query::LatestEquipmentSnapshot(inner)
    }

    pub fn historical_snapshot(
        game_id: u32,
        sender: oneshot::Sender<Option<HistoricalSnapshot>>,
    ) -> Query {
        let inner = HistoricalSnapshotQuery { game_id, sender };
        Query::HistoricalSnapshot(inner)
    }

    pub fn historical_account_summary(
        identity: Identity,
        game_id: u32,
        sender: oneshot::Sender<Option<(AccountSnapshot, u32)>>,
    ) -> Query {
        let inner = HistoricalAccountSnapshotQuery {
            identity,
            game_id,
            sender,
        };
        Query::HistoricalAccountSnapshot(inner)
    }

    pub fn all_known_straps(sender: oneshot::Sender<Vec<(AssetId, Strap)>>) -> Query {
        Query::AllKnownStraps(sender)
    }

    pub fn unclaimed_games(
        identity: Identity,
        order: SortOrder,
        limit: usize,
        cursor: Option<u32>,
        sender: oneshot::Sender<crate::Result<UnclaimedGamesPage>>,
    ) -> Query {
        let inner = UnclaimedGamesQuery {
            identity,
            order,
            limit,
            cursor,
            sender,
        };
        Query::UnclaimedGames(inner)
    }

    pub fn bet_history(
        identity: Identity,
        order: SortOrder,
        limit: usize,
        cursor: Option<u32>,
        sender: oneshot::Sender<crate::Result<BetHistoryPage>>,
    ) -> Query {
        let inner = BetHistoryQuery {
            identity,
            order,
            limit,
            cursor,
            sender,
        };
        Query::BetHistory(inner)
    }
}

#[derive(Debug)]
pub struct AccountSnapshotQuery {
    pub identity: Identity,
    pub sender: oneshot::Sender<Option<(AccountSnapshot, u32)>>,
}

#[derive(Debug)]
pub struct EquipmentSnapshotQuery {
    pub identity: Identity,
    pub sender: oneshot::Sender<Option<(EquipmentSnapshot, u32)>>,
}

#[derive(Debug)]
pub struct HistoricalSnapshotQuery {
    pub game_id: u32,
    pub sender: oneshot::Sender<Option<HistoricalSnapshot>>,
}

#[derive(Debug)]
pub struct HistoricalAccountSnapshotQuery {
    pub identity: Identity,
    pub game_id: u32,
    pub sender: oneshot::Sender<Option<(AccountSnapshot, u32)>>,
}

#[derive(Debug)]
pub struct UnclaimedGamesQuery {
    pub identity: Identity,
    pub order: SortOrder,
    pub limit: usize,
    pub cursor: Option<u32>,
    pub sender: oneshot::Sender<crate::Result<UnclaimedGamesPage>>,
}

#[derive(Debug)]
pub struct BetHistoryQuery {
    pub identity: Identity,
    pub order: SortOrder,
    pub limit: usize,
    pub cursor: Option<u32>,
    pub sender: oneshot::Sender<crate::Result<BetHistoryPage>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnclaimedGame {
    pub game_id: u32,
    pub account_snapshot: AccountSnapshot,
    pub historical_snapshot: HistoricalSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BetHistoryGame {
    pub game_id: u32,
    pub account_snapshot: AccountSnapshot,
    pub historical_snapshot: HistoricalSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnclaimedGamesPage {
    pub games: Vec<UnclaimedGame>,
    pub next_cursor: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BetHistoryPage {
    pub games: Vec<BetHistoryGame>,
    pub next_cursor: Option<u32>,
}
