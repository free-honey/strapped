#![allow(non_snake_case)]

use fuels::{
    accounts::ViewOnlyAccount,
    prelude::{AssetConfig, AssetId, CallParameters, VariableOutputPolicy},
    tx::ContractIdExt,
    types::Bits256,
};
use strapped_contract::{
    contract_id, get_contract_instance, separate_contract_instance, strap_to_sub_id,
    strapped_types::{Bet, Modifier, Roll, Strap, StrapKind},
    test_helpers::*,
};

#[tokio::test]
async fn roll_dice__adds_roll_to_roll_history() {
    let ctx = TestContext::new().await;
    let owner = ctx.owner();
    // given
    let (instance, _id) = get_contract_instance(owner.clone()).await;
    let (vrf_instance, vrf_id) = get_vrf_contract_instance(owner).await;
    let first_number = 10;
    let second_number = 34;
    instance
        .methods()
        .set_vrf_contract_id(Bits256(*vrf_id))
        .call()
        .await
        .unwrap();
    vrf_instance
        .methods()
        .set_number(first_number)
        .call()
        .await
        .unwrap();

    // when
    instance
        .methods()
        .roll_dice()
        .with_contracts(&[&vrf_instance])
        .call()
        .await
        .unwrap();
    // update vrf
    vrf_instance
        .methods()
        .set_number(second_number)
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

    // then
    let actual = instance
        .methods()
        .roll_history()
        .call()
        .await
        .unwrap()
        .value;
    // 0-35
    // where 35 is Twelve and 0 is Two
    // 10 % 36
    // so 10 is Six
    // 34 % 36
    // so 34 is Eleven
    let expected = vec![Roll::Six, Roll::Eleven];
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn roll_dice__if_seven_rolled_move_to_next_game() {
    let ctx = TestContext::new().await;
    let owner = ctx.owner();
    // given
    let (instance, _id) = get_contract_instance(owner.clone()).await;
    let (vrf_instance, vrf_id) = get_vrf_contract_instance(owner).await;
    let first_number = 10;
    let second_number = 34;
    instance
        .methods()
        .set_vrf_contract_id(Bits256(*vrf_id))
        .call()
        .await
        .unwrap();
    // update vrf
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
    // update vrf
    vrf_instance
        .methods()
        .set_number(second_number)
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
    // update vrf to something that will resolve to Seven
    let third_number = 19; // 22 % 36 = 22 which is Seven
    vrf_instance
        .methods()
        .set_number(third_number)
        .call()
        .await
        .unwrap();

    // when
    instance
        .methods()
        .roll_dice()
        .with_contracts(&[&vrf_instance])
        .call()
        .await
        .unwrap();

    // then
    let actual = instance
        .methods()
        .roll_history()
        .call()
        .await
        .unwrap()
        .value;
    let expected: Vec<Roll> = Vec::new();
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn roll_dice__if_seven_adds_new_strap_reward() {
    let ctx = TestContext::new().await;
    let owner = ctx.owner();
    // given
    let (instance, _id) = get_contract_instance(owner.clone()).await;
    let (vrf_instance, vrf_id) = get_vrf_contract_instance(owner).await;
    instance
        .methods()
        .set_vrf_contract_id(Bits256(*vrf_id))
        .call()
        .await
        .unwrap();
    // update vrf to something that will resolve to Seven
    let seven_vrf_number = 19; // 22 % 36 = 22 which is Seven
    vrf_instance
        .methods()
        .set_number(seven_vrf_number)
        .call()
        .await
        .unwrap();

    // when
    instance
        .methods()
        .roll_dice()
        .with_contracts(&[&vrf_instance])
        .call()
        .await
        .unwrap();

    // then
    let actual = instance
        .methods()
        .strap_rewards()
        .call()
        .await
        .unwrap()
        .value;
    let expected_roll = Roll::Eight;
    let expected_strap = Strap {
        level: 1,
        kind: StrapKind::Shirt,
        modifier: Modifier::Nothing,
    };
    let expected = vec![(expected_roll, expected_strap)];
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn roll_dice__if_seven_generates_new_modifier_triggers() {
    let ctx = TestContext::new().await;
    let owner = ctx.owner();
    // given
    let (instance, _id) = get_contract_instance(owner.clone()).await;
    let (vrf_instance, vrf_id) = get_vrf_contract_instance(owner).await;
    instance
        .methods()
        .set_vrf_contract_id(Bits256(*vrf_id))
        .call()
        .await
        .unwrap();
    // update vrf to something that will resolve to Seven
    let seven_vrf_number = 19; // 22 % 36 = 22 which is Seven
    vrf_instance
        .methods()
        .set_number(seven_vrf_number)
        .call()
        .await
        .unwrap();

    // when
    instance
        .methods()
        .roll_dice()
        .with_contracts(&[&vrf_instance])
        .call()
        .await
        .unwrap();

    // then
    let actual = instance
        .methods()
        .modifier_triggers()
        .call()
        .await
        .unwrap()
        .value;
    let expected = vec![
        (Roll::Two, Roll::Six, Modifier::Burnt, false),
        (Roll::Twelve, Roll::Eight, Modifier::Lucky, false),
    ];
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn roll_dice__if_hit_the_modifier_value_triggers_the_modifier_to_be_purchased() {
    let ctx = TestContext::new().await;
    let owner = ctx.owner();
    // given
    let (instance, _id) = get_contract_instance(owner.clone()).await;
    let (vrf_instance, vrf_id) = get_vrf_contract_instance(owner).await;
    instance
        .methods()
        .set_vrf_contract_id(Bits256(*vrf_id))
        .call()
        .await
        .unwrap();
    // update vrf to something that will resolve to Seven
    let seven_vrf_number = 19; // 22 % 36 = 22 which is Seven
    vrf_instance
        .methods()
        .set_number(seven_vrf_number)
        .call()
        .await
        .unwrap();

    // when
    instance
        .methods()
        .roll_dice()
        .with_contracts(&[&vrf_instance])
        .call()
        .await
        .unwrap();
    let two_six_vrf_number = 0; // 0 % 36 = 0 which is Two
    vrf_instance
        .methods()
        .set_number(two_six_vrf_number)
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

    // then
    let actual = instance
        .methods()
        .modifier_triggers()
        .call()
        .await
        .unwrap()
        .value;
    let expected = vec![
        (Roll::Two, Roll::Six, Modifier::Burnt, true),
        (Roll::Twelve, Roll::Eight, Modifier::Lucky, false),
    ];
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn roll_dice__resets_active_modifiers_and_triggers() {
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
    let alice = ctx.alice();
    // given
    let (instance, contract_id) = get_contract_instance(owner.clone()).await;
    let alice_instance = separate_contract_instance(&contract_id, alice).await;
    let (vrf_instance, vrf_id) = get_vrf_contract_instance(owner).await;
    instance
        .methods()
        .set_vrf_contract_id(Bits256(*vrf_id))
        .call()
        .await
        .unwrap();
    let chip_asset_id = AssetId::new([1u8; 32]);
    instance
        .methods()
        .set_chip_asset_id(chip_asset_id)
        .call()
        .await
        .unwrap();
    // update vrf to something that will resolve to Seven
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

    // trigger modifier
    let two_vrf_number = 0; // 0 % 36 = 0 which is Two
    vrf_instance
        .methods()
        .set_number(two_vrf_number)
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

    // purchase triggered modifier
    let cost = 1;
    let roll = Roll::Six;
    let call_params = CallParameters::new(cost, chip_asset_id, 1_000_000);
    alice_instance
        .methods()
        .purchase_modifier(roll.clone(), Modifier::Burnt)
        .call_params(call_params)
        .unwrap()
        .call()
        .await
        .unwrap();

    // roll seven to end game
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
    let triggers = instance
        .methods()
        .modifier_triggers()
        .call()
        .await
        .unwrap()
        .value;
    let active_modifiers = alice_instance
        .methods()
        .active_modifiers()
        .call()
        .await
        .unwrap()
        .value;

    // then
    let expected_triggers: Vec<(Roll, Roll, Modifier, bool)> = vec![
        (Roll::Two, Roll::Six, Modifier::Burnt, false),
        (Roll::Twelve, Roll::Eight, Modifier::Lucky, false),
    ];
    assert_eq!(expected_triggers, triggers);
    let expected_active_modifiers: Vec<(Roll, Modifier, u64)> = Vec::new();
    assert_eq!(expected_active_modifiers, active_modifiers);
}
