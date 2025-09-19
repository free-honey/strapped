#![allow(non_snake_case)]
use fuels::{
    accounts::ViewOnlyAccount,
    prelude::{
        AssetConfig,
        AssetId,
        CallParameters,
        VariableOutputPolicy,
    },
    tx::ContractIdExt,
    types::Bits256,
};
use strapped_contract::{
    contract_id,
    get_contract_instance,
    separate_contract_instance,
    strap_to_sub_id,
    strapped_types,
    strapped_types::{
        Bet,
        Modifier,
        Roll,
        Strap,
        StrapKind,
    },
    test_helpers::*,
};

#[tokio::test]
async fn claim_rewards__adds_chips_to_wallet() {
    let ctx = TestContext::new().await;
    let owner = ctx.owner();
    let chip_asset_id = AssetId::new([1; 32]);

    // given
    // init contracts
    let (instance, contract_id) = get_contract_instance(owner.clone()).await;
    let alice_instance = separate_contract_instance(&contract_id, ctx.alice()).await;
    let (vrf_instance, vrf_contract_id) = get_vrf_contract_instance(owner).await;
    instance
        .methods()
        .set_vrf_contract_id(Bits256(*vrf_contract_id))
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .set_chip_asset_id(chip_asset_id)
        .call()
        .await
        .unwrap();

    // fund contract with chips
    let call_params = CallParameters::new(1_000_000, chip_asset_id, 1_000_000);
    instance
        .methods()
        .fund()
        .call_params(call_params)
        .unwrap()
        .call()
        .await
        .unwrap();

    // place bet
    let bet_amount = 100;
    let bet = strapped_types::Bet::Chip;
    let roll = Roll::Six;
    let call_params = CallParameters::new(bet_amount, chip_asset_id, 1_000_000);
    alice_instance
        .methods()
        .place_bet(roll.clone(), bet.clone(), bet_amount)
        .call_params(call_params)
        .unwrap()
        .call()
        .await
        .unwrap();

    let bet_game_id = alice_instance
        .methods()
        .current_game_id()
        .call()
        .await
        .unwrap()
        .value;

    // roll the correct number
    let first_number = 10; // 10 % 36 = 10 which is Six
    vrf_instance
        .methods()
        .set_number(first_number)
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .roll_dice()
        .with_contracts(&[&vrf_instance])
        .call()
        .await
        .unwrap();

    // roll seven
    let seven_vrf_number = 19; // 22 % 36 = 22 which is Seven
    vrf_instance
        .methods()
        .set_number(seven_vrf_number)
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .roll_dice()
        .with_contracts(&[&vrf_instance])
        .call()
        .await
        .unwrap();

    // when

    // claim reward
    let wallet_balance = ctx.alice().get_asset_balance(&chip_asset_id).await.unwrap();
    alice_instance
        .methods()
        .claim_rewards(bet_game_id)
        .with_variable_output_policy(VariableOutputPolicy::Exactly(1))
        .call()
        .await
        .unwrap();

    // then

    // Six pays 2:1 for each roll
    let expected = wallet_balance + bet_amount * 2 - bet_amount;
    let actual = ctx.alice().get_asset_balance(&chip_asset_id).await.unwrap();
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn claim_rewards__cannot_claim_rewards_for_current_game() {
    let ctx = TestContext::new().await;
    let owner = ctx.owner();
    let chip_asset_id = AssetId::new([1; 32]);

    // given
    // init contracts
    let (instance, contract_id) = get_contract_instance(owner.clone()).await;
    let alice_instance = separate_contract_instance(&contract_id, ctx.alice()).await;
    let (vrf_instance, vrf_contract_id) = get_vrf_contract_instance(owner).await;
    instance
        .methods()
        .set_vrf_contract_id(Bits256(*vrf_contract_id))
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .set_chip_asset_id(chip_asset_id)
        .call()
        .await
        .unwrap();

    // place bet
    let bet_amount = 100;
    let bet = strapped_types::Bet::Chip;
    let roll = Roll::Six;
    let call_params = CallParameters::new(bet_amount, chip_asset_id, 1_000_000);
    alice_instance
        .methods()
        .place_bet(roll.clone(), bet.clone(), bet_amount)
        .call_params(call_params)
        .unwrap()
        .call()
        .await
        .unwrap();
    vrf_instance
        .methods()
        .set_number(10) // 10 % 36 = 10 which is Six
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .roll_dice()
        .with_contracts(&[&vrf_instance])
        .call()
        .await
        .unwrap();

    let bet_game_id = alice_instance
        .methods()
        .current_game_id()
        .call()
        .await
        .unwrap()
        .value;

    // when
    let result = instance.methods().claim_rewards(bet_game_id).call().await;

    // then
    assert!(result.is_err());
}

#[tokio::test]
async fn claim_rewards__do_not_reward_bets_placed_after_roll() {
    let ctx = TestContext::new().await;
    let owner = ctx.owner();
    let chip_asset_id = AssetId::new([1; 32]);

    // given
    // init contracts
    let (instance, contract_id) = get_contract_instance(owner.clone()).await;
    let alice_instance = separate_contract_instance(&contract_id, ctx.alice()).await;
    let (vrf_instance, vrf_contract_id) = get_vrf_contract_instance(owner).await;
    instance
        .methods()
        .set_vrf_contract_id(Bits256(*vrf_contract_id))
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .set_chip_asset_id(chip_asset_id)
        .call()
        .await
        .unwrap();

    // fund contract with chips
    let call_params = CallParameters::new(1_000_000, chip_asset_id, 1_000_000);
    instance
        .methods()
        .fund()
        .call_params(call_params)
        .unwrap()
        .call()
        .await
        .unwrap();

    // roll the correct number
    let first_number = 10; // 10 % 36 = 10 which is Six
    vrf_instance
        .methods()
        .set_number(first_number)
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .roll_dice()
        .with_contracts(&[&vrf_instance])
        .call()
        .await
        .unwrap();

    // place bet
    let bet_amount = 100;
    let bet = strapped_types::Bet::Chip;
    let roll = Roll::Six;
    let call_params = CallParameters::new(bet_amount, chip_asset_id, 1_000_000);
    alice_instance
        .methods()
        .place_bet(roll.clone(), bet.clone(), bet_amount)
        .call_params(call_params)
        .unwrap()
        .call()
        .await
        .unwrap();

    let bet_game_id = alice_instance
        .methods()
        .current_game_id()
        .call()
        .await
        .unwrap()
        .value;

    // roll seven
    let seven_vrf_number = 19; // 22 % 36 = 22 which is Seven
    vrf_instance
        .methods()
        .set_number(seven_vrf_number)
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .roll_dice()
        .with_contracts(&[&vrf_instance])
        .call()
        .await
        .unwrap();

    // when

    // claim reward
    let wallet_balance = ctx.alice().get_asset_balance(&chip_asset_id).await.unwrap();
    alice_instance
        .methods()
        .claim_rewards(bet_game_id)
        .with_variable_output_policy(VariableOutputPolicy::Exactly(1))
        .call()
        .await
        .unwrap_err();

    // then
    let expected = wallet_balance;
    let actual = ctx.alice().get_asset_balance(&chip_asset_id).await.unwrap();
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn claim_rewards__cannot_claim_rewards_twice() {
    let ctx = TestContext::new().await;
    let owner = ctx.owner();
    let chip_asset_id = AssetId::new([1; 32]);

    // given
    // init contracts
    let (instance, contract_id) = get_contract_instance(owner.clone()).await;
    let alice_instance = separate_contract_instance(&contract_id, ctx.alice()).await;
    let (vrf_instance, vrf_contract_id) = get_vrf_contract_instance(owner).await;
    instance
        .methods()
        .set_vrf_contract_id(Bits256(*vrf_contract_id))
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .set_chip_asset_id(chip_asset_id)
        .call()
        .await
        .unwrap();

    // fund contract with chips
    let call_params = CallParameters::new(1_000_000, chip_asset_id, 1_000_000);
    instance
        .methods()
        .fund()
        .call_params(call_params)
        .unwrap()
        .call()
        .await
        .unwrap();

    // place bet
    let bet_amount = 100;
    let bet = strapped_types::Bet::Chip;
    let roll = Roll::Six;
    let call_params = CallParameters::new(bet_amount, chip_asset_id, 1_000_000);
    alice_instance
        .methods()
        .place_bet(roll.clone(), bet.clone(), bet_amount)
        .call_params(call_params)
        .unwrap()
        .call()
        .await
        .unwrap();

    let bet_game_id = alice_instance
        .methods()
        .current_game_id()
        .call()
        .await
        .unwrap()
        .value;

    // roll the correct number
    let first_number = 10; // 10 % 36 = 10 which is Six
    vrf_instance
        .methods()
        .set_number(first_number)
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .roll_dice()
        .with_contracts(&[&vrf_instance])
        .call()
        .await
        .unwrap();

    // roll seven
    let seven_vrf_number = 19; // 22 % 36 = 22 which is Seven
    vrf_instance
        .methods()
        .set_number(seven_vrf_number)
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .roll_dice()
        .with_contracts(&[&vrf_instance])
        .call()
        .await
        .unwrap();

    // claim reward
    alice_instance
        .methods()
        .claim_rewards(bet_game_id)
        .with_variable_output_policy(VariableOutputPolicy::Exactly(1))
        .call()
        .await
        .unwrap();

    // when
    let wallet_balance = ctx.alice().get_asset_balance(&chip_asset_id).await.unwrap();
    // try claiming again
    alice_instance
        .methods()
        .claim_rewards(bet_game_id)
        .with_variable_output_policy(VariableOutputPolicy::Exactly(1))
        .call()
        .await
        .unwrap_err();

    let expected = wallet_balance;
    let actual = ctx.alice().get_asset_balance(&chip_asset_id).await.unwrap();
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn claim_rewards__can_receive_strap_token() {
    let ctx = TestContext::new().await;
    let owner = ctx.owner();
    let chip_asset_id = AssetId::new([1; 32]);

    // given
    // init contracts
    let (instance, contract_id) = get_contract_instance(owner.clone()).await;
    let alice_instance = separate_contract_instance(&contract_id, ctx.alice()).await;
    let (vrf_instance, vrf_contract_id) = get_vrf_contract_instance(owner).await;
    instance
        .methods()
        .set_vrf_contract_id(Bits256(*vrf_contract_id))
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .set_chip_asset_id(chip_asset_id)
        .call()
        .await
        .unwrap();

    // fund contract with chips
    let call_params = CallParameters::new(1_000_000, chip_asset_id, 1_000_000);
    instance
        .methods()
        .fund()
        .call_params(call_params)
        .unwrap()
        .call()
        .await
        .unwrap();

    // roll seven
    let seven_vrf_number = 19; // 22 % 36 = 22 which is Seven
    vrf_instance
        .methods()
        .set_number(seven_vrf_number)
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .roll_dice()
        .with_contracts(&[&vrf_instance])
        .call()
        .await
        .unwrap();

    // place bet
    let bet_amount = 100;
    let bet = Bet::Chip;
    let roll = Roll::Eight;
    let call_params = CallParameters::new(bet_amount, chip_asset_id, 1_000_000);
    alice_instance
        .methods()
        .place_bet(roll.clone(), bet.clone(), bet_amount)
        .call_params(call_params)
        .unwrap()
        .call()
        .await
        .unwrap();

    let bet_game_id = alice_instance
        .methods()
        .current_game_id()
        .call()
        .await
        .unwrap()
        .value;

    // roll the correct number
    let first_number = 25; // 25 % 36 = 25 which is Eight
    vrf_instance
        .methods()
        .set_number(first_number)
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .roll_dice()
        .with_contracts(&[&vrf_instance])
        .call()
        .await
        .unwrap();

    // roll seven
    let seven_vrf_number = 19; // 22 % 36 = 22 which is Seven
    vrf_instance
        .methods()
        .set_number(seven_vrf_number)
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .roll_dice()
        .with_contracts(&[&vrf_instance])
        .call()
        .await
        .unwrap();

    // when
    let strap = Strap::new(1, StrapKind::Shirt, Modifier::Nothing);
    let sub_asset_id = strap_to_sub_id(&strap);
    let expected_asset_id = contract_id.asset_id(&sub_asset_id);
    let wallet_balance = ctx
        .alice()
        .get_asset_balance(&expected_asset_id)
        .await
        .unwrap();
    alice_instance
        .methods()
        .claim_rewards(bet_game_id)
        .with_variable_output_policy(VariableOutputPolicy::Exactly(2))
        .call()
        .await
        .unwrap();

    // then
    let expected = wallet_balance + 1;
    let actual = ctx
        .alice()
        .get_asset_balance(&expected_asset_id)
        .await
        .unwrap();
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn claim_rewards__will_only_receive_one_strap_reward_per_roll() {
    let ctx = TestContext::new().await;
    let owner = ctx.owner();
    let chip_asset_id = AssetId::new([1; 32]);

    // given
    // init contracts
    let (instance, contract_id) = get_contract_instance(owner.clone()).await;
    let alice_instance = separate_contract_instance(&contract_id, ctx.alice()).await;
    let (vrf_instance, vrf_contract_id) = get_vrf_contract_instance(owner).await;
    instance
        .methods()
        .set_vrf_contract_id(Bits256(*vrf_contract_id))
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .set_chip_asset_id(chip_asset_id)
        .call()
        .await
        .unwrap();

    // fund contract with chips
    let call_params = CallParameters::new(1_000_000, chip_asset_id, 1_000_000);
    instance
        .methods()
        .fund()
        .call_params(call_params)
        .unwrap()
        .call()
        .await
        .unwrap();

    // roll seven
    let seven_vrf_number = 19; // 22 % 36 = 22 which is Seven
    vrf_instance
        .methods()
        .set_number(seven_vrf_number)
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .roll_dice()
        .with_contracts(&[&vrf_instance])
        .call()
        .await
        .unwrap();

    // place bet
    let bet_amount = 100;
    let bet = Bet::Chip;
    let roll = Roll::Eight;
    let call_params = CallParameters::new(bet_amount, chip_asset_id, 1_000_000);
    alice_instance
        .methods()
        .place_bet(roll.clone(), bet.clone(), bet_amount)
        .call_params(call_params.clone())
        .unwrap()
        .call()
        .await
        .unwrap();
    alice_instance
        .methods()
        .place_bet(roll.clone(), bet.clone(), bet_amount)
        .call_params(call_params)
        .unwrap()
        .call()
        .await
        .unwrap();

    let bet_game_id = alice_instance
        .methods()
        .current_game_id()
        .call()
        .await
        .unwrap()
        .value;

    // roll the correct number
    let first_number = 25; // 25 % 36 = 25 which is Eight
    vrf_instance
        .methods()
        .set_number(first_number)
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .roll_dice()
        .with_contracts(&[&vrf_instance])
        .call()
        .await
        .unwrap();

    // roll seven
    let seven_vrf_number = 19; // 22 % 36 = 22 which is Seven
    vrf_instance
        .methods()
        .set_number(seven_vrf_number)
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .roll_dice()
        .with_contracts(&[&vrf_instance])
        .call()
        .await
        .unwrap();

    // when
    let strap = Strap::new(1, StrapKind::Shirt, Modifier::Nothing);
    let sub_asset_id = strap_to_sub_id(&strap);
    let expected_asset_id = contract_id.asset_id(&sub_asset_id);
    let wallet_balance = ctx
        .alice()
        .get_asset_balance(&expected_asset_id)
        .await
        .unwrap();
    alice_instance
        .methods()
        .claim_rewards(bet_game_id)
        .with_variable_output_policy(VariableOutputPolicy::Exactly(2))
        .call()
        .await
        .unwrap();

    // then
    let expected = wallet_balance + 1;
    let actual = ctx
        .alice()
        .get_asset_balance(&expected_asset_id)
        .await
        .unwrap();
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn claim_rewards__bet_straps_are_levelled_up() {
    let chip_asset_id = AssetId::new([1; 32]);

    // given
    let contract_id = contract_id();
    let strap = Strap::new(1, StrapKind::Shirt, Modifier::Nothing);
    let strap_sub_id = strap_to_sub_id(&strap);
    let bet = Bet::Strap(strap);
    let strap_asset_id = contract_id.asset_id(&strap_sub_id);
    let ctx = TestContext::new_with_extra_assets(vec![AssetConfig {
        id: strap_asset_id.clone(),
        num_coins: 1,
        coin_amount: 1,
    }])
    .await;
    let owner = ctx.owner();
    let (instance, contract_id) = get_contract_instance(owner.clone()).await;
    let alice_instance = separate_contract_instance(&contract_id, ctx.alice()).await;
    let (vrf_instance, vrf_contract_id) = get_vrf_contract_instance(owner).await;
    instance
        .methods()
        .set_vrf_contract_id(Bits256(*vrf_contract_id))
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .set_chip_asset_id(chip_asset_id)
        .call()
        .await
        .unwrap();

    // fund contract with chips
    let call_params = CallParameters::new(1_000_000, chip_asset_id, 1_000_000);
    instance
        .methods()
        .fund()
        .call_params(call_params)
        .unwrap()
        .call()
        .await
        .unwrap();

    // place bet
    let bet_amount = 1;
    let roll = Roll::Six;
    let call_params = CallParameters::new(bet_amount, strap_asset_id, 1_000_000);
    alice_instance
        .methods()
        .place_bet(roll.clone(), bet.clone(), bet_amount)
        .call_params(call_params)
        .unwrap()
        .call()
        .await
        .unwrap();

    let bet_game_id = alice_instance
        .methods()
        .current_game_id()
        .call()
        .await
        .unwrap()
        .value;

    // roll the correct number
    let first_number = 10; // 10 % 36 = 10 which is Six
    vrf_instance
        .methods()
        .set_number(first_number)
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .roll_dice()
        .with_contracts(&[&vrf_instance])
        .call()
        .await
        .unwrap();

    // roll seven
    let seven_vrf_number = 19; // 22 % 36 = 22 which is Seven
    vrf_instance
        .methods()
        .set_number(seven_vrf_number)
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .roll_dice()
        .with_contracts(&[&vrf_instance])
        .call()
        .await
        .unwrap();

    // when

    // claim reward
    let wallet_balance = ctx.alice().get_asset_balance(&chip_asset_id).await.unwrap();
    alice_instance
        .methods()
        .claim_rewards(bet_game_id)
        .with_variable_output_policy(VariableOutputPolicy::Exactly(1))
        .call()
        .await
        .unwrap();

    // then
    let lvl_2_strap = Strap::new(2, StrapKind::Shirt, Modifier::Nothing);
    let new_strap_sub_id = strap_to_sub_id(&lvl_2_strap);
    let new_strap_asset_id = contract_id.asset_id(&new_strap_sub_id);
    let expected = 1;
    let actual = ctx
        .alice()
        .get_asset_balance(&new_strap_asset_id)
        .await
        .unwrap();
    assert_eq!(expected, actual);
}
