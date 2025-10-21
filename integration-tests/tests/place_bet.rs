#![allow(non_snake_case)]

use fuels::{
    prelude::{
        AssetConfig,
        CallParameters,
        Execution,
    },
    tx::ContractIdExt,
};
use generated_abi::{
    contract_id,
    strap_to_sub_id,
    strapped_types::{
        Bet,
        Modifier,
        Roll,
        Strap,
        StrapKind,
    },
    test_helpers::TestContext,
};

#[tokio::test]
async fn place_bet__adds_bets_to_list() {
    let ctx = TestContext::new().await;
    let chip_asset_id = ctx.chip_asset_id();

    // given
    let roll = Roll::Six;
    let bet_amount = 100;

    // when
    ctx.alice_instance()
        .methods()
        .place_bet(roll.clone(), Bet::Chip, bet_amount)
        .call_params(CallParameters::new(bet_amount, chip_asset_id, 1_000_000))
        .unwrap()
        .call()
        .await
        .unwrap();

    // then
    let actual = ctx
        .alice_instance()
        .methods()
        .get_my_bets(roll.clone())
        .simulate(Execution::Realistic)
        .await
        .unwrap()
        .value;
    let expected = vec![(Bet::Chip, bet_amount, 0)];
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn place_bet__fails_if_funds_not_transferred() {
    let ctx = TestContext::new().await;
    // given
    let roll = Roll::Six;
    let bet_amount = 100;

    // when
    let result = ctx
        .alice_instance()
        .methods()
        .place_bet(roll, Bet::Chip, bet_amount)
        .call()
        .await;

    // then
    assert!(result.is_err());
}

#[tokio::test]
async fn place_bet__can_bet_strap() {
    let contract_id = contract_id();
    let strap = Strap::new(1, StrapKind::Shirt, Modifier::Nothing);
    let strap_sub_id = strap_to_sub_id(&strap);
    let strap_asset_id = contract_id.asset_id(&strap_sub_id);

    let ctx = TestContext::new_with_extra_assets(vec![AssetConfig {
        id: strap_asset_id,
        num_coins: 1,
        coin_amount: 1,
    }])
    .await;

    // given
    let strap_asset_id = ctx.contract_id().asset_id(&strap_sub_id);
    let roll = Roll::Six;
    let bet_amount = 1;

    // when
    ctx.alice_instance()
        .methods()
        .place_bet(roll.clone(), Bet::Strap(strap.clone()), bet_amount)
        .call_params(CallParameters::new(bet_amount, strap_asset_id, 1_000_000))
        .unwrap()
        .call()
        .await
        .unwrap();

    // then
    let actual = ctx
        .alice_instance()
        .methods()
        .get_my_bets(roll)
        .simulate(Execution::Realistic)
        .await
        .unwrap()
        .value;
    let expected = vec![(Bet::Strap(strap), bet_amount, 0)];
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn place_bet__fails_if_does_not_include_strap() {
    let strap = Strap::new(1, StrapKind::Shirt, Modifier::Nothing);
    let roll = Roll::Six;
    let bet_amount = 1;

    let ctx = TestContext::new().await;

    // when
    let result = ctx
        .alice_instance()
        .methods()
        .place_bet(roll, Bet::Strap(strap), bet_amount)
        .call()
        .await;

    // then
    assert!(result.is_err());
}
