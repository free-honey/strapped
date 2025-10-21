use crate::{
    Result,
    app::{
        event_source::EventSource,
        query_api::QueryAPI,
        snapshot_storage::{
            MetadataStorage,
            SnapshotStorage,
        },
    },
    snapshot::Snapshot,
};
use std::fs::Metadata;

pub mod event_source;
pub mod query_api;
pub mod snapshot_storage;

pub struct App<Events, API, Snapshots, Metadata> {
    events: Events,
    api: API,
    snapshots: Snapshots,
    metadata: Metadata,
}

#[cfg(test)]
mod tests;

impl<Events, API, Snapshots, Metadata> App<Events, API, Snapshots, Metadata> {
    pub fn new(
        events: Events,
        api: API,
        snapshots: Snapshots,
        metadata: Metadata,
    ) -> Self {
        Self {
            events,
            api,
            snapshots,
            metadata,
        }
    }
}

impl<
    Events: EventSource,
    API: QueryAPI,
    Snapshots: SnapshotStorage,
    Metadata: MetadataStorage,
> App<Events, API, Snapshots, Metadata>
{
    pub async fn run(&mut self) {
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
