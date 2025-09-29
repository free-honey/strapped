#![allow(non_snake_case)]

use fuels::prelude::CallParameters;
use strapped_contract::{
    strapped_types::{Modifier, Roll},
    test_helpers::TestContext,
};

#[tokio::test]
async fn purchase_modifier__activates_modifier_for_current_game() {
    let ctx = TestContext::new().await;
    let chip_asset_id = ctx.chip_asset_id();

    ctx.advance_and_roll(19).await; // Seven -> seed modifiers
    ctx.advance_and_roll(0).await; // Two -> trigger Burnt modifier

    ctx.alice_instance()
        .methods()
        .purchase_modifier(Roll::Six, Modifier::Burnt)
        .call_params(CallParameters::new(1, chip_asset_id, 1_000_000))
        .unwrap()
        .call()
        .await
        .unwrap();

    let actual_active_modifier = ctx
        .owner_instance()
        .methods()
        .active_modifiers()
        .call()
        .await
        .unwrap()
        .value;
    let expected_active_modifier = vec![(Roll::Six, Modifier::Burnt, 1u64)];
    assert_eq!(expected_active_modifier, actual_active_modifier);
}
