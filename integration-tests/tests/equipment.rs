#![allow(non_snake_case)]

use fuels::prelude::Execution;
use generated_abi::{
    strapped_types::{
        Modifier,
        Strap,
        StrapKind,
    },
    test_helpers::TestContext,
};

#[tokio::test]
async fn set_shirt__stores_shirt() {
    let ctx = TestContext::new().await;
    let shirt = Strap::new(2, StrapKind::Shirt, Modifier::Lucky);

    // given

    // when
    ctx.alice_instance()
        .methods()
        .set_shirt(shirt.clone())
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
    let result = ctx
        .alice_instance()
        .methods()
        .set_shirt(pants)
        .call()
        .await;

    // then
    assert!(result.is_err());
}

#[tokio::test]
async fn add_accessory__appends_accessory() {
    let ctx = TestContext::new().await;
    let hat = Strap::new(1, StrapKind::Hat, Modifier::Nothing);

    // given

    // when
    ctx.alice_instance()
        .methods()
        .add_accessory(hat.clone())
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
async fn add_accessory__fails_when_over_limit() {
    let ctx = TestContext::new().await;
    let hat = Strap::new(1, StrapKind::Hat, Modifier::Nothing);
    let ring = Strap::new(1, StrapKind::Ring, Modifier::Nothing);
    let glasses = Strap::new(1, StrapKind::Glasses, Modifier::Nothing);
    let belt = Strap::new(1, StrapKind::Belt, Modifier::Nothing);

    // given
    ctx.alice_instance()
        .methods()
        .add_accessory(hat)
        .call()
        .await
        .unwrap();
    ctx.alice_instance()
        .methods()
        .add_accessory(ring)
        .call()
        .await
        .unwrap();
    ctx.alice_instance()
        .methods()
        .add_accessory(glasses)
        .call()
        .await
        .unwrap();

    // when
    let result = ctx
        .alice_instance()
        .methods()
        .add_accessory(belt)
        .call()
        .await;

    // then
    assert!(result.is_err());
}

#[tokio::test]
async fn remove_accessory__removes_by_index() {
    let ctx = TestContext::new().await;
    let hat = Strap::new(1, StrapKind::Hat, Modifier::Nothing);
    let ring = Strap::new(1, StrapKind::Ring, Modifier::Nothing);

    // given
    ctx.alice_instance()
        .methods()
        .add_accessory(hat)
        .call()
        .await
        .unwrap();
    ctx.alice_instance()
        .methods()
        .add_accessory(ring.clone())
        .call()
        .await
        .unwrap();

    // when
    ctx.alice_instance()
        .methods()
        .remove_accessory(0)
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
