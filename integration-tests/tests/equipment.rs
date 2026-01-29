#![allow(non_snake_case)]

use fuels::{
    accounts::ViewOnlyAccount,
    prelude::{
        AssetConfig,
        CallParameters,
        Execution,
        VariableOutputPolicy,
    },
    tx::ContractIdExt,
};
use generated_abi::{
    contract_id,
    strap_to_sub_id,
    strapped_types::{
        Modifier,
        Strap,
        StrapKind,
    },
    test_helpers::TestContext,
};

#[tokio::test]
async fn set_shirt__stores_shirt() {
    let shirt = Strap::new(2, StrapKind::Shirt, Modifier::Lucky);
    let strap_sub_id = strap_to_sub_id(&shirt);
    let strap_asset_id = contract_id().asset_id(&strap_sub_id);
    let ctx = TestContext::builder()
        .with_extra_assets(vec![AssetConfig {
            id: strap_asset_id,
            num_coins: 1,
            coin_amount: 1,
        }])
        .build()
        .await;

    // given

    // when
    ctx.alice_instance()
        .methods()
        .set_shirt(shirt.clone())
        .with_variable_output_policy(VariableOutputPolicy::EstimateMinimum)
        .call_params(CallParameters::new(1, strap_asset_id, 1_000_000))
        .unwrap()
        .call()
        .await
        .unwrap();

    // then
    let equipment = ctx
        .alice_instance()
        .methods()
        .get_my_equipment()
        .simulate(Execution::realistic())
        .await
        .unwrap()
        .value;
    assert_eq!(equipment.shirt, Some(shirt));
}

#[tokio::test]
async fn set_shirt__rejects_wrong_kind() {
    let ctx = TestContext::new().await;
    let pants = Strap::new(1, StrapKind::Pants, Modifier::Nothing);

    // given

    // when
    let result = ctx.alice_instance().methods().set_shirt(pants).call().await;

    // then
    assert!(result.is_err());
}

#[tokio::test]
async fn set_shirt__fails_without_strap_asset() {
    let ctx = TestContext::new().await;
    let shirt = Strap::new(1, StrapKind::Shirt, Modifier::Nothing);

    // given

    // when
    let result = ctx.alice_instance().methods().set_shirt(shirt).call().await;

    // then
    assert!(result.is_err());
}

#[tokio::test]
async fn add_accessory__appends_accessory() {
    let hat = Strap::new(1, StrapKind::Hat, Modifier::Nothing);
    let strap_sub_id = strap_to_sub_id(&hat);
    let strap_asset_id = contract_id().asset_id(&strap_sub_id);
    let ctx = TestContext::builder()
        .with_extra_assets(vec![AssetConfig {
            id: strap_asset_id,
            num_coins: 1,
            coin_amount: 1,
        }])
        .build()
        .await;

    // given

    // when
    ctx.alice_instance()
        .methods()
        .add_accessory(hat.clone())
        .call_params(CallParameters::new(1, strap_asset_id, 1_000_000))
        .unwrap()
        .call()
        .await
        .unwrap();

    // then
    let equipment = ctx
        .alice_instance()
        .methods()
        .get_my_equipment()
        .simulate(Execution::realistic())
        .await
        .unwrap()
        .value;
    assert_eq!(equipment.accessories, vec![hat]);
}

#[tokio::test]
async fn add_accessory__fails_without_strap_asset() {
    let ctx = TestContext::new().await;
    let hat = Strap::new(1, StrapKind::Hat, Modifier::Nothing);

    // given

    // when
    let result = ctx
        .alice_instance()
        .methods()
        .add_accessory(hat)
        .call()
        .await;

    // then
    assert!(result.is_err());
}

#[tokio::test]
async fn add_accessory__fails_when_over_limit() {
    let hat = Strap::new(1, StrapKind::Hat, Modifier::Nothing);
    let ring = Strap::new(1, StrapKind::Ring, Modifier::Nothing);
    let glasses = Strap::new(1, StrapKind::Glasses, Modifier::Nothing);
    let belt = Strap::new(1, StrapKind::Belt, Modifier::Nothing);
    let hat_asset_id = contract_id().asset_id(&strap_to_sub_id(&hat));
    let ring_asset_id = contract_id().asset_id(&strap_to_sub_id(&ring));
    let glasses_asset_id = contract_id().asset_id(&strap_to_sub_id(&glasses));
    let belt_asset_id = contract_id().asset_id(&strap_to_sub_id(&belt));
    let ctx = TestContext::builder()
        .with_extra_assets(vec![
            AssetConfig {
                id: hat_asset_id,
                num_coins: 1,
                coin_amount: 1,
            },
            AssetConfig {
                id: ring_asset_id,
                num_coins: 1,
                coin_amount: 1,
            },
            AssetConfig {
                id: glasses_asset_id,
                num_coins: 1,
                coin_amount: 1,
            },
            AssetConfig {
                id: belt_asset_id,
                num_coins: 1,
                coin_amount: 1,
            },
        ])
        .build()
        .await;

    // given
    ctx.alice_instance()
        .methods()
        .add_accessory(hat)
        .call_params(CallParameters::new(1, hat_asset_id, 1_000_000))
        .unwrap()
        .call()
        .await
        .unwrap();
    ctx.alice_instance()
        .methods()
        .add_accessory(ring)
        .call_params(CallParameters::new(1, ring_asset_id, 1_000_000))
        .unwrap()
        .call()
        .await
        .unwrap();
    ctx.alice_instance()
        .methods()
        .add_accessory(glasses)
        .call_params(CallParameters::new(1, glasses_asset_id, 1_000_000))
        .unwrap()
        .call()
        .await
        .unwrap();

    // when
    let result = ctx
        .alice_instance()
        .methods()
        .add_accessory(belt)
        .call_params(CallParameters::new(1, belt_asset_id, 1_000_000))
        .unwrap()
        .call()
        .await;

    // then
    assert!(result.is_err());
}

#[tokio::test]
async fn remove_accessory__removes_by_index() {
    let hat = Strap::new(1, StrapKind::Hat, Modifier::Nothing);
    let ring = Strap::new(1, StrapKind::Ring, Modifier::Nothing);
    let hat_asset_id = contract_id().asset_id(&strap_to_sub_id(&hat));
    let ring_asset_id = contract_id().asset_id(&strap_to_sub_id(&ring));
    let ctx = TestContext::builder()
        .with_extra_assets(vec![
            AssetConfig {
                id: hat_asset_id,
                num_coins: 1,
                coin_amount: 1,
            },
            AssetConfig {
                id: ring_asset_id,
                num_coins: 1,
                coin_amount: 1,
            },
        ])
        .build()
        .await;

    // given
    ctx.alice_instance()
        .methods()
        .add_accessory(hat)
        .call_params(CallParameters::new(1, hat_asset_id, 1_000_000))
        .unwrap()
        .call()
        .await
        .unwrap();
    ctx.alice_instance()
        .methods()
        .add_accessory(ring.clone())
        .call_params(CallParameters::new(1, ring_asset_id, 1_000_000))
        .unwrap()
        .call()
        .await
        .unwrap();

    // when
    ctx.alice_instance()
        .methods()
        .remove_accessory(0)
        .with_variable_output_policy(VariableOutputPolicy::EstimateMinimum)
        .call()
        .await
        .unwrap();

    // then
    let equipment = ctx
        .alice_instance()
        .methods()
        .get_my_equipment()
        .simulate(Execution::realistic())
        .await
        .unwrap()
        .value;
    assert_eq!(equipment.accessories, vec![ring]);
}

#[tokio::test]
async fn set_shirt__returns_replaced_strap() {
    let first = Strap::new(1, StrapKind::Shirt, Modifier::Nothing);
    let second = Strap::new(1, StrapKind::Shirt, Modifier::Lucky);
    let first_asset_id = contract_id().asset_id(&strap_to_sub_id(&first));
    let second_asset_id = contract_id().asset_id(&strap_to_sub_id(&second));
    let ctx = TestContext::builder()
        .with_extra_assets(vec![
            AssetConfig {
                id: first_asset_id,
                num_coins: 1,
                coin_amount: 1,
            },
            AssetConfig {
                id: second_asset_id,
                num_coins: 1,
                coin_amount: 1,
            },
        ])
        .build()
        .await;

    // given
    let starting_first = ctx
        .alice()
        .get_asset_balance(&first_asset_id)
        .await
        .unwrap();

    // when
    ctx.alice_instance()
        .methods()
        .set_shirt(first.clone())
        .with_variable_output_policy(VariableOutputPolicy::EstimateMinimum)
        .call_params(CallParameters::new(1, first_asset_id, 1_000_000))
        .unwrap()
        .call()
        .await
        .unwrap();
    ctx.alice_instance()
        .methods()
        .set_shirt(second)
        .with_variable_output_policy(VariableOutputPolicy::EstimateMinimum)
        .call_params(CallParameters::new(1, second_asset_id, 1_000_000))
        .unwrap()
        .call()
        .await
        .unwrap();

    // then
    let ending_first = ctx
        .alice()
        .get_asset_balance(&first_asset_id)
        .await
        .unwrap();
    assert_eq!(ending_first, starting_first);
}

#[tokio::test]
async fn clear_shirt__returns_strap() {
    let shirt = Strap::new(1, StrapKind::Shirt, Modifier::Nothing);
    let strap_asset_id = contract_id().asset_id(&strap_to_sub_id(&shirt));
    let ctx = TestContext::builder()
        .with_extra_assets(vec![AssetConfig {
            id: strap_asset_id,
            num_coins: 1,
            coin_amount: 1,
        }])
        .build()
        .await;

    // given
    ctx.alice_instance()
        .methods()
        .set_shirt(shirt.clone())
        .with_variable_output_policy(VariableOutputPolicy::EstimateMinimum)
        .call_params(CallParameters::new(1, strap_asset_id, 1_000_000))
        .unwrap()
        .call()
        .await
        .unwrap();

    // when
    ctx.alice_instance()
        .methods()
        .clear_shirt()
        .with_variable_output_policy(VariableOutputPolicy::EstimateMinimum)
        .call()
        .await
        .unwrap();

    // then
    let balance = ctx
        .alice()
        .get_asset_balance(&strap_asset_id)
        .await
        .unwrap();
    assert_eq!(balance, 1);
}

#[tokio::test]
async fn remove_accessory__returns_strap() {
    let hat = Strap::new(1, StrapKind::Hat, Modifier::Nothing);
    let hat_asset_id = contract_id().asset_id(&strap_to_sub_id(&hat));
    let ctx = TestContext::builder()
        .with_extra_assets(vec![AssetConfig {
            id: hat_asset_id,
            num_coins: 1,
            coin_amount: 1,
        }])
        .build()
        .await;

    // given
    ctx.alice_instance()
        .methods()
        .add_accessory(hat.clone())
        .call_params(CallParameters::new(1, hat_asset_id, 1_000_000))
        .unwrap()
        .call()
        .await
        .unwrap();

    // when
    ctx.alice_instance()
        .methods()
        .remove_accessory(0)
        .with_variable_output_policy(VariableOutputPolicy::EstimateMinimum)
        .call()
        .await
        .unwrap();

    // then
    let balance = ctx.alice().get_asset_balance(&hat_asset_id).await.unwrap();
    assert_eq!(balance, 1);
}
