#![allow(non_snake_case)]

use super::*;
use crate::{
    app::query_api::Query,
    events::Event,
    snapshot::AccountSnapshot,
};
use anyhow::Result;
use fuels::{
    prelude::AssetId,
    types::Identity,
};

use generated_abi::strapped_types::Strap;
use std::{
    collections::HashMap,
    future::pending,
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

impl SnapshotStorage for FakeSnapshotStorage {
    fn latest_snapshot(&self) -> crate::Result<(Snapshot, u32)> {
        todo!()
    }

    fn latest_account_snapshot(
        &self,
        account: &Identity,
    ) -> crate::Result<(AccountSnapshot, u32)> {
        todo!()
    }

    fn get_snapshot_at(&self, height: u32) -> crate::Result<Snapshot> {
        todo!()
    }

    fn get_account_snapshot_at(
        &self,
        account: &Identity,
        height: u32,
    ) -> crate::Result<AccountSnapshot> {
        todo!()
    }

    fn update_snapshot(&mut self, snapshot: &Snapshot, height: u32) -> crate::Result<()> {
        let mut guard = self.snapshot.lock().unwrap();
        *guard = Some(snapshot.clone());
        Ok(())
    }

    fn update_account_snapshot(
        &mut self,
        account_snapshot: &AccountSnapshot,
        height: u32,
    ) -> crate::Result<()> {
        todo!()
    }

    fn roll_back_snapshots(&mut self, to_height: u32) -> crate::Result<()> {
        todo!()
    }
}

pub struct FakeMetadataStorage;

impl MetadataStorage for FakeMetadataStorage {
    fn strap_asset_id(&self, strap_id: &AssetId) -> crate::Result<Option<Strap>> {
        todo!()
    }

    fn record_new_asset_id(
        &mut self,
        strap_id: &AssetId,
        strap: &Strap,
    ) -> crate::Result<()> {
        todo!()
    }
}

pub struct FakeQueryApi;

impl QueryAPI for FakeQueryApi {
    async fn query(&self) -> crate::Result<Query> {
        pending().await
    }
}

fn arb_init_event() -> Event {
    let vrf_contract_id = [0u8; 32];
    let chip_asset_id = [1u8; 32];
    let roll_frequency = 10;
    let first_height = 100;
    Event::init_event(vrf_contract_id, chip_asset_id, roll_frequency, first_height)
}

#[tokio::test]
async fn run__initialize_event__creates_first_snapshot() {
    // given
    let (event_source, mut event_sender) = FakeEventSource::new_with_sender();
    let snapshot_storage = FakeSnapshotStorage::new();
    let snapshot_copy = snapshot_storage.snapshot();

    let metadata_storage = FakeMetadataStorage;
    let query_api = FakeQueryApi;
    let mut app = App::new(event_source, query_api, snapshot_storage, metadata_storage);

    let init_event = arb_init_event();
    let init_height = 100;

    // when
    event_sender.send((init_event, init_height)).await.unwrap();
    app.run().await;

    // then
    let expected = Snapshot::new();
    let actual = snapshot_copy.lock().unwrap().clone().unwrap();
    assert_eq!(expected, actual);
}
