use crate::{
    Error,
    Result,
    events::Event,
    snapshot::Snapshot,
};

pub struct App<Events, API, Storage> {
    events: Events,
    api: API,
    storage: Storage,
}

#[cfg(test)]
mod tests;

pub trait EventSource {
    fn next_event(&self) -> impl Future<Output = Result<(Event, u32)>>;
}

pub trait QueryAPI {
    fn query(&self) -> impl Future<Output = Result<Query>>;
}

pub enum Query {}

pub trait SnapshotStorage {
    fn latest_snapshot(&self) -> Result<(Snapshot, u32)>;
    fn get_snapshot_at(&self, height: u32) -> Result<Snapshot>;
    fn update_snapshot(&mut self, snapshot: &Snapshot, height: u32) -> Result<()>;
    fn roll_back_snapshot(&mut self, to_height: u32) -> Result<()>;
}

impl<Events, API, Storage> App<Events, API, Storage> {
    pub fn new(events: Events, api: API, storage: Storage) -> Self {
        Self {
            events,
            api,
            storage,
        }
    }
}

impl<Events: EventSource, API: QueryAPI, Storage: SnapshotStorage>
    App<Events, API, Storage>
{
    pub async fn run(&self) {
        tokio::select! {
            event = self.events.next_event() => {
                match event {
                    Ok(ev) => {
                        // Handle the event
                    }
                    Err(e) => {
                        // Handle the error
                    }
                }
            }
            query = self.api.query() => {
                match query {
                    Ok(q) => {
                        // Handle the query
                    }
                    Err(e) => {
                        // Handle the error
                    }
                }
            }
        }
    }
}
