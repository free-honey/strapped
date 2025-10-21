#![allow(non_snake_case)]

use super::*;
use crate::events::Event;
use anyhow::Result;
use std::{
    collections::HashMap,
    sync::{
        Arc,
        Mutex,
    },
};

pub struct FakeEventSource {
    recv: tokio::sync::mpsc::Receiver<(Event, u32)>,
}

impl FakeEventSource {
    pub fn new_with_sender() -> (Self, tokio::sync::mpsc::Sender<(Event, u32)>) {
        let (send, recv) = tokio::sync::mpsc::channel(10);
        let recv = FakeEventSource { recv };
        (recv, send)
    }
}

impl EventSource for FakeEventSource {
    async fn next_event(&mut self) -> Result<(Event, u32)> {
        match self.recv.recv().await {
            Some(event) => Ok(event),
            None => Err(anyhow::anyhow!("No more events")),
        }
    }
}

pub struct FakeSnapshotStorage {
    snapshot: Arc<Mutex<Option<Snapshot>>>,
}

impl FakeSnapshotStorage {
    pub fn new() -> Self {
        Self {
            snapshot: Arc::new(Mutex::new(None)),
        }
    }

    pub fn snapshot(&self) -> Arc<Mutex<Option<Snapshot>>> {
        self.snapshot.clone()
    }
}

pub struct FakeMetadataStorage;

pub struct FakeQueryApi;

#[tokio::test]
async fn run__initialize_event__creates_first_snapshot() {
    // given
    let (mut event_source, mut event_sender) = FakeEventSource::new_with_sender();
    let snapshot_storage = FakeSnapshotStorage::new();
    let snapshot_copy = snapshot_storage.snapshot();

    let metadata_storage = FakeMetadataStorage;
    let query_api = FakeQueryApi;
    let app = App::new(event_source, query_api, snapshot_storage, metadata_storage);

    let vrf_contract_id = [0u8; 32];
    let chip_asset_id = [1u8; 32];
    let roll_frequency = 10;
    let first_height = 100;
    let init_event =
        Event::init_event(vrf_contract_id, chip_asset_id, roll_frequency, first_height);

    // when
    event_sender.send((init_event, 100)).await.unwrap();

    // then
    let expected = Snapshot::new();
    let actual = snapshot_copy.lock().unwrap().clone().unwrap();
    assert_eq!(expected, actual);
}
