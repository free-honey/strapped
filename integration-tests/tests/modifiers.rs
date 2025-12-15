#![allow(non_snake_case)]

use fuels::prelude::{
    CallParameters,
    Execution,
};
use generated_abi::{
    strapped_types::abigen_bindings::my_contract_mod::contract_types::{
        Modifier,
        Roll,
    },
    test_helpers::{
        TestContext,
        modifier_floor_price,
        modifier_triggers_for_roll,
        roll_to_vrf_number,
    },
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
    let floor_price = modifier_floor_price(&modifier);
    let vrf_number = roll_to_vrf_number(&trigger_roll);
    ctx.advance_and_roll(vrf_number).await; // Two -> trigger Burnt modifier

    // when
    ctx.alice_instance()
        .methods()
        .purchase_modifier(modifier_roll.clone(), modifier.clone())
        .call_params(CallParameters::new(floor_price, chip_asset_id, 1_000_000))
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

#[tokio::test]
async fn purchase_modifier__price_doubles_next_time_when_bought() {
    let ctx = TestContext::new().await;
    let chip_asset_id = ctx.chip_asset_id();

    // given
    ctx.advance_and_roll(SEVEN_VRF_NUMBER).await; // Seven -> seed modifiers
    let (trigger_roll, modifier_roll, modifier, floor_price) = first_modifier().await;
    ctx.advance_and_roll(roll_to_vrf_number(&trigger_roll))
        .await; // trigger modifier

    ctx.alice_instance()
        .methods()
        .purchase_modifier(modifier_roll.clone(), modifier.clone())
        .call_params(CallParameters::new(floor_price, chip_asset_id, 1_000_000))
        .unwrap()
        .call()
        .await
        .unwrap();

    ctx.advance_and_roll(SEVEN_VRF_NUMBER).await; // Seven -> new game, price should double
    ctx.advance_and_roll(roll_to_vrf_number(&trigger_roll))
        .await; // trigger again

    // when
    let cheap_attempt = ctx
        .alice_instance()
        .methods()
        .purchase_modifier(modifier_roll.clone(), modifier.clone())
        .call_params(CallParameters::new(floor_price, chip_asset_id, 1_000_000))
        .unwrap()
        .call()
        .await;

    // then
    assert!(
        cheap_attempt.is_err(),
        "expected price to increase after purchase"
    );
    let double_price = floor_price * 2;
    ctx.alice_instance()
        .methods()
        .purchase_modifier(modifier_roll, modifier)
        .call_params(CallParameters::new(double_price, chip_asset_id, 1_000_000))
        .unwrap()
        .call()
        .await
        .unwrap();
}

#[tokio::test]
async fn purchase_modifier__price_halves_next_time_when_triggered_but_not_bought_after_increase()
 {
    let ctx = TestContext::new().await;
    let chip_asset_id = ctx.chip_asset_id();

    // given
    ctx.advance_and_roll(SEVEN_VRF_NUMBER).await; // Seven -> seed modifiers
    let (trigger_roll, modifier_roll, modifier, floor_price) = first_modifier().await;
    ctx.advance_and_roll(roll_to_vrf_number(&trigger_roll))
        .await; // trigger modifier

    ctx.alice_instance()
        .methods()
        .purchase_modifier(modifier_roll.clone(), modifier.clone())
        .call_params(CallParameters::new(floor_price, chip_asset_id, 1_000_000))
        .unwrap()
        .call()
        .await
        .unwrap();

    ctx.advance_and_roll(SEVEN_VRF_NUMBER).await; // Seven -> new game, price should double
    ctx.advance_and_roll(roll_to_vrf_number(&trigger_roll))
        .await; // trigger but do not purchase
    ctx.advance_and_roll(SEVEN_VRF_NUMBER).await; // Seven -> new game, price should halve
    ctx.advance_and_roll(roll_to_vrf_number(&trigger_roll))
        .await; // trigger again

    // when
    ctx.alice_instance()
        .methods()
        .purchase_modifier(modifier_roll.clone(), modifier.clone())
        .call_params(CallParameters::new(floor_price, chip_asset_id, 1_000_000))
        .unwrap()
        .call()
        .await
        .unwrap();

    // then
    // purchase succeeded at floor price, implying price halved
}

#[tokio::test]
async fn purchase_modifier__price_stays_high_if_not_triggered_between_games() {
    let ctx = TestContext::new().await;
    let chip_asset_id = ctx.chip_asset_id();

    // given
    ctx.advance_and_roll(SEVEN_VRF_NUMBER).await; // Seven -> seed modifiers
    let (trigger_roll, modifier_roll, modifier, floor_price) = first_modifier().await;
    ctx.advance_and_roll(roll_to_vrf_number(&trigger_roll))
        .await; // trigger modifier

    ctx.alice_instance()
        .methods()
        .purchase_modifier(modifier_roll.clone(), modifier.clone())
        .call_params(CallParameters::new(floor_price, chip_asset_id, 1_000_000))
        .unwrap()
        .call()
        .await
        .unwrap();

    ctx.advance_and_roll(SEVEN_VRF_NUMBER).await; // Seven -> new game, price should double
    // Skip triggering/purchasing in this game
    ctx.advance_and_roll(SEVEN_VRF_NUMBER).await; // Seven -> next game, price should stay doubled
    ctx.advance_and_roll(roll_to_vrf_number(&trigger_roll))
        .await; // trigger now

    // when
    let cheap_attempt = ctx
        .alice_instance()
        .methods()
        .purchase_modifier(modifier_roll.clone(), modifier.clone())
        .call_params(CallParameters::new(floor_price, chip_asset_id, 1_000_000))
        .unwrap()
        .call()
        .await;

    // then
    assert!(
        cheap_attempt.is_err(),
        "expected price to remain doubled when not triggered between games"
    );
    let double_price = floor_price * 2;
    ctx.alice_instance()
        .methods()
        .purchase_modifier(modifier_roll, modifier)
        .call_params(CallParameters::new(double_price, chip_asset_id, 1_000_000))
        .unwrap()
        .call()
        .await
        .unwrap();
}

async fn first_modifier() -> (Roll, Roll, Modifier, u64) {
    let (trigger_roll, modifier_roll, modifier) =
        modifier_triggers_for_roll(SEVEN_VRF_NUMBER)
            .first()
            .unwrap()
            .clone();
    let floor_price = modifier_floor_price(&modifier);
    (trigger_roll, modifier_roll, modifier, floor_price)
}
