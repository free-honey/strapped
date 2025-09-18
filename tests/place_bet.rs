use super::*;

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
