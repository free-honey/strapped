#![allow(non_snake_case)]

use super::*;
use crate::{
    app::query_api::Query,
    events::{
        ContractEvent,
        Event,
    },
};
use anyhow::Result;
use fuels::types::{
    Address,
    ContractId,
    Identity,
};

use crate::{
    app::{
        in_memory_metadata_storage::InMemoryMetadataStorage,
        in_memory_snapshot_storage::InMemorySnapshotStorage,
    },
    events::{
        ClaimRewardsEvent,
        FundPotEvent,
        Modifier,
        ModifierTriggeredEvent,
        NewGameEvent,
        PlaceChipBetEvent,
        PlaceStrapBetEvent,
        PurchaseModifierEvent,
        Roll,
        Strap,
        StrapKind,
    },
};
use std::future::pending;
use tokio::sync::{
    mpsc,
    oneshot,
};

pub struct FakeEventSource {
    recv: mpsc::Receiver<(Vec<Event>, u32)>,
}

impl FakeEventSource {
    pub fn new_with_sender() -> (Self, mpsc::Sender<(Vec<Event>, u32)>) {
        let (send, recv) = mpsc::channel(10);
        let recv = FakeEventSource { recv };
        (recv, send)
    }
}

impl EventSource for FakeEventSource {
    async fn next_event_batch(&mut self) -> Result<(Vec<Event>, u32)> {
        match self.recv.recv().await {
            Some((events, height)) => Ok((events, height)),
            None => Err(anyhow::anyhow!("No more events")),
        }
    }
}

pub struct PendingEventSource;

impl EventSource for PendingEventSource {
    async fn next_event_batch(&mut self) -> Result<(Vec<Event>, u32)> {
        pending().await
    }
}

fn zero_contract_id() -> ContractId {
    ContractId::from([0u8; 32])
}

pub struct FakeQueryApi {
    receiver: mpsc::Receiver<Query>,
}

impl FakeQueryApi {
    pub fn new_with_sender() -> (Self, mpsc::Sender<Query>) {
        let (sender, receiver) = mpsc::channel(10);
        (Self { receiver }, sender)
    }
}

impl QueryAPI for FakeQueryApi {
    async fn query(&mut self) -> crate::Result<Query> {
        self.receiver
            .recv()
            .await
            .ok_or_else(|| anyhow::anyhow!("No query received"))
    }
}

pub struct PendingQueryApi;

impl QueryAPI for PendingQueryApi {
    async fn query(&mut self) -> Result<Query> {
        pending().await
    }
}

fn arb_init_event() -> Event {
    let vrf_contract_id = [0u8; 32];
    let chip_asset_id = [1u8; 32];
    let roll_frequency = 10;
    let first_height = 100;
    Event::init_event(
        vrf_contract_id.into(),
        chip_asset_id.into(),
        roll_frequency,
        first_height,
    )
}

#[tokio::test]
async fn run__initialize_event__creates_first_snapshot() {
    // given
    let (event_source, event_sender) = FakeEventSource::new_with_sender();
    let snapshot_storage = InMemorySnapshotStorage::new();
    let snapshot_copy = snapshot_storage.snapshot();

    let metadata_storage = InMemoryMetadataStorage::default();
    let query_api = PendingQueryApi;
    let mut app = App::new(
        event_source,
        query_api,
        snapshot_storage,
        metadata_storage,
        zero_contract_id(),
    );

    let init_event = arb_init_event();
    let init_height = 100;

    // when
    event_sender
        .send((vec![init_event], init_height))
        .await
        .unwrap();
    app.run(pending()).await.unwrap();

    // then
    let mut expected = OverviewSnapshot::new();
    expected.current_block_height = init_height;
    let arb_roll_frequency = 10; // based on `arb_init_event`
    expected.next_roll_height = Some(init_height + arb_roll_frequency);
    expected.roll_frequency = Some(arb_roll_frequency);
    expected.first_roll_height = Some(init_height);
    let (actual, _) = snapshot_copy.lock().unwrap().clone().unwrap();
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn run__roll_event__updates_snapshot() {
    // given
    let (event_source, event_sender) = FakeEventSource::new_with_sender();
    let game_id = 1u32;
    let roll_index = 0u32;
    let rolled_value = Roll::Five;

    let existing_snapshot = OverviewSnapshot {
        game_id,
        ..OverviewSnapshot::default()
    };
    let snapshot_storage =
        InMemorySnapshotStorage::new_with_snapshot(existing_snapshot.clone(), 105);
    let snapshot_copy = snapshot_storage.snapshot();

    let metadata_storage = InMemoryMetadataStorage::default();
    let query_api = PendingQueryApi;
    let mut app = App::new(
        event_source,
        query_api,
        snapshot_storage,
        metadata_storage,
        zero_contract_id(),
    );
    app.roll_frequency = Some(10);

    let roll_event = Event::roll_event(game_id, roll_index, rolled_value.clone());
    let roll_height = 110;

    // when
    event_sender
        .send((vec![roll_event], roll_height))
        .await
        .unwrap();
    app.run(pending()).await.unwrap();

    // then
    let expected = {
        let mut snap = existing_snapshot;
        snap.rolls.push(rolled_value);
        snap.current_block_height = roll_height;
        snap
    };
    let (actual, _) = snapshot_copy.lock().unwrap().clone().unwrap();
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn run__new_game_event__resets_overview_snapshot() {
    // given
    let (event_source, event_sender) = FakeEventSource::new_with_sender();

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
        InMemorySnapshotStorage::new_with_snapshot(existing_snapshot.clone(), 200);
    let snapshot_copy = snapshot_storage.snapshot();
    let historical_copy = snapshot_storage.historical_snapshots();

    let metadata_storage = InMemoryMetadataStorage::default();
    let query_api = PendingQueryApi;
    let mut app = App::new(
        event_source,
        query_api,
        snapshot_storage,
        metadata_storage,
        zero_contract_id(),
    );

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
        .send((vec![Event::ContractEvent(new_game_event)], new_game_height))
        .await
        .unwrap();
    app.run(pending()).await.unwrap();

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
    expected.pot_size = existing_snapshot.pot_size;
    expected.current_block_height = new_game_height;
    assert_eq!(expected, actual);

    let historical = historical_copy.lock().unwrap();
    let stored = historical
        .get(&existing_snapshot.game_id)
        .expect("expected historical snapshot");
    assert_eq!(stored.game_id, existing_snapshot.game_id);
    assert_eq!(stored.rolls, existing_snapshot.rolls);
    assert!(stored.modifiers.is_empty());
    assert_eq!(stored.strap_rewards, existing_snapshot.rewards);
}

#[tokio::test]
async fn run__multiple_new_game_events__persists_historical_snapshots() {
    let (event_source, event_sender) = FakeEventSource::new_with_sender();

    let first_snapshot = OverviewSnapshot {
        game_id: 1,
        rolls: vec![Roll::Two, Roll::Three],
        pot_size: 250,
        ..OverviewSnapshot::default()
    };

    let snapshot_storage =
        InMemorySnapshotStorage::new_with_snapshot(first_snapshot.clone(), 150);
    let historical_copy = snapshot_storage.historical_snapshots();

    let metadata_storage = InMemoryMetadataStorage::default();
    let query_api = PendingQueryApi;
    let mut app = App::new(
        event_source,
        query_api,
        snapshot_storage,
        metadata_storage,
        zero_contract_id(),
    );

    let first_new_game = ContractEvent::NewGame(NewGameEvent {
        game_id: 2,
        new_straps: vec![],
        new_modifiers: vec![],
    });
    let second_new_game = ContractEvent::NewGame(NewGameEvent {
        game_id: 3,
        new_straps: vec![],
        new_modifiers: vec![],
    });

    event_sender
        .send((vec![Event::ContractEvent(first_new_game)], 210))
        .await
        .unwrap();
    app.run(pending()).await.unwrap();

    // update snapshot to imitate game progress
    let mut mid_snapshot = OverviewSnapshot::default();
    mid_snapshot.game_id = 2;
    mid_snapshot.pot_size = 500;
    app.snapshots
        .update_snapshot(&mid_snapshot, 220)
        .expect("update snapshot");

    event_sender
        .send((vec![Event::ContractEvent(second_new_game)], 230))
        .await
        .unwrap();
    app.run(pending()).await.unwrap();

    let historical = historical_copy.lock().unwrap();
    let stored_first = historical
        .get(&first_snapshot.game_id)
        .expect("missing first historical snapshot");
    assert_eq!(stored_first.rolls, first_snapshot.rolls);
    assert!(stored_first.modifiers.is_empty());
    assert_eq!(stored_first.strap_rewards, first_snapshot.rewards);

    let stored_second = historical
        .get(&2)
        .expect("missing second historical snapshot");
    assert_eq!(stored_second.rolls, mid_snapshot.rolls);
    assert!(stored_second.modifiers.is_empty());
    assert_eq!(stored_second.strap_rewards, mid_snapshot.rewards);
}

#[tokio::test]
async fn run__new_game_event__captures_triggered_modifiers_in_history() {
    let (event_source, event_sender) = FakeEventSource::new_with_sender();

    let existing_snapshot = OverviewSnapshot {
        game_id: 5,
        modifier_shop: vec![(Roll::Three, Roll::Four, Modifier::Holy, false)],
        rolls: vec![Roll::Two],
        ..OverviewSnapshot::default()
    };
    let snapshot_storage =
        InMemorySnapshotStorage::new_with_snapshot(existing_snapshot.clone(), 300);
    let historical_copy = snapshot_storage.historical_snapshots();

    let metadata_storage = InMemoryMetadataStorage::default();
    let query_api = PendingQueryApi;
    let mut app = App::new(
        event_source,
        query_api,
        snapshot_storage,
        metadata_storage,
        zero_contract_id(),
    );
    app.roll_frequency = Some(10);

    let expected_index = 321;
    let modifier_event = ContractEvent::ModifierTriggered(ModifierTriggeredEvent {
        game_id: existing_snapshot.game_id,
        roll_index: expected_index,
        trigger_roll: Roll::Two,
        modifier_roll: Roll::Four,
        modifier: Modifier::Holy,
    });

    event_sender
        .send((vec![Event::ContractEvent(modifier_event)], 305))
        .await
        .unwrap();
    app.run(pending()).await.unwrap();

    let new_game_event = ContractEvent::NewGame(NewGameEvent {
        game_id: 6,
        new_straps: vec![],
        new_modifiers: vec![],
    });

    event_sender
        .send((vec![Event::ContractEvent(new_game_event)], 310))
        .await
        .unwrap();
    app.run(pending()).await.unwrap();

    let historical = historical_copy.lock().unwrap();
    let stored = historical
        .get(&existing_snapshot.game_id)
        .expect("expected historical snapshot");
    assert_eq!(stored.rolls, existing_snapshot.rolls);
    assert_eq!(stored.strap_rewards, existing_snapshot.rewards);
    let active_modifier = ActiveModifier::new(expected_index, Modifier::Holy, Roll::Four);
    assert_eq!(stored.modifiers, vec![active_modifier]);

    // ensure modifiers for new game start fresh
    let next_entry = historical.get(&6);
    assert!(next_entry.is_none());
}

#[tokio::test]
async fn run__modifier_triggered_event__activates_modifier() {
    // given
    let (event_source, event_sender) = FakeEventSource::new_with_sender();

    let existing_snapshot = OverviewSnapshot {
        game_id: 5,
        modifier_shop: vec![(Roll::Three, Roll::Four, Modifier::Holy, false)],
        ..OverviewSnapshot::default()
    };
    let snapshot_storage =
        InMemorySnapshotStorage::new_with_snapshot(existing_snapshot.clone(), 220);
    let snapshot_copy = snapshot_storage.snapshot();

    let metadata_storage = InMemoryMetadataStorage::default();
    let query_api = PendingQueryApi;
    let mut app = App::new(
        event_source,
        query_api,
        snapshot_storage,
        metadata_storage,
        zero_contract_id(),
    );
    app.roll_frequency = Some(10);

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
        .send((vec![Event::ContractEvent(modifier_event)], event_height))
        .await
        .unwrap();
    app.run(pending()).await.unwrap();

    // then
    let (actual, _) = snapshot_copy.lock().unwrap().clone().unwrap();
    let mut expected = existing_snapshot;
    expected.modifiers_active[2] = Some(Modifier::Holy);
    expected.modifier_shop[0].3 = true;
    expected.current_block_height = event_height;
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn run__place_chip_bet_event__updates_pot_and_totals() {
    let (event_source, event_sender) = FakeEventSource::new_with_sender();

    let mut existing_snapshot = OverviewSnapshot::default();
    existing_snapshot.pot_size = 200;
    existing_snapshot.total_bets[4].0 = 50; // Roll::Six index

    let snapshot_storage =
        InMemorySnapshotStorage::new_with_snapshot(existing_snapshot.clone(), 300);
    let snapshot_copy = snapshot_storage.snapshot();

    let metadata_storage = InMemoryMetadataStorage::default();
    let query_api = PendingQueryApi;
    let mut app = App::new(
        event_source,
        query_api,
        snapshot_storage,
        metadata_storage,
        zero_contract_id(),
    );

    let chip_event = ContractEvent::PlaceChipBet(PlaceChipBetEvent {
        game_id: existing_snapshot.game_id,
        bet_roll_index: 0,
        player: Identity::Address(Address::from([0u8; 32])),
        roll: Roll::Six,
        amount: 150,
    });

    event_sender
        .send((vec![Event::ContractEvent(chip_event)], 305))
        .await
        .unwrap();
    app.run(pending()).await.unwrap();

    let (actual, _) = snapshot_copy.lock().unwrap().clone().unwrap();
    let mut expected = existing_snapshot;
    expected.pot_size = 350;
    expected.total_bets[4].0 = 200;
    expected.current_block_height = 305;
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn run__place_chip_bet_event__updates_account_snapshot() {
    let (event_source, event_sender) = FakeEventSource::new_with_sender();

    let snapshot_storage =
        InMemorySnapshotStorage::new_with_snapshot(OverviewSnapshot::default(), 300);
    let accounts_map = snapshot_storage.account_snapshots();

    let metadata_storage = InMemoryMetadataStorage::default();
    let query_api = PendingQueryApi;
    let mut app = App::new(
        event_source,
        query_api,
        snapshot_storage,
        metadata_storage,
        zero_contract_id(),
    );

    let player = Identity::Address(Address::from([0u8; 32]));
    let chip_event = ContractEvent::PlaceChipBet(PlaceChipBetEvent {
        game_id: 0,
        bet_roll_index: 0,
        player: player.clone(),
        roll: Roll::Six,
        amount: 150,
    });

    event_sender
        .send((vec![Event::ContractEvent(chip_event)], 305))
        .await
        .unwrap();
    app.run(pending()).await.unwrap();

    let key = InMemorySnapshotStorage::identity_key(&player);
    let account_guard = accounts_map.lock().unwrap();
    let game_id = 0;
    let (account_snapshot, _) = account_guard
        .get(&key)
        .unwrap()
        .get(&game_id)
        .cloned()
        .unwrap();
    assert_eq!(account_snapshot.total_chip_bet, 150);
    assert!(account_snapshot.strap_bets.is_empty());
    assert_eq!(account_snapshot.total_chip_won, 0);
    assert_eq!(account_snapshot.claimed_rewards, None);
    let roll_entry = account_snapshot
        .per_roll_bets
        .iter()
        .find(|entry| entry.roll == Roll::Six)
        .expect("missing roll entry for Six");
    assert_eq!(roll_entry.bets.len(), 1);
    let bet = &roll_entry.bets[0];
    assert_eq!(bet.amount, 150);
    assert_eq!(bet.bet_roll_index, 0);
    assert!(matches!(bet.kind, crate::snapshot::AccountBetKind::Chip));
}

#[tokio::test]
async fn run__place_strap_bet_event__records_strap_bet() {
    let (event_source, event_sender) = FakeEventSource::new_with_sender();

    let mut existing_snapshot = OverviewSnapshot::default();
    existing_snapshot.total_bets[3].1 =
        vec![(Strap::new(1, StrapKind::Gloves, Modifier::Lucky), 1)];

    let snapshot_storage =
        InMemorySnapshotStorage::new_with_snapshot(existing_snapshot.clone(), 410);
    let snapshot_copy = snapshot_storage.snapshot();

    let metadata_storage = InMemoryMetadataStorage::default();
    let query_api = PendingQueryApi;
    let mut app = App::new(
        event_source,
        query_api,
        snapshot_storage,
        metadata_storage,
        zero_contract_id(),
    );

    let strap = Strap::new(2, StrapKind::Gloves, Modifier::Lucky);
    let player = Identity::Address(Address::from([1u8; 32]));
    let strap_event = ContractEvent::PlaceStrapBet(PlaceStrapBetEvent {
        game_id: existing_snapshot.game_id,
        bet_roll_index: 3,
        player,
        roll: Roll::Five,
        strap: strap.clone(),
        amount: 2,
    });

    event_sender
        .send((vec![Event::ContractEvent(strap_event)], 415))
        .await
        .unwrap();
    app.run(pending()).await.unwrap();

    let (actual, _) = snapshot_copy.lock().unwrap().clone().unwrap();
    let mut expected = existing_snapshot;
    expected.total_bets[3].1.push((strap, 2));
    expected.current_block_height = 415;
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn run__place_strap_bet_event__updates_account_snapshot() {
    let (event_source, event_sender) = FakeEventSource::new_with_sender();

    let snapshot_storage =
        InMemorySnapshotStorage::new_with_snapshot(OverviewSnapshot::default(), 0);
    let accounts_map = snapshot_storage.account_snapshots();

    let metadata_storage = InMemoryMetadataStorage::default();
    let query_api = PendingQueryApi;
    let mut app = App::new(
        event_source,
        query_api,
        snapshot_storage,
        metadata_storage,
        zero_contract_id(),
    );

    let strap = Strap::new(2, StrapKind::Gloves, Modifier::Lucky);
    let expected_strap = strap.clone();
    let player = Identity::Address(Address::from([1u8; 32]));
    let strap_event = ContractEvent::PlaceStrapBet(PlaceStrapBetEvent {
        game_id: 0,
        bet_roll_index: 3,
        player: player.clone(),
        roll: Roll::Five,
        strap: strap.clone(),
        amount: 2,
    });

    event_sender
        .send((vec![Event::ContractEvent(strap_event)], 415))
        .await
        .unwrap();
    app.run(pending()).await.unwrap();

    let key = InMemorySnapshotStorage::identity_key(&player);
    let game_id = 0;
    let account_guard = accounts_map.lock().unwrap();
    let (account_snapshot, _) = account_guard
        .get(&key)
        .unwrap()
        .get(&game_id)
        .cloned()
        .unwrap();
    assert_eq!(account_snapshot.total_chip_bet, 0);
    assert_eq!(account_snapshot.total_chip_won, 0);
    assert_eq!(account_snapshot.claimed_rewards, None);
    assert_eq!(account_snapshot.strap_bets, vec![(expected_strap, 2)]);
    let strap_entry = account_snapshot
        .per_roll_bets
        .iter()
        .find(|entry| entry.roll == Roll::Five)
        .expect("missing roll entry for strap bet");
    assert_eq!(strap_entry.bets.len(), 1);
    let bet = &strap_entry.bets[0];
    assert_eq!(bet.amount, 2);
    assert_eq!(bet.bet_roll_index, 3);
    match &bet.kind {
        crate::snapshot::AccountBetKind::Strap(s) => assert_eq!(s, &strap),
        other => panic!("unexpected bet kind: {:?}", other),
    }
}

#[tokio::test]
async fn run__claim_rewards_event__reduces_pot() {
    let (event_source, event_sender) = FakeEventSource::new_with_sender();

    let mut existing_snapshot = OverviewSnapshot::default();
    existing_snapshot.pot_size = 500;

    let snapshot_storage =
        InMemorySnapshotStorage::new_with_snapshot(existing_snapshot.clone(), 510);
    let snapshot_copy = snapshot_storage.snapshot();

    let metadata_storage = InMemoryMetadataStorage::default();
    let query_api = PendingQueryApi;
    let mut app = App::new(
        event_source,
        query_api,
        snapshot_storage,
        metadata_storage,
        zero_contract_id(),
    );

    let claim_event = ContractEvent::ClaimRewards(ClaimRewardsEvent {
        game_id: existing_snapshot.game_id,
        player: Identity::Address(Address::from([2u8; 32])),
        enabled_modifiers: vec![],
        total_chips_winnings: 120,
        total_strap_winnings: vec![],
    });

    event_sender
        .send((vec![Event::ContractEvent(claim_event)], 515))
        .await
        .unwrap();
    app.run(pending()).await.unwrap();

    let (actual, _) = snapshot_copy.lock().unwrap().clone().unwrap();
    let mut expected = existing_snapshot;
    expected.pot_size = 380;
    expected.current_block_height = 515;
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn run__claim_rewards_event__updates_account_snapshot() {
    let (event_source, event_sender) = FakeEventSource::new_with_sender();

    let snapshot_storage =
        InMemorySnapshotStorage::new_with_snapshot(OverviewSnapshot::default(), 0);
    let accounts_map = snapshot_storage.account_snapshots();

    let metadata_storage = InMemoryMetadataStorage::default();
    let query_api = PendingQueryApi;
    let mut app = App::new(
        event_source,
        query_api,
        snapshot_storage,
        metadata_storage,
        zero_contract_id(),
    );

    let player = Identity::Address(Address::from([2u8; 32]));
    let claim_event = ContractEvent::ClaimRewards(ClaimRewardsEvent {
        game_id: 0,
        player: player.clone(),
        enabled_modifiers: vec![],
        total_chips_winnings: 120,
        total_strap_winnings: vec![],
    });

    event_sender
        .send((vec![Event::ContractEvent(claim_event)], 515))
        .await
        .unwrap();
    app.run(pending()).await.unwrap();

    let game_id = 0;
    let key = InMemorySnapshotStorage::identity_key(&player);
    let account_guard = accounts_map.lock().unwrap();
    let (account_snapshot, _) = account_guard
        .get(&key)
        .unwrap()
        .get(&game_id)
        .cloned()
        .unwrap();
    assert_eq!(account_snapshot.total_chip_bet, 0);
    assert!(account_snapshot.strap_bets.is_empty());
    assert_eq!(account_snapshot.total_chip_won, 120);
    assert_eq!(account_snapshot.claimed_rewards, Some((120, Vec::new())));
}

#[tokio::test]
async fn run__claim_rewards_event__records_strap_winnings_in_account_snapshot() {
    let (event_source, event_sender) = FakeEventSource::new_with_sender();

    let snapshot_storage =
        InMemorySnapshotStorage::new_with_snapshot(OverviewSnapshot::default(), 0);
    let accounts_map = snapshot_storage.account_snapshots();

    let metadata_storage = InMemoryMetadataStorage::default();
    let query_api = PendingQueryApi;
    let mut app = App::new(
        event_source,
        query_api,
        snapshot_storage,
        metadata_storage,
        zero_contract_id(),
    );

    let strap_a = Strap::new(1, StrapKind::Hat, Modifier::Lucky);
    let strap_b = Strap::new(2, StrapKind::Ring, Modifier::Delicate);
    let new_game = ContractEvent::NewGame(NewGameEvent {
        game_id: 1,
        new_straps: vec![
            (Roll::Two, strap_a.clone(), 0),
            (Roll::Three, strap_b.clone(), 0),
        ],
        new_modifiers: vec![],
    });
    event_sender
        .send((vec![Event::ContractEvent(new_game)], 100))
        .await
        .unwrap();
    app.run(pending()).await.unwrap();

    let player = Identity::Address(Address::from([9u8; 32]));
    let claim_event = ContractEvent::ClaimRewards(ClaimRewardsEvent {
        game_id: 0,
        player: player.clone(),
        enabled_modifiers: vec![(Roll::Two, Modifier::Lucky)],
        total_chips_winnings: 75,
        total_strap_winnings: vec![(strap_a.clone(), 1), (strap_b.clone(), 2)],
    });

    event_sender
        .send((vec![Event::ContractEvent(claim_event)], 520))
        .await
        .unwrap();
    app.run(pending()).await.unwrap();

    let key = InMemorySnapshotStorage::identity_key(&player);
    let game_id = 0;
    let account_guard = accounts_map.lock().unwrap();
    let (account_snapshot, _) = account_guard
        .get(&key)
        .unwrap()
        .get(&game_id)
        .cloned()
        .unwrap();
    assert_eq!(account_snapshot.total_chip_bet, 0);
    assert!(account_snapshot.strap_bets.is_empty());
    assert_eq!(account_snapshot.total_chip_won, 75);
    assert_eq!(
        account_snapshot.claimed_rewards,
        Some((75, vec![(strap_a, 1), (strap_b, 2)]))
    );
}

#[tokio::test]
async fn run__fund_pot_event__increases_pot() {
    // given
    let (event_source, event_sender) = FakeEventSource::new_with_sender();

    let mut existing_snapshot = OverviewSnapshot::default();
    existing_snapshot.pot_size = 75;

    let snapshot_storage =
        InMemorySnapshotStorage::new_with_snapshot(existing_snapshot.clone(), 610);
    let snapshot_copy = snapshot_storage.snapshot();

    let metadata_storage = InMemoryMetadataStorage::default();
    let query_api = PendingQueryApi;
    let mut app = App::new(
        event_source,
        query_api,
        snapshot_storage,
        metadata_storage,
        zero_contract_id(),
    );

    let fund_event = ContractEvent::FundPot(FundPotEvent {
        chips_amount: 325,
        funder: Identity::Address(Address::from([3u8; 32])),
    });

    // when
    event_sender
        .send((vec![Event::ContractEvent(fund_event)], 615))
        .await
        .unwrap();
    app.run(pending()).await.unwrap();

    // then
    let (actual, _) = snapshot_copy.lock().unwrap().clone().unwrap();
    let mut expected = existing_snapshot;
    expected.pot_size = 400;
    expected.current_block_height = 615;
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn run__purchase_modifier_event__marks_shop_entry() {
    // given
    let (event_source, event_sender) = FakeEventSource::new_with_sender();

    let mut existing_snapshot = OverviewSnapshot::default();
    existing_snapshot.modifier_shop =
        vec![(Roll::Two, Roll::Four, Modifier::Holy, false)];

    let snapshot_storage =
        InMemorySnapshotStorage::new_with_snapshot(existing_snapshot.clone(), 710);
    let snapshot_copy = snapshot_storage.snapshot();

    let metadata_storage = InMemoryMetadataStorage::default();
    let query_api = PendingQueryApi;
    let mut app = App::new(
        event_source,
        query_api,
        snapshot_storage,
        metadata_storage,
        zero_contract_id(),
    );

    let purchase_event = ContractEvent::PurchaseModifier(PurchaseModifierEvent {
        expected_roll: Roll::Four,
        expected_modifier: Modifier::Holy,
        purchaser: Identity::Address(Address::from([4u8; 32])),
    });

    // when
    event_sender
        .send((vec![Event::ContractEvent(purchase_event)], 715))
        .await
        .unwrap();
    app.run(pending()).await.unwrap();

    // then
    let (actual, _) = snapshot_copy.lock().unwrap().clone().unwrap();
    let mut expected = existing_snapshot;
    expected.modifiers_active[2] = Some(Modifier::Holy);
    expected.modifier_shop[0].3 = true;
    expected.current_block_height = 715;
    assert_eq!(expected, actual);
}

fn arb_snapshot() -> OverviewSnapshot {
    OverviewSnapshot {
        game_id: 1234,
        rolls: vec![Roll::Two, Roll::Three, Roll::Four, Roll::Five, Roll::Six],
        pot_size: 999999999,
        current_block_height: 123,
        next_roll_height: Some(333),
        roll_frequency: Some(10),
        first_roll_height: Some(123),
        rewards: vec![(
            Roll::Ten,
            Strap {
                level: 88,
                kind: StrapKind::Shirt,
                modifier: Modifier::Nothing,
            },
            4444,
        )],
        total_bets: [
            (100, vec![]),
            (200, vec![]),
            (300, vec![]),
            (400, vec![]),
            (500, vec![]),
            (600, vec![]),
            (700, vec![]),
            (800, vec![]),
            (900, vec![]),
            (1000, vec![]),
            (1100, vec![]),
        ],
        modifiers_active: [None; 11],
        modifier_shop: vec![],
    }
}
#[tokio::test]
async fn run__latest_snapshot_query__returns_latest_snapshot() {
    // given
    let snapshot = arb_snapshot();
    let height = 1000;

    let snapshot_storage =
        InMemorySnapshotStorage::new_with_snapshot(snapshot.clone(), height);

    let (query_api, sender) = FakeQueryApi::new_with_sender();
    let mut app = App::new(
        PendingEventSource,
        query_api,
        snapshot_storage,
        InMemoryMetadataStorage::default(),
        zero_contract_id(),
    );

    // when
    let (one_send, one_recv) = oneshot::channel();
    let query = Query::LatestSnapshot(one_send);
    sender.send(query).await.unwrap();
    app.run(pending()).await.unwrap();

    // then
    let response = one_recv.await.unwrap();
    assert_eq!(response, (snapshot, height));
}

#[tokio::test]
async fn run__latest_account_snapshot_query__returns_latest_account_snapshot() {
    // given
    let overview_snapshot = OverviewSnapshot {
        game_id: 42,
        ..OverviewSnapshot::default()
    };
    let overview_height = 500;
    let identity = Identity::Address(Address::from([9u8; 32]));
    let expected_snapshot = crate::snapshot::AccountSnapshot::default();
    let expected_height = 777;

    let mut snapshot_storage = InMemorySnapshotStorage::new();
    snapshot_storage
        .update_snapshot(&overview_snapshot, overview_height)
        .unwrap();
    snapshot_storage
        .update_account_snapshot(
            &identity,
            overview_snapshot.game_id,
            &expected_snapshot,
            expected_height,
        )
        .unwrap();

    let (query_api, sender) = FakeQueryApi::new_with_sender();
    let mut app = App::new(
        PendingEventSource,
        query_api,
        snapshot_storage,
        InMemoryMetadataStorage::default(),
        zero_contract_id(),
    );

    // when
    let (one_send, one_recv) = oneshot::channel();
    let query = Query::latest_account_summary(identity.clone(), one_send);
    sender.send(query).await.unwrap();
    app.run(pending()).await.unwrap();

    // then
    let response = one_recv.await.unwrap();
    assert_eq!(response, Some((expected_snapshot, expected_height)));
}

#[tokio::test]
async fn run__historical_snapshot_query__returns_historical_snapshot() {
    // given
    let game_id = 1337u32;
    let expected_snapshot = crate::snapshot::HistoricalSnapshot::new(
        game_id,
        vec![Roll::Six, Roll::Seven],
        vec![ActiveModifier::new(3, Modifier::Lucky, Roll::Six)],
    );

    let mut snapshot_storage = InMemorySnapshotStorage::new();
    snapshot_storage
        .write_historical_snapshot(game_id, &expected_snapshot)
        .unwrap();

    let (query_api, sender) = FakeQueryApi::new_with_sender();
    let mut app = App::new(
        PendingEventSource,
        query_api,
        snapshot_storage,
        InMemoryMetadataStorage::default(),
        zero_contract_id(),
    );

    // when
    let (one_send, one_recv) = oneshot::channel();
    let query = Query::historical_snapshot(game_id, one_send);
    sender.send(query).await.unwrap();
    app.run(pending()).await.unwrap();

    // then
    let response = one_recv.await.unwrap();
    assert_eq!(response, Some(expected_snapshot));
}

#[tokio::test]
async fn run__historical_account_snapshot_query__returns_historical_account_snapshot() {
    // given
    let identity = Identity::Address(Address::from([5u8; 32]));
    let game_id = 21u32;
    let mut expected_snapshot = crate::snapshot::AccountSnapshot::default();
    expected_snapshot.total_chip_bet = 99;
    expected_snapshot
        .strap_bets
        .push((Strap::new(1, StrapKind::Hat, Modifier::Lucky), 88));
    expected_snapshot.total_chip_won = 77;
    expected_snapshot.claimed_rewards = Some((55, vec![]));
    let expected_height = 4242;

    let mut snapshot_storage = InMemorySnapshotStorage::new();
    snapshot_storage
        .update_account_snapshot(&identity, game_id, &expected_snapshot, expected_height)
        .unwrap();

    let (query_api, sender) = FakeQueryApi::new_with_sender();
    let mut app = App::new(
        PendingEventSource,
        query_api,
        snapshot_storage,
        InMemoryMetadataStorage::default(),
        zero_contract_id(),
    );

    // when
    let (one_send, one_recv) = oneshot::channel();
    let query = Query::historical_account_summary(identity.clone(), game_id, one_send);
    sender.send(query).await.unwrap();
    app.run(pending()).await.unwrap();

    // then
    let response = one_recv.await.unwrap();
    assert_eq!(response, Some((expected_snapshot, expected_height)));
}
