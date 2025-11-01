#![allow(non_snake_case)]

use super::*;
use crate::events::ContractEvent;
use fuel_core::state::rocks_db::ColumnsPolicy;
use fuels::{
    prelude::{
        AssetConfig,
        AssetId,
        Contract,
        LoadConfiguration,
        TxPolicies,
        WalletsConfig,
        launch_custom_provider_and_get_wallets,
    },
    programs::calls::Execution,
    types::Bits256,
};
use generated_abi::{
    get_contract_instance,
    vrf_types::FakeVRFContract,
};
use url::Url;

#[tokio::test]
async fn next_event_batch__can_get_init_event() {
    let chip_asset_id = AssetId::new([1u8; 32]);
    let base_assets = vec![
        AssetConfig {
            id: AssetId::zeroed(),
            num_coins: 1,
            coin_amount: 10_000_000_000,
        },
        AssetConfig {
            id: chip_asset_id,
            num_coins: 1,
            coin_amount: 10_000_000_000,
        },
    ];
    let temp_dir = tempdir::TempDir::new("database")
        .unwrap()
        .path()
        .to_path_buf();

    let database_config = DatabaseConfig {
        cache_capacity: None,
        max_fds: 512,
        columns_policy: ColumnsPolicy::Lazy,
    };
    let mut wallets = launch_custom_provider_and_get_wallets(
        WalletsConfig::new_multiple_assets(1, base_assets),
        None,
        None,
    )
    .await
    .expect("failed to launch local provider");
    let wallet = wallets.pop().unwrap();

    let (contract_instance, _contract_id) = get_contract_instance(wallet.clone()).await;
    let address = wallet.provider().url();
    let indexer_config = fuel_indexer::indexer::IndexerConfig::new(
        0u32.into(),
        Url::parse(address).unwrap(),
    );

    let mut event_source = FuelIndexerEventSource::new(
        parse_event_logs,
        temp_dir,
        database_config,
        indexer_config,
        BlockHeight::from(0u32),
    )
    .await
    .unwrap();

    // given
    let fake_vrf_contract_id = [5; 32];

    // when
    contract_instance
        .methods()
        .initialize(Bits256(fake_vrf_contract_id), chip_asset_id.clone(), 100)
        .call()
        .await
        .unwrap();

    // then
    let _should_be_empty_first_block = event_source.next_event_batch().await.unwrap();
    let _checkpoint = event_source.next_event_batch().await.unwrap();
    let (events, _) = event_source.next_event_batch().await.unwrap();
    let actual = events.first().unwrap();
    let expected = Event::init_event(fake_vrf_contract_id.into(), chip_asset_id, 100, 2);

    assert_eq!(actual, &expected);
}

#[tokio::test]
async fn next_event_batch__can_get_roll_event() {
    let chip_asset_id = AssetId::new([1u8; 32]);
    let base_assets = vec![
        AssetConfig {
            id: AssetId::zeroed(),
            num_coins: 1,
            coin_amount: 10_000_000_000,
        },
        AssetConfig {
            id: chip_asset_id,
            num_coins: 1,
            coin_amount: 10_000_000_000,
        },
    ];
    let temp_dir = tempdir::TempDir::new("database")
        .unwrap()
        .path()
        .to_path_buf();

    let database_config = DatabaseConfig {
        cache_capacity: None,
        max_fds: 512,
        columns_policy: ColumnsPolicy::Lazy,
    };
    let mut wallets = launch_custom_provider_and_get_wallets(
        WalletsConfig::new_multiple_assets(1, base_assets),
        None,
        None,
    )
    .await
    .expect("failed to launch local provider");
    let wallet = wallets.pop().unwrap();

    let (contract_instance, _) = get_contract_instance(wallet.clone()).await;

    let vrf_bin_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../sway-projects/fake-vrf-contract/out/release/fake-vrf-contract.bin");
    let vrf_contract = Contract::load_from(vrf_bin_path, LoadConfiguration::default())
        .expect("failed to load fake vrf contract");
    let deployment = vrf_contract
        .deploy(&wallet, TxPolicies::default())
        .await
        .expect("failed to deploy fake vrf contract");
    let vrf_contract_id = deployment.contract_id.clone();
    let vrf_instance = FakeVRFContract::new(vrf_contract_id.clone(), wallet.clone());

    let address = wallet.provider().url();
    let indexer_config = fuel_indexer::indexer::IndexerConfig::new(
        0u32.into(),
        Url::parse(address).unwrap(),
    );

    let mut event_source = FuelIndexerEventSource::new(
        parse_event_logs,
        temp_dir,
        database_config,
        indexer_config,
        BlockHeight::from(0u32),
    )
    .await
    .unwrap();

    contract_instance
        .methods()
        .initialize(Bits256(*vrf_contract_id), chip_asset_id.clone(), 1)
        .call()
        .await
        .unwrap();

    let provider = wallet.provider();
    let next_roll_height = contract_instance
        .methods()
        .next_roll_height()
        .simulate(Execution::state_read_only())
        .await
        .unwrap()
        .value
        .expect("expected next roll height");
    let current_height = provider
        .latest_block_height()
        .await
        .expect("failed to read current block height");
    if next_roll_height > current_height {
        provider
            .produce_blocks(next_roll_height - current_height, None)
            .await
            .expect("failed to advance blocks");
    }

    let vrf_number = 0u64;
    vrf_instance
        .methods()
        .set_number(vrf_number)
        .call()
        .await
        .unwrap();

    // when
    contract_instance
        .methods()
        .roll_dice()
        .with_contracts(&[&vrf_instance])
        .call()
        .await
        .unwrap();

    // then
    let mut actual_event = None;
    for _ in 0..10 {
        let (events, _) = event_source.next_event_batch().await.unwrap();
        if let Some(event) = events
            .into_iter()
            .find(|event| matches!(event, Event::ContractEvent(ContractEvent::Roll(_))))
        {
            actual_event = Some(event);
            break;
        }
    }

    let actual_event = actual_event.expect("expected to receive roll event");
    let expected = Event::roll_event(0, 1, crate::events::Roll::Two);
    assert_eq!(actual_event, expected);
}

#[tokio::test]
async fn next_event_batch__can_get_new_game_event() {
    let chip_asset_id = AssetId::new([1u8; 32]);
    let base_assets = vec![
        AssetConfig {
            id: AssetId::zeroed(),
            num_coins: 1,
            coin_amount: 10_000_000_000,
        },
        AssetConfig {
            id: chip_asset_id,
            num_coins: 1,
            coin_amount: 10_000_000_000,
        },
    ];
    let temp_dir = tempdir::TempDir::new("database")
        .unwrap()
        .path()
        .to_path_buf();

    let database_config = DatabaseConfig {
        cache_capacity: None,
        max_fds: 512,
        columns_policy: ColumnsPolicy::Lazy,
    };
    let mut wallets = launch_custom_provider_and_get_wallets(
        WalletsConfig::new_multiple_assets(1, base_assets),
        None,
        None,
    )
    .await
    .expect("failed to launch local provider");
    let wallet = wallets.pop().unwrap();

    let (contract_instance, _) = get_contract_instance(wallet.clone()).await;

    let vrf_bin_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../sway-projects/fake-vrf-contract/out/release/fake-vrf-contract.bin");
    let vrf_contract = Contract::load_from(vrf_bin_path, LoadConfiguration::default())
        .expect("failed to load fake vrf contract");
    let deployment = vrf_contract
        .deploy(&wallet, TxPolicies::default())
        .await
        .expect("failed to deploy fake vrf contract");
    let vrf_contract_id = deployment.contract_id.clone();
    let vrf_instance = FakeVRFContract::new(vrf_contract_id.clone(), wallet.clone());

    let address = wallet.provider().url();
    let indexer_config = fuel_indexer::indexer::IndexerConfig::new(
        0u32.into(),
        Url::parse(address).unwrap(),
    );

    let mut event_source = FuelIndexerEventSource::new(
        parse_event_logs,
        temp_dir,
        database_config,
        indexer_config,
        BlockHeight::from(0u32),
    )
    .await
    .unwrap();

    let seven_vrf_number = 15u64;

    contract_instance
        .methods()
        .initialize(Bits256(*vrf_contract_id), chip_asset_id.clone(), 1)
        .call()
        .await
        .unwrap();

    let provider = wallet.provider();
    let next_roll_height = contract_instance
        .methods()
        .next_roll_height()
        .simulate(Execution::state_read_only())
        .await
        .unwrap()
        .value
        .expect("expected next roll height");
    let current_height = provider
        .latest_block_height()
        .await
        .expect("failed to read current block height");
    if next_roll_height > current_height {
        provider
            .produce_blocks(next_roll_height - current_height, None)
            .await
            .expect("failed to advance blocks");
    }

    vrf_instance
        .methods()
        .set_number(seven_vrf_number)
        .call()
        .await
        .unwrap();

    contract_instance
        .methods()
        .roll_dice()
        .with_contracts(&[&vrf_instance])
        .call()
        .await
        .unwrap();

    let mut actual_new_game = None;
    for _ in 0..10 {
        let (events, _) = event_source.next_event_batch().await.unwrap();
        for event in events {
            if let Event::ContractEvent(ContractEvent::NewGame(inner)) = event {
                actual_new_game = Some(inner);
                break;
            }
        }
        if actual_new_game.is_some() {
            break;
        }
    }
    if actual_new_game.is_none() {
        panic!("expected to receive new game event");
    }
}

#[tokio::test]
#[ignore = "TODO: implement modifier triggered event integration"]
async fn next_event_batch__can_get_modifier_triggered_event() {
    todo!("implement modifier triggered event test");
}

#[tokio::test]
#[ignore = "TODO: implement place chip bet event integration"]
async fn next_event_batch__can_get_place_chip_bet_event() {
    todo!("implement place chip bet event test");
}

#[tokio::test]
#[ignore = "TODO: implement place strap bet event integration"]
async fn next_event_batch__can_get_place_strap_bet_event() {
    todo!("implement place strap bet event test");
}

#[tokio::test]
#[ignore = "TODO: implement claim rewards event integration"]
async fn next_event_batch__can_get_claim_rewards_event() {
    todo!("implement claim rewards event test");
}

#[tokio::test]
#[ignore = "TODO: implement fund pot event integration"]
async fn next_event_batch__can_get_fund_pot_event() {
    todo!("implement fund pot event test");
}

#[tokio::test]
#[ignore = "TODO: implement purchase modifier event integration"]
async fn next_event_batch__can_get_purchase_modifier_event() {
    todo!("implement purchase modifier event test");
}
