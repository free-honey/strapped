#![allow(non_snake_case)]

use fuels::prelude::CallParameters;
use strapped_contract::{
    strapped_types::{
        Modifier,
        Roll,
    },
    test_helpers::TestContext,
};

pub const TWO_VRF_NUMBER: u64 = 0;
pub const SIX_VRF_NUMBER: u64 = 10;
pub const SEVEN_VRF_NUMBER: u64 = 19;

#[tokio::test]
async fn purchase_modifier__activates_modifier_for_current_game() {
    let ctx = TestContext::new().await;
    let chip_asset_id = ctx.chip_asset_id();

    // given
    ctx.advance_and_roll(SEVEN_VRF_NUMBER).await; // Seven -> seed modifiers
    ctx.advance_and_roll(TWO_VRF_NUMBER).await; // Two -> trigger Burnt modifier

    // when
    ctx.alice_instance()
        .methods()
        .purchase_modifier(Roll::Six, Modifier::Burnt)
        .call_params(CallParameters::new(1, chip_asset_id, 1_000_000))
        .unwrap()
        .call()
        .await
        .unwrap();

    // then
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
