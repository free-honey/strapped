use crate::snapshot::OverviewSnapshot;
use tokio::sync::oneshot;

pub trait QueryAPI {
    fn query(&mut self) -> impl Future<Output = crate::Result<Query>>;
}

#[derive(Debug)]
pub enum Query {
    LatestSnapshot(oneshot::Sender<(OverviewSnapshot, u32)>),
}
