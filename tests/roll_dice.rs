#![allow(non_snake_case)]

use fuels::prelude::CallParameters;
use strapped_contract::{
    strapped_types::{Modifier, Roll, Strap, StrapKind},
    test_helpers::TestContext,
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
        .call()
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
    let actual = ctx
        .owner_instance()
        .methods()
        .strap_rewards()
        .call()
        .await
        .unwrap()
        .value;

    let expected = vec![(
        Roll::Eight,
        Strap::new(1, StrapKind::Shirt, Modifier::Nothing),
    )];
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn roll_dice__if_seven_generates_new_modifier_triggers() {
    let ctx = TestContext::new().await;
    // given
    // when
    ctx.advance_and_roll(SEVEN_VRF_NUMBER).await;

    // then
    let actual = ctx
        .owner_instance()
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
    // given
    ctx.advance_and_roll(SEVEN_VRF_NUMBER).await;

    // when
    ctx.advance_and_roll(TWO_VRF_NUMBER).await;

    // then
    let actual = ctx
        .owner_instance()
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
    let ctx = TestContext::new().await;
    // given
    ctx.advance_and_roll(SEVEN_VRF_NUMBER).await;
    ctx.advance_and_roll(TWO_VRF_NUMBER).await;

    let chip_asset_id = ctx.chip_asset_id();
    let call_params = CallParameters::new(1, chip_asset_id, 1_000_000);
    ctx.alice_instance()
        .methods()
        .purchase_modifier(Roll::Six, Modifier::Burnt)
        .call_params(call_params)
        .unwrap()
        .call()
        .await
        .unwrap();

    // when
    ctx.advance_and_roll(19).await; // Seven -> new game resets state

    // then
    let triggers = ctx
        .owner_instance()
        .methods()
        .modifier_triggers()
        .call()
        .await
        .unwrap()
        .value;
    let expected_triggers = vec![
        (Roll::Two, Roll::Six, Modifier::Burnt, false),
        (Roll::Twelve, Roll::Eight, Modifier::Lucky, false),
    ];
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
