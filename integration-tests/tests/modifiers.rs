#![allow(non_snake_case)]

use fuels::prelude::{
    CallParameters,
    Execution,
};
use generated_abi::test_helpers::{
    TestContext,
    modifier_triggers_for_roll,
    roll_to_vrf_number,
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
    let (trigger_roll, modifier_roll, modifier) =
        modifier_triggers_for_roll(SEVEN_VRF_NUMBER)
            .first()
            .unwrap()
            .clone();
    let vrf_number = roll_to_vrf_number(&trigger_roll);
    ctx.advance_and_roll(vrf_number).await; // Two -> trigger Burnt modifier

    // when
    ctx.alice_instance()
        .methods()
        .purchase_modifier(modifier_roll.clone(), modifier.clone())
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
        .simulate(Execution::state_read_only())
        .await
        .unwrap()
        .value;
    let expected_active_modifier = vec![(modifier_roll, modifier, 1u32)];
    assert_eq!(expected_active_modifier, actual_active_modifier);
}
