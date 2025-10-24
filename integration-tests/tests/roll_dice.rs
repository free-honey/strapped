#![allow(non_snake_case)]

use fuels::prelude::{
    CallParameters,
    Execution,
};
use generated_abi::{
    strapped_types::Roll,
    test_helpers::*,
};

pub const TWO_VRF_NUMBER: u64 = 0;
pub const SIX_VRF_NUMBER: u64 = 10;
pub const SEVEN_VRF_NUMBER: u64 = 19;
pub const ELEVEN_VRF_NUMBER: u64 = 34;

#[tokio::test]
async fn roll_dice__adds_roll_to_roll_history() {
    let ctx = TestContext::new().await;

    // given
    ctx.advance_and_roll(SIX_VRF_NUMBER).await; // Six

    // when
    ctx.advance_and_roll(ELEVEN_VRF_NUMBER).await; // Eleven

    // then
    let actual = ctx
        .owner_instance()
        .methods()
        .roll_history()
        .simulate(Execution::state_read_only())
        .await
        .unwrap()
        .value;
    let expected = vec![Roll::Six, Roll::Eleven];
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn roll_dice__if_seven_rolled_move_to_next_game() {
    let ctx = TestContext::new().await;
    // given
    ctx.advance_and_roll(SIX_VRF_NUMBER).await;
    ctx.advance_and_roll(ELEVEN_VRF_NUMBER).await;

    // when
    ctx.advance_and_roll(SEVEN_VRF_NUMBER).await;

    // then
    let actual = ctx
        .owner_instance()
        .methods()
        .roll_history()
        .call()
        .await
        .unwrap()
        .value;
    assert!(actual.is_empty());
}

#[tokio::test]
async fn roll_dice__if_seven_adds_new_strap_reward() {
    let ctx = TestContext::new().await;
    // given

    // when
    ctx.advance_and_roll(SEVEN_VRF_NUMBER).await;

    // then
    let expected = generate_straps(SEVEN_VRF_NUMBER);
    let actual = ctx
        .owner_instance()
        .methods()
        .strap_rewards()
        .call()
        .await
        .unwrap()
        .value;
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn roll_dice__if_seven_generates_new_modifier_triggers() {
    let ctx = TestContext::new().await;
    // given
    // when
    ctx.advance_and_roll(SEVEN_VRF_NUMBER).await;
    let triggers = modifier_triggers_for_roll(SEVEN_VRF_NUMBER);

    // then
    let actual = ctx
        .owner_instance()
        .methods()
        .modifier_triggers()
        .call()
        .await
        .unwrap()
        .value;
    let expected = triggers
        .into_iter()
        .map(|(a, b, c)| (a, b, c, false))
        .collect::<Vec<_>>();
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn roll_dice__if_hit_the_modifier_value_triggers_the_modifier_to_be_purchased() {
    let ctx = TestContext::new().await;
    // given
    ctx.advance_and_roll(SEVEN_VRF_NUMBER).await;

    let triggers = modifier_triggers_for_roll(SEVEN_VRF_NUMBER);

    let (trigger_roll, _, _) = triggers.first().unwrap().clone();
    let vrf_number = roll_to_vrf_number(&trigger_roll);
    // when
    ctx.advance_and_roll(vrf_number).await;

    // then
    let actual = ctx
        .owner_instance()
        .methods()
        .modifier_triggers()
        .call()
        .await
        .unwrap()
        .value;
    let mut expected = triggers
        .into_iter()
        .map(|(a, b, c)| (a, b, c, false))
        .collect::<Vec<_>>();
    if let Some((_, _, _, triggered)) = expected.first_mut() {
        *triggered = true;
    } else {
        panic!("Expected at least one modifier trigger");
    }
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn roll_dice__resets_active_modifiers_and_triggers() {
    let ctx = TestContext::new().await;
    // given
    ctx.advance_and_roll(SEVEN_VRF_NUMBER).await;
    let (trigger_roll, modifier_roll, modifier) =
        modifier_triggers_for_roll(SEVEN_VRF_NUMBER)
            .first()
            .unwrap()
            .clone();
    let vrf_number = roll_to_vrf_number(&trigger_roll);
    ctx.advance_and_roll(vrf_number).await; // Two -> trigger Burnt modifier

    let chip_asset_id = ctx.chip_asset_id();
    let call_params = CallParameters::new(1, chip_asset_id, 1_000_000);
    ctx.alice_instance()
        .methods()
        .purchase_modifier(modifier_roll, modifier)
        .call_params(call_params)
        .unwrap()
        .call()
        .await
        .unwrap();

    // when
    let different_seven_vrf_number = 15 + 36;
    ctx.advance_and_roll(different_seven_vrf_number).await; // Seven -> new game resets state
    let expected_triggers = modifier_triggers_for_roll(different_seven_vrf_number)
        .into_iter()
        .map(|(a, b, c)| (a, b, c, false))
        .collect::<Vec<_>>();

    // then
    let triggers = ctx
        .owner_instance()
        .methods()
        .modifier_triggers()
        .call()
        .await
        .unwrap()
        .value;
    assert_eq!(expected_triggers, triggers);

    let active_modifiers = ctx
        .alice_instance()
        .methods()
        .active_modifiers()
        .call()
        .await
        .unwrap()
        .value;
    assert!(active_modifiers.is_empty());
}
