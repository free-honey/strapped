#![allow(non_snake_case)]

use super::*;
use crate::{
    app::query_api::Query,
    events::{
        ContractEvent,
        Event,
    },
    snapshot::AccountSnapshot,
};
use anyhow::Result;
use fuels::{
    prelude::AssetId,
    types::Identity,
};

use crate::snapshot::HistoricalSnapshot;
use generated_abi::strapped_types::*;
use std::{
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
    snapshot: Arc<Mutex<Option<(OverviewSnapshot, u32)>>>,
}

impl FakeSnapshotStorage {
    pub fn new() -> Self {
        Self {
            snapshot: Arc::new(Mutex::new(None)),
        }
    }

    pub fn new_with_snapshot(snapshot: OverviewSnapshot, height: u32) -> Self {
        Self {
            snapshot: Arc::new(Mutex::new(Some((snapshot, height)))),
        }
    }

    pub fn snapshot(&self) -> Arc<Mutex<Option<(OverviewSnapshot, u32)>>> {
        self.snapshot.clone()
    }
}

impl SnapshotStorage for FakeSnapshotStorage {
    fn latest_snapshot(&self) -> crate::Result<(OverviewSnapshot, u32)> {
        let guard = self.snapshot.lock().unwrap();
        match &*guard {
            Some(snapshot) => Ok(snapshot.clone()),
            None => Err(anyhow::anyhow!("No snapshot found")),
        }
    }

    fn latest_account_snapshot(
        &self,
        account: &Identity,
    ) -> crate::Result<(AccountSnapshot, u32)> {
        todo!()
    }

    fn update_snapshot(
        &mut self,
        snapshot: &OverviewSnapshot,
        height: u32,
    ) -> crate::Result<()> {
        let mut guard = self.snapshot.lock().unwrap();
        *guard = Some((snapshot.clone(), height));
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

    fn historical_snapshots(&self, game_id: u32) -> crate::Result<HistoricalSnapshot> {
        todo!()
    }

    fn write_historical_snapshot(
        &mut self,
        game_id: u32,
        snapshot: &HistoricalSnapshot,
    ) -> crate::Result<()> {
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
    app.run().await.unwrap();

    // then
    let expected = OverviewSnapshot::new();
    let (actual, _) = snapshot_copy.lock().unwrap().clone().unwrap();
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn run__roll_event__updates_snapshot() {
    // given
    let (event_source, mut event_sender) = FakeEventSource::new_with_sender();
    let game_id = 1u32;
    let roll_index = 0u32;
    let rolled_value = Roll::Five;

    let existing_snapshot = OverviewSnapshot {
        game_id,
        ..OverviewSnapshot::default()
    };
    let snapshot_storage =
        FakeSnapshotStorage::new_with_snapshot(existing_snapshot.clone(), 105);
    let snapshot_copy = snapshot_storage.snapshot();

    let metadata_storage = FakeMetadataStorage;
    let query_api = FakeQueryApi;
    let mut app = App::new(event_source, query_api, snapshot_storage, metadata_storage);

    let roll_event = Event::roll_event(game_id, roll_index, rolled_value.clone());
    let roll_height = 110;

    // when
    event_sender.send((roll_event, roll_height)).await.unwrap();
    app.run().await.unwrap();

    // then
    let expected = {
        let mut snap = existing_snapshot;
        snap.rolls.push(rolled_value);
        snap
    };
    let (actual, _) = snapshot_copy.lock().unwrap().clone().unwrap();
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn run__new_game_event__resets_overview_snapshot() {
    // given
    let (event_source, mut event_sender) = FakeEventSource::new_with_sender();

    let existing_snapshot = OverviewSnapshot {
        game_id: 1,
        rolls: vec![Roll::Five],
        pot_size: 500,
        rewards: vec![(
            Roll::Three,
            Strap::new(1, StrapKind::Hat, Modifier::Lucky),
            100,
        )],
        modifier_shop: vec![(Roll::Two, Roll::Three, Modifier::Burnt, false)],
        ..OverviewSnapshot::default()
    };
    let snapshot_storage =
        FakeSnapshotStorage::new_with_snapshot(existing_snapshot.clone(), 200);
    let snapshot_copy = snapshot_storage.snapshot();

    let metadata_storage = FakeMetadataStorage;
    let query_api = FakeQueryApi;
    let mut app = App::new(event_source, query_api, snapshot_storage, metadata_storage);

    let next_game_id: u32 = 2;
    let shop_modifier = (Roll::Seven, Roll::Four, Modifier::Groovy);
    let strap_reward = (
        Roll::Nine,
        Strap::new(2, StrapKind::Coat, Modifier::Nothing),
        150,
    );

    let new_game_event = ContractEvent::NewGame(NewGameEvent {
        game_id: next_game_id,
        new_straps: vec![strap_reward.clone()],
        new_modifiers: vec![shop_modifier.clone()],
    });
    let new_game_height = 210;

    // when
    event_sender
        .send((Event::ContractEvent(new_game_event), new_game_height))
        .await
        .unwrap();
    app.run().await.unwrap();

    // then
    let (actual, _) = snapshot_copy.lock().unwrap().clone().unwrap();
    let mut expected = OverviewSnapshot::default();
    expected.game_id = next_game_id;
    expected.rewards = vec![strap_reward];
    expected.modifier_shop = vec![(
        shop_modifier.0.clone(),
        shop_modifier.1.clone(),
        shop_modifier.2.clone(),
        false,
    )];
    assert_eq!(expected, actual);
}
