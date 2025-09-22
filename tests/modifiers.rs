#![allow(non_snake_case)]

use fuels::{
    prelude::CallParameters,
    types::{
        AssetId,
        Bits256,
    },
};
use strapped_contract::{
    get_contract_instance,
    separate_contract_instance,
    strapped_types::{
        Modifier,
        Roll,
    },
    test_helpers::{
        TestContext,
        get_vrf_contract_instance,
    },
};

#[tokio::test]
async fn purchase_modifier__activates_modifier_for_current_game() {
    let ctx = TestContext::new().await;
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

    // when
    let cost = 1;
    let call_params = CallParameters::new(cost, chip_asset_id, 1_000_000);
    alice_instance
        .methods()
        .purchase_modifier(Roll::Six, Modifier::Burnt)
        .call_params(call_params)
        .unwrap()
        .call()
        .await
        .unwrap();

    // then
    let actual_active_modifier = instance
        .methods()
        .active_modifiers()
        .call()
        .await
        .unwrap()
        .value;
    let expected_active_modifier = vec![(Roll::Six, Modifier::Burnt, 1u64)];
    assert_eq!(expected_active_modifier, actual_active_modifier);
}

// TODO: test where try to purchase not existing modifier && untriggered modifier
