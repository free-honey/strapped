#![allow(non_snake_case)]
use fuels::{
    prelude::{AssetConfig, AssetId, CallParameters},
    tx::ContractIdExt,
};
use strapped_contract::{
    contract_id, get_contract_instance, strap_to_sub_id, strapped_types,
    strapped_types::Strap, test_helpers::TestContext,
};

#[tokio::test]
async fn place_bet__adds_bets_to_list() {
    let asset_id = AssetId::new([1; 32]);
    let ctx = TestContext::new().await;
    let alice = ctx.alice();
    // given
    let (instance, _id) = get_contract_instance(alice.clone()).await;
    let bet_amount = 100;
    let bet = strapped_types::Bet::Chip;
    let roll = strapped_types::Roll::Six;
    instance
        .methods()
        .set_chip_asset_id(asset_id)
        .call()
        .await
        .unwrap();

    // when
    let call_params = CallParameters::new(bet_amount, asset_id, 1_000_000);
    instance
        .methods()
        .place_bet(roll.clone(), bet.clone(), bet_amount)
        .call_params(call_params)
        .unwrap()
        .call()
        .await
        .unwrap();

    // then
    let actual = instance
        .methods()
        .get_my_bets(roll)
        .call()
        .await
        .unwrap()
        .value;
    let expected = vec![(bet, bet_amount, 0)];
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn place_bet__fails_if_funds_not_transferred() {
    let ctx = TestContext::new().await;
    let alice = ctx.alice();
    // given
    let (instance, _id) = get_contract_instance(alice.clone()).await;
    let bet_amount = 100;
    let bet = strapped_types::Bet::Chip;
    let roll = strapped_types::Roll::Six;

    // when
    let result = instance
        .methods()
        .place_bet(roll.clone(), bet.clone(), bet_amount)
        .call()
        .await;

    // then
    assert!(result.is_err());
}

#[tokio::test]
async fn place_bet__can_bet_strap() {
    // given
    let contract_id = contract_id();
    let roll = strapped_types::Roll::Six;

    // when
    let level = 1;
    let kind = strapped_types::StrapKind::Shirt;
    let modifier = strapped_types::Modifier::Nothing;
    let strap = Strap {
        level,
        kind: kind.clone(),
        modifier: modifier.clone(),
    };
    let bet_amount = 1;
    let bet = strapped_types::Bet::Strap(strap.clone());
    let sub_id = strap_to_sub_id(&strap);
    let asset_id = contract_id.asset_id(&sub_id);

    let ctx = TestContext::new_with_extra_assets(vec![AssetConfig {
        id: asset_id.clone(),
        num_coins: 1,
        coin_amount: 1,
    }])
    .await;
    let alice = ctx.alice();
    let (instance, _contract_id) = get_contract_instance(alice.clone()).await;
    let call_params = CallParameters::new(bet_amount, asset_id, 1_000_000);
    instance
        .methods()
        .place_bet(roll.clone(), bet.clone(), bet_amount)
        .call_params(call_params)
        .unwrap()
        .call()
        .await
        .unwrap();

    // then
    let actual = instance
        .methods()
        .get_my_bets(roll)
        .call()
        .await
        .unwrap()
        .value;
    let expected = vec![(bet, bet_amount, 0)];
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn place_bet__fails_if_does_not_include_strap() {
    // given
    let roll = strapped_types::Roll::Six;
    let ctx = TestContext::new().await;
    let alice = ctx.alice();
    let (instance, _contract_id) = get_contract_instance(alice.clone()).await;

    let level = 1;
    let kind = strapped_types::StrapKind::Shirt;
    let modifier = strapped_types::Modifier::Nothing;
    let strap = Strap {
        level,
        kind: kind.clone(),
        modifier: modifier.clone(),
    };
    let bet_amount = 1;
    let bet = strapped_types::Bet::Strap(strap.clone());

    // when
    let result = instance
        .methods()
        .place_bet(roll.clone(), bet.clone(), bet_amount)
        .call()
        .await;

    // then
    assert!(result.is_err());
}
