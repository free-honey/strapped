use crate::snapshot::{
    AccountSnapshot,
    HistoricalSnapshot,
    OverviewSnapshot,
};
use fuels::types::Identity;
use std::future::Future;
use tokio::sync::oneshot;

pub trait QueryAPI {
    fn query(&mut self) -> impl Future<Output = crate::Result<Query>>;
}

#[derive(Debug)]
pub enum Query {
    LatestSnapshot(oneshot::Sender<(OverviewSnapshot, u32)>),
    LatestAccountSnapshot(AccountSnapshotQuery),
    HistoricalSnapshot(HistoricalSnapshotQuery),
    HistoricalAccountSnapshot(HistoricalAccountSnapshotQuery),
}

impl Query {
    pub fn latest_account_summary(
        identity: Identity,
        sender: oneshot::Sender<Option<(AccountSnapshot, u32)>>,
    ) -> Query {
        let inner = AccountSnapshotQuery { identity, sender };
        Query::LatestAccountSnapshot(inner)
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
}

#[derive(Debug)]
pub struct AccountSnapshotQuery {
    pub identity: Identity,
    pub sender: oneshot::Sender<Option<(AccountSnapshot, u32)>>,
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
