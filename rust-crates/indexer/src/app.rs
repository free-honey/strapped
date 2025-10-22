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
use generated_abi::strapped_types::{
    Roll,
    RollEvent,
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

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();
}

impl<
    Events: EventSource,
    API: QueryAPI,
    Snapshots: SnapshotStorage,
    Metadata: MetadataStorage,
> App<Events, API, Snapshots, Metadata>
{
    pub async fn run(&mut self) -> Result<()> {
        init_tracing();
        tokio::select! {
            event = self.events.next_event() => {
                match event {
                    Ok((ev, height)) => {
                        self.handle_event(ev, height)
                    }
                    Err(e) => {
                        Err(e)
                    }
                }
            }
            query = self.api.query() => {
                match query {
                    Ok(q) => {
                        Ok(())
                    }
                    Err(e) => {
                        Err(e)
                    }
                }
            }
        }
    }

    fn handle_event(&mut self, event: Event, height: u32) -> Result<()> {
        match event {
            Event::BlockchainEvent => {
                todo!()
            }
            Event::ContractEvent(contract_event) => match contract_event {
                ContractEvent::Initialized(_) => {
                    self.handle_initialized_event(contract_event, height)
                }
                ContractEvent::Roll(roll_event) => {
                    let RollEvent { rolled_value, .. } = roll_event;
                    self.handle_roll_event(rolled_value, height)
                }
                _ => {
                    todo!()
                }
            },
        }
    }

    fn handle_initialized_event(
        &mut self,
        _event: ContractEvent,
        height: u32,
    ) -> Result<()> {
        tracing::info!("Handling InitializedEvent at height {}", height);
        let snapshot = Snapshot::new();
        self.snapshots.update_snapshot(&snapshot, height)
    }

    fn handle_roll_event(&mut self, roll: Roll, height: u32) -> Result<()> {
        tracing::info!("Handling RollEvent at height {}", height);
        let (mut snapshot, _) = self.snapshots.latest_snapshot()?;
        snapshot.rolls.push(roll);
        self.snapshots.update_snapshot(&snapshot, height)
    }
}
