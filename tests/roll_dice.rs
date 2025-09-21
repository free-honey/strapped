#![allow(non_snake_case)]
use fuels::types::Bits256;
use strapped_contract::{
    get_contract_instance,
    strapped_types::{
        Modifier,
        Roll,
        Strap,
        StrapKind,
    },
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
    // triggers.push((Roll::Two, Roll::Six, Modifier::Burnt, false));
    // triggers.push((Roll::Twelve, Roll::Eight, Modifier::Lucky, false));
    let expected = vec![
        (Roll::Two, Roll::Six, Modifier::Burnt, false),
        (Roll::Twelve, Roll::Eight, Modifier::Lucky, false),
    ];
    assert_eq!(expected, actual);
}
