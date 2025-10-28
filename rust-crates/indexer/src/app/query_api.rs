use crate::snapshot::{
    AccountSnapshot,
    OverviewSnapshot,
};
use fuels::types::Identity;
use tokio::sync::oneshot;

pub trait QueryAPI {
    fn query(&mut self) -> impl Future<Output = crate::Result<Query>>;
}

#[derive(Debug)]
pub enum Query {
    LatestSnapshot(oneshot::Sender<(OverviewSnapshot, u32)>),
    LatestAccountSnapshot(AccountSnapshotQuery),
}

impl Query {
    pub fn latest_account_summary(
        identity: Identity,
        sender: oneshot::Sender<Option<(AccountSnapshot, u32)>>,
    ) -> Query {
        let inner = AccountSnapshotQuery { identity, sender };
        Query::LatestAccountSnapshot(inner)
    }
}

#[derive(Debug)]
pub struct AccountSnapshotQuery {
    pub identity: Identity,
    pub sender: oneshot::Sender<Option<(AccountSnapshot, u32)>>,
}
