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
    types::{Address, Identity},
};

use crate::snapshot::HistoricalSnapshot;
use generated_abi::strapped_types::*;
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
    snapshot: Arc<Mutex<Option<(OverviewSnapshot, u32)>>>,
    account_snapshots: Arc<Mutex<HashMap<String, (AccountSnapshot, u32)>>>,
}

impl FakeSnapshotStorage {
    pub fn new() -> Self {
        Self {
            snapshot: Arc::new(Mutex::new(None)),
            account_snapshots: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn new_with_snapshot(snapshot: OverviewSnapshot, height: u32) -> Self {
        Self {
            snapshot: Arc::new(Mutex::new(Some((snapshot, height)))),
            account_snapshots: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn snapshot(&self) -> Arc<Mutex<Option<(OverviewSnapshot, u32)>>> {
        self.snapshot.clone()
    }

    pub fn account_snapshots(
        &self,
    ) -> Arc<Mutex<HashMap<String, (AccountSnapshot, u32)>>> {
        self.account_snapshots.clone()
    }

    pub fn identity_key(account: &Identity) -> String {
        format!("{:?}", account)
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
        let key = Self::identity_key(account);
        let guard = self.account_snapshots.lock().unwrap();
        guard
            .get(&key)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("No account snapshot found"))
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
        account: &Identity,
        account_snapshot: &AccountSnapshot,
        height: u32,
    ) -> crate::Result<()> {
        let key = Self::identity_key(account);
        let mut guard = self.account_snapshots.lock().unwrap();
        guard.insert(key, (account_snapshot.clone(), height));
        Ok(())
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

#[tokio::test]
async fn run__modifier_triggered_event__activates_modifier() {
    // given
    let (event_source, mut event_sender) = FakeEventSource::new_with_sender();

    let existing_snapshot = OverviewSnapshot {
        game_id: 5,
        modifier_shop: vec![(Roll::Three, Roll::Four, Modifier::Holy, false)],
        ..OverviewSnapshot::default()
    };
    let snapshot_storage =
        FakeSnapshotStorage::new_with_snapshot(existing_snapshot.clone(), 220);
    let snapshot_copy = snapshot_storage.snapshot();

    let metadata_storage = FakeMetadataStorage;
    let query_api = FakeQueryApi;
    let mut app = App::new(event_source, query_api, snapshot_storage, metadata_storage);

    let modifier_event = ContractEvent::ModifierTriggered(ModifierTriggeredEvent {
        game_id: existing_snapshot.game_id,
        roll_index: 1,
        trigger_roll: Roll::Three,
        modifier_roll: Roll::Four,
        modifier: Modifier::Holy,
    });
    let event_height = 225;

    // when
    event_sender
        .send((Event::ContractEvent(modifier_event), event_height))
        .await
        .unwrap();
    app.run().await.unwrap();

    // then
    let (actual, _) = snapshot_copy.lock().unwrap().clone().unwrap();
    let mut expected = existing_snapshot;
    expected.modifiers_active[2] = true;
    expected.modifier_shop[0].3 = true;
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn run__place_chip_bet_event__updates_pot_and_totals() {
    let (event_source, mut event_sender) = FakeEventSource::new_with_sender();

    let mut existing_snapshot = OverviewSnapshot::default();
    existing_snapshot.pot_size = 200;
    existing_snapshot.total_bets[4].0 = 50; // Roll::Six index

    let snapshot_storage =
        FakeSnapshotStorage::new_with_snapshot(existing_snapshot.clone(), 300);
    let snapshot_copy = snapshot_storage.snapshot();

    let metadata_storage = FakeMetadataStorage;
    let query_api = FakeQueryApi;
    let mut app = App::new(event_source, query_api, snapshot_storage, metadata_storage);

    let chip_event = ContractEvent::PlaceChipBet(PlaceChipBetEvent {
        game_id: existing_snapshot.game_id,
        bet_roll_index: 0,
        player: Identity::Address(Address::from([0u8; 32])),
        roll: Roll::Six,
        amount: 150,
    });

    event_sender
        .send((Event::ContractEvent(chip_event), 305))
        .await
        .unwrap();
    app.run().await.unwrap();

    let (actual, _) = snapshot_copy.lock().unwrap().clone().unwrap();
    let mut expected = existing_snapshot;
    expected.pot_size = 350;
    expected.total_bets[4].0 = 200;
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn run__place_chip_bet_event__updates_account_snapshot() {
    let (event_source, mut event_sender) = FakeEventSource::new_with_sender();

    let snapshot_storage =
        FakeSnapshotStorage::new_with_snapshot(OverviewSnapshot::default(), 300);
    let accounts_map = snapshot_storage.account_snapshots();

    let metadata_storage = FakeMetadataStorage;
    let query_api = FakeQueryApi;
    let mut app = App::new(event_source, query_api, snapshot_storage, metadata_storage);

    let player = Identity::Address(Address::from([0u8; 32]));
    let chip_event = ContractEvent::PlaceChipBet(PlaceChipBetEvent {
        game_id: 0,
        bet_roll_index: 0,
        player: player.clone(),
        roll: Roll::Six,
        amount: 150,
    });

    event_sender
        .send((Event::ContractEvent(chip_event), 305))
        .await
        .unwrap();
    app.run().await.unwrap();

    let key = FakeSnapshotStorage::identity_key(&player);
    let account_guard = accounts_map.lock().unwrap();
    let (account_snapshot, _) = account_guard.get(&key).cloned().unwrap();
    assert_eq!(account_snapshot.total_chip_bet, 150);
    assert!(account_snapshot.strap_bets.is_empty());
    assert_eq!(account_snapshot.total_chip_won, 0);
    assert_eq!(account_snapshot.claimed_rewards, None);
}

#[tokio::test]
async fn run__place_strap_bet_event__records_strap_bet() {
    let (event_source, mut event_sender) = FakeEventSource::new_with_sender();

    let mut existing_snapshot = OverviewSnapshot::default();
    existing_snapshot.total_bets[3].1 = vec![(
        Strap::new(1, StrapKind::Gloves, Modifier::Lucky),
        1,
    )];

    let snapshot_storage =
        FakeSnapshotStorage::new_with_snapshot(existing_snapshot.clone(), 410);
    let snapshot_copy = snapshot_storage.snapshot();

    let metadata_storage = FakeMetadataStorage;
    let query_api = FakeQueryApi;
    let mut app = App::new(event_source, query_api, snapshot_storage, metadata_storage);

    let strap = Strap::new(2, StrapKind::Gloves, Modifier::Lucky);
    let player = Identity::Address(Address::from([1u8; 32]));
    let strap_event = ContractEvent::PlaceStrapBet(PlaceStrapBetEvent {
        game_id: existing_snapshot.game_id,
        bet_roll_index: 3,
        player,
        strap: strap.clone(),
        amount: 2,
    });

    event_sender
        .send((Event::ContractEvent(strap_event), 415))
        .await
        .unwrap();
    app.run().await.unwrap();

    let (actual, _) = snapshot_copy.lock().unwrap().clone().unwrap();
    let mut expected = existing_snapshot;
    expected.total_bets[3].1.push((strap, 2));
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn run__place_strap_bet_event__updates_account_snapshot() {
    let (event_source, mut event_sender) = FakeEventSource::new_with_sender();

    let snapshot_storage =
        FakeSnapshotStorage::new_with_snapshot(OverviewSnapshot::default(), 0);
    let accounts_map = snapshot_storage.account_snapshots();

    let metadata_storage = FakeMetadataStorage;
    let query_api = FakeQueryApi;
    let mut app = App::new(event_source, query_api, snapshot_storage, metadata_storage);

    let strap = Strap::new(2, StrapKind::Gloves, Modifier::Lucky);
    let expected_strap = strap.clone();
    let player = Identity::Address(Address::from([1u8; 32]));
    let strap_event = ContractEvent::PlaceStrapBet(PlaceStrapBetEvent {
        game_id: 0,
        bet_roll_index: 3,
        player: player.clone(),
        strap: strap.clone(),
        amount: 2,
    });

    event_sender
        .send((Event::ContractEvent(strap_event), 415))
        .await
        .unwrap();
    app.run().await.unwrap();

    let key = FakeSnapshotStorage::identity_key(&player);
    let account_guard = accounts_map.lock().unwrap();
    let (account_snapshot, _) = account_guard.get(&key).cloned().unwrap();
    assert_eq!(account_snapshot.total_chip_bet, 0);
    assert_eq!(account_snapshot.total_chip_won, 0);
    assert_eq!(account_snapshot.claimed_rewards, None);
    assert_eq!(account_snapshot.strap_bets, vec![(expected_strap, 2)]);
}

#[tokio::test]
async fn run__claim_rewards_event__reduces_pot() {
    let (event_source, mut event_sender) = FakeEventSource::new_with_sender();

    let mut existing_snapshot = OverviewSnapshot::default();
    existing_snapshot.pot_size = 500;

    let snapshot_storage =
        FakeSnapshotStorage::new_with_snapshot(existing_snapshot.clone(), 510);
    let snapshot_copy = snapshot_storage.snapshot();

    let metadata_storage = FakeMetadataStorage;
    let query_api = FakeQueryApi;
    let mut app = App::new(event_source, query_api, snapshot_storage, metadata_storage);

    let claim_event = ContractEvent::ClaimRewards(ClaimRewardsEvent {
        game_id: existing_snapshot.game_id,
        player: Identity::Address(Address::from([2u8; 32])),
        enabled_modifiers: vec![],
        total_chips_winnings: 120,
        total_strap_winnings: vec![],
    });

    event_sender
        .send((Event::ContractEvent(claim_event), 515))
        .await
        .unwrap();
    app.run().await.unwrap();

    let (actual, _) = snapshot_copy.lock().unwrap().clone().unwrap();
    let mut expected = existing_snapshot;
    expected.pot_size = 380;
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn run__claim_rewards_event__updates_account_snapshot() {
    let (event_source, mut event_sender) = FakeEventSource::new_with_sender();

    let snapshot_storage =
        FakeSnapshotStorage::new_with_snapshot(OverviewSnapshot::default(), 0);
    let accounts_map = snapshot_storage.account_snapshots();

    let metadata_storage = FakeMetadataStorage;
    let query_api = FakeQueryApi;
    let mut app = App::new(event_source, query_api, snapshot_storage, metadata_storage);

    let player = Identity::Address(Address::from([2u8; 32]));
    let claim_event = ContractEvent::ClaimRewards(ClaimRewardsEvent {
        game_id: 0,
        player: player.clone(),
        enabled_modifiers: vec![],
        total_chips_winnings: 120,
        total_strap_winnings: vec![],
    });

    event_sender
        .send((Event::ContractEvent(claim_event), 515))
        .await
        .unwrap();
    app.run().await.unwrap();

    let key = FakeSnapshotStorage::identity_key(&player);
    let account_guard = accounts_map.lock().unwrap();
    let (account_snapshot, _) = account_guard.get(&key).cloned().unwrap();
    assert_eq!(account_snapshot.total_chip_bet, 0);
    assert!(account_snapshot.strap_bets.is_empty());
    assert_eq!(account_snapshot.total_chip_won, 120);
    assert_eq!(account_snapshot.claimed_rewards, Some((120, Vec::new())));
}

#[tokio::test]
async fn run__fund_pot_event__increases_pot() {
    // given
    let (event_source, mut event_sender) = FakeEventSource::new_with_sender();

    let mut existing_snapshot = OverviewSnapshot::default();
    existing_snapshot.pot_size = 75;

    let snapshot_storage =
        FakeSnapshotStorage::new_with_snapshot(existing_snapshot.clone(), 610);
    let snapshot_copy = snapshot_storage.snapshot();

    let metadata_storage = FakeMetadataStorage;
    let query_api = FakeQueryApi;
    let mut app = App::new(event_source, query_api, snapshot_storage, metadata_storage);

    let fund_event = ContractEvent::FundPot(FundPotEvent {
        chips_amount: 325,
        funder: Identity::Address(Address::from([3u8; 32])),
    });

    // when
    event_sender
        .send((Event::ContractEvent(fund_event), 615))
        .await
        .unwrap();
    app.run().await.unwrap();

    // then
    let (actual, _) = snapshot_copy.lock().unwrap().clone().unwrap();
    let mut expected = existing_snapshot;
    expected.pot_size = 400;
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn run__purchase_modifier_event__marks_shop_entry() {
    // given
    let (event_source, mut event_sender) = FakeEventSource::new_with_sender();

    let mut existing_snapshot = OverviewSnapshot::default();
    existing_snapshot.modifier_shop = vec![(
        Roll::Two,
        Roll::Four,
        Modifier::Holy,
        false,
    )];

    let snapshot_storage =
        FakeSnapshotStorage::new_with_snapshot(existing_snapshot.clone(), 710);
    let snapshot_copy = snapshot_storage.snapshot();

    let metadata_storage = FakeMetadataStorage;
    let query_api = FakeQueryApi;
    let mut app = App::new(event_source, query_api, snapshot_storage, metadata_storage);

    let purchase_event = ContractEvent::PurchaseModifier(PurchaseModifierEvent {
        expected_roll: Roll::Four,
        expected_modifier: Modifier::Holy,
        purchaser: Identity::Address(Address::from([4u8; 32])),
    });

    // when
    event_sender
        .send((Event::ContractEvent(purchase_event), 715))
        .await
        .unwrap();
    app.run().await.unwrap();

    // then
    let (actual, _) = snapshot_copy.lock().unwrap().clone().unwrap();
    let mut expected = existing_snapshot;
    expected.modifiers_active[2] = true;
    expected.modifier_shop[0].3 = true;
    assert_eq!(expected, actual);
}
