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
    events::{
        ContractEvent,
        Event,
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
                    Ok((ev, height)) => {
                        self.handle_event(ev, height);
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

    fn handle_event(&mut self, event: Event, height: u32) {
        match event {
            Event::BlockchainEvent => {}
            Event::ContractEvent(contract_event) => {
                match contract_event {
                    ContractEvent::Initialized(_) => {
                        tracing::info!("Contract initialized at height {}", height);
                        let snapshot = Snapshot::new();
                        // TODO: use actual height
                        self.snapshots.update_snapshot(&snapshot, height).unwrap();
                    }
                    _ => {}
                }
            }
        }
    }
}
