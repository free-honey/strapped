#![allow(non_snake_case)]

use fuels::{
    accounts::ViewOnlyAccount,
    prelude::{AssetConfig, AssetId, CallParameters, Execution, VariableOutputPolicy},
    tx::ContractIdExt,
};
use proptest::prelude::*;
use strapped_contract::{
    contract_id, strap_to_sub_id,
    strapped_types::{self, Bet, Modifier, Roll, Strap, StrapKind},
    test_helpers::TestContext,
};
use tokio::runtime::Runtime;

pub const SIX_VRF_NUMBER: u64 = 10;
pub const SEVEN_VRF_NUMBER: u64 = 19;

proptest! {
    #![proptest_config(ProptestConfig { cases: 10, .. ProptestConfig::default() })]
    #[test]
    fn claim_rewards__adds_chips_to_wallet((vrf_number, bet_amount) in (0u64..36, 1u64..=1_000u64)) {
        run_claim_rewards_property(vrf_number, bet_amount).unwrap();
    }
}

fn run_claim_rewards_property(
    vrf_number: u64,
    bet_amount: u64,
) -> Result<(), TestCaseError> {
    Runtime::new().unwrap().block_on(async move {
        let ctx = TestContext::new().await;

        // given
        let chip_asset_id = ctx.chip_asset_id();
        let payout_config = ctx
            .owner_contract()
            .methods()
            .payouts()
            .call()
            .await
            .unwrap()
            .value;

        let target_roll = roll_from_vrf_bucket(vrf_number);
        let expected_multiplier = multiplier_for_roll(&payout_config, &target_roll);

        place_chip_bet(&ctx, target_roll.clone(), bet_amount).await;

        let bet_game_id = ctx
            .alice_contract()
            .methods()
            .current_game_id()
            .simulate(Execution::StateReadOnly)
            .await
            .unwrap()
            .value;

        ctx.advance_and_roll(vrf_number).await;
        // roll seven to end game if not already rolled
        if target_roll != Roll::Seven {
            ctx.advance_and_roll(SEVEN_VRF_NUMBER).await;
        }

        let balance_before = ctx.alice().get_asset_balance(&chip_asset_id).await.unwrap();

        // when
        if expected_multiplier != 0 {
            ctx.alice_contract()
                .methods()
                .claim_rewards(bet_game_id, Vec::new())
                .with_variable_output_policy(VariableOutputPolicy::EstimateMinimum)
                .call()
                .await
                .unwrap();
        }

        // then
        let expected = balance_before + bet_amount * expected_multiplier;
        let actual = ctx.alice().get_asset_balance(&chip_asset_id).await.unwrap();
        prop_assert_eq!(expected, actual);
        Ok(())
    })
}

#[tokio::test]
async fn claim_rewards__multiple_hits_results_in_additional_winnings() {
    let ctx = TestContext::new().await;

    // given
    let chip_asset_id = ctx.chip_asset_id();

    let bet_amount = 100;
    let roll = Roll::Six;
    place_chip_bet(&ctx, roll.clone(), bet_amount).await;

    let bet_game_id = ctx
        .alice_contract()
        .methods()
        .current_game_id()
        .call()
        .await
        .unwrap()
        .value;

    ctx.advance_and_roll(SIX_VRF_NUMBER).await;
    ctx.advance_and_roll(SIX_VRF_NUMBER).await;
    ctx.advance_and_roll(SIX_VRF_NUMBER).await;
    ctx.advance_and_roll(SEVEN_VRF_NUMBER).await;

    let balance_before = ctx.alice().get_asset_balance(&chip_asset_id).await.unwrap();

    // when
    ctx.alice_contract()
        .methods()
        .claim_rewards(bet_game_id, Vec::new())
        .with_variable_output_policy(VariableOutputPolicy::Exactly(1))
        .call()
        .await
        .unwrap();

    // then
    let balance_after = ctx.alice().get_asset_balance(&chip_asset_id).await.unwrap();
    let expected = balance_before + bet_amount * 2 * 3;
    assert_eq!(balance_after, expected);
}

#[tokio::test]
async fn claim_rewards__cannot_claim_rewards_for_current_game() {
    let ctx = TestContext::new().await;

    // given
    let chip_asset_id = ctx.chip_asset_id();

    place_chip_bet(&ctx, Roll::Six, 100).await;

    let bet_game_id = ctx
        .alice_contract()
        .methods()
        .current_game_id()
        .call()
        .await
        .unwrap()
        .value;

    ctx.advance_and_roll(SIX_VRF_NUMBER).await;

    let balance_before = ctx.alice().get_asset_balance(&chip_asset_id).await.unwrap();

    // when
    let result = ctx
        .alice_contract()
        .methods()
        .claim_rewards(bet_game_id, Vec::new())
        .with_variable_output_policy(VariableOutputPolicy::Exactly(1))
        .call()
        .await;

    // then
    assert!(result.is_err());
    let balance_after = ctx.alice().get_asset_balance(&chip_asset_id).await.unwrap();
    assert_eq!(balance_after, balance_before);
}

#[tokio::test]
async fn claim_rewards__do_not_reward_bets_placed_after_roll() {
    let ctx = TestContext::new().await;

    // given
    let chip_asset_id = ctx.chip_asset_id();

    ctx.advance_and_roll(SIX_VRF_NUMBER).await; // roll happens before bet

    place_chip_bet(&ctx, Roll::Six, 100).await;

    // when
    let bet_game_id = ctx
        .alice_contract()
        .methods()
        .current_game_id()
        .call()
        .await
        .unwrap()
        .value;

    ctx.advance_and_roll(SEVEN_VRF_NUMBER).await; // end game

    let balance_before = ctx.alice().get_asset_balance(&chip_asset_id).await.unwrap();

    ctx.alice_contract()
        .methods()
        .claim_rewards(bet_game_id, Vec::new())
        .with_variable_output_policy(VariableOutputPolicy::Exactly(1))
        .call()
        .await
        .unwrap_err();

    // then
    let balance_after = ctx.alice().get_asset_balance(&chip_asset_id).await.unwrap();
    assert_eq!(balance_before, balance_after);
}

#[tokio::test]
async fn claim_rewards__cannot_claim_rewards_twice() {
    let ctx = TestContext::new().await;

    // given
    let chip_asset_id = ctx.chip_asset_id();

    place_chip_bet(&ctx, Roll::Six, 100).await;

    let bet_game_id = ctx
        .alice_contract()
        .methods()
        .current_game_id()
        .call()
        .await
        .unwrap()
        .value;

    ctx.advance_and_roll(SIX_VRF_NUMBER).await;
    ctx.advance_and_roll(SEVEN_VRF_NUMBER).await;

    // when
    ctx.alice_contract()
        .methods()
        .claim_rewards(bet_game_id, Vec::new())
        .with_variable_output_policy(VariableOutputPolicy::Exactly(1))
        .call()
        .await
        .unwrap();

    let balance_after_first =
        ctx.alice().get_asset_balance(&chip_asset_id).await.unwrap();

    let result = ctx
        .alice_contract()
        .methods()
        .claim_rewards(bet_game_id, Vec::new())
        .with_variable_output_policy(VariableOutputPolicy::Exactly(1))
        .call()
        .await;

    // then
    assert!(result.is_err());
    let balance_after_second =
        ctx.alice().get_asset_balance(&chip_asset_id).await.unwrap();
    assert_eq!(balance_after_first, balance_after_second);
}

#[tokio::test]
async fn claim_rewards__can_receive_strap_token() {
    let ctx = TestContext::new().await;
    ctx.advance_and_roll(SEVEN_VRF_NUMBER).await; // seed strap rewards for roll eight

    // given
    place_chip_bet(&ctx, Roll::Eight, 100).await;

    let bet_game_id = ctx
        .alice_contract()
        .methods()
        .current_game_id()
        .call()
        .await
        .unwrap()
        .value;

    ctx.advance_and_roll(25).await; // Eight
    ctx.advance_and_roll(SEVEN_VRF_NUMBER).await; // Seven to end game

    let strap = Strap::new(1, StrapKind::Shirt, Modifier::Nothing);
    let strap_asset_id = strap_asset_id(&ctx, &strap);
    let balance_before = ctx
        .alice()
        .get_asset_balance(&strap_asset_id)
        .await
        .unwrap();

    // when
    ctx.alice_contract()
        .methods()
        .claim_rewards(bet_game_id, Vec::new())
        .with_variable_output_policy(VariableOutputPolicy::Exactly(2))
        .call()
        .await
        .unwrap();

    // then
    let balance_after = ctx
        .alice()
        .get_asset_balance(&strap_asset_id)
        .await
        .unwrap();
    assert_eq!(balance_after, balance_before + 1);
}

#[tokio::test]
async fn claim_rewards__will_only_receive_one_strap_reward_per_roll() {
    let ctx = TestContext::new().await;

    ctx.advance_and_roll(SEVEN_VRF_NUMBER).await; // seed strap rewards

    // given
    place_chip_bet(&ctx, Roll::Eight, 100).await;
    place_chip_bet(&ctx, Roll::Eight, 100).await;

    let bet_game_id = ctx
        .alice_contract()
        .methods()
        .current_game_id()
        .call()
        .await
        .unwrap()
        .value;

    ctx.advance_and_roll(25).await; // Eight
    ctx.advance_and_roll(SEVEN_VRF_NUMBER).await; // Seven to end game

    let strap = Strap::new(1, StrapKind::Shirt, Modifier::Nothing);
    let strap_asset_id = strap_asset_id(&ctx, &strap);
    let balance_before = ctx
        .alice()
        .get_asset_balance(&strap_asset_id)
        .await
        .unwrap();

    // when
    ctx.alice_contract()
        .methods()
        .claim_rewards(bet_game_id, Vec::new())
        .with_variable_output_policy(VariableOutputPolicy::Exactly(2))
        .call()
        .await
        .unwrap();

    // then
    let balance_after = ctx
        .alice()
        .get_asset_balance(&strap_asset_id)
        .await
        .unwrap();
    assert_eq!(balance_after, balance_before + 1);
}

#[tokio::test]
async fn claim_rewards__bet_straps_are_levelled_up() {
    // given
    let base_contract_id = contract_id();
    let base_strap = Strap::new(1, StrapKind::Shirt, Modifier::Nothing);
    let base_strap_asset = base_contract_id.asset_id(&strap_to_sub_id(&base_strap));

    let ctx = TestContext::new_with_extra_assets(vec![AssetConfig {
        id: base_strap_asset,
        num_coins: 1,
        coin_amount: 1,
    }])
    .await;

    place_strap_bet(&ctx, &base_strap, Roll::Six, 1).await;

    let bet_game_id = ctx
        .alice_contract()
        .methods()
        .current_game_id()
        .call()
        .await
        .unwrap()
        .value;

    ctx.advance_and_roll(SIX_VRF_NUMBER).await; // Six
    ctx.advance_and_roll(SEVEN_VRF_NUMBER).await; // Seven to end game

    // when
    ctx.alice_contract()
        .methods()
        .claim_rewards(bet_game_id, Vec::new())
        .with_variable_output_policy(VariableOutputPolicy::Exactly(1))
        .call()
        .await
        .unwrap();

    // then
    let leveled_strap = Strap::new(2, StrapKind::Shirt, Modifier::Nothing);
    let leveled_asset_id = strap_asset_id(&ctx, &leveled_strap);
    let balance = ctx
        .alice()
        .get_asset_balance(&leveled_asset_id)
        .await
        .unwrap();
    assert_eq!(balance, 1);
}

#[tokio::test]
async fn claim_rewards__bet_straps_only_give_one_reward_with_multiple_hits() {
    // given
    let base_contract_id = contract_id();
    let base_strap = Strap::new(1, StrapKind::Shirt, Modifier::Nothing);
    let base_strap_asset = base_contract_id.asset_id(&strap_to_sub_id(&base_strap));

    let ctx = TestContext::new_with_extra_assets(vec![AssetConfig {
        id: base_strap_asset,
        num_coins: 1,
        coin_amount: 1,
    }])
    .await;

    place_strap_bet(&ctx, &base_strap, Roll::Six, 1).await;

    let bet_game_id = ctx
        .alice_contract()
        .methods()
        .current_game_id()
        .call()
        .await
        .unwrap()
        .value;

    ctx.advance_and_roll(SIX_VRF_NUMBER).await;
    ctx.advance_and_roll(SIX_VRF_NUMBER).await;
    ctx.advance_and_roll(SEVEN_VRF_NUMBER).await;

    // when
    ctx.alice_contract()
        .methods()
        .claim_rewards(bet_game_id, Vec::new())
        .with_variable_output_policy(VariableOutputPolicy::Exactly(1))
        .call()
        .await
        .unwrap();

    // then
    let leveled_strap = Strap::new(2, StrapKind::Shirt, Modifier::Nothing);
    let leveled_asset_id = strap_asset_id(&ctx, &leveled_strap);
    let balance = ctx
        .alice()
        .get_asset_balance(&leveled_asset_id)
        .await
        .unwrap();
    assert_eq!(balance, 1);
}

#[tokio::test]
async fn claim_rewards__includes_modifier_in_strap_level_up() {
    // given
    let base_contract_id = contract_id();
    let base_strap = Strap::new(1, StrapKind::Shirt, Modifier::Nothing);
    let base_strap_asset = base_contract_id.asset_id(&strap_to_sub_id(&base_strap));

    let ctx = TestContext::new_with_extra_assets(vec![AssetConfig {
        id: base_strap_asset,
        num_coins: 1,
        coin_amount: 1,
    }])
    .await;

    ctx.advance_and_roll(SEVEN_VRF_NUMBER).await; // seed modifiers
    ctx.advance_and_roll(0).await; // trigger Burnt modifier

    ctx.alice_contract()
        .methods()
        .purchase_modifier(Roll::Six, Modifier::Burnt)
        .call_params(CallParameters::new(1, ctx.chip_asset_id(), 1_000_000))
        .unwrap()
        .call()
        .await
        .unwrap();

    place_strap_bet(&ctx, &base_strap, Roll::Six, 1).await;

    let bet_game_id = ctx
        .alice_contract()
        .methods()
        .current_game_id()
        .call()
        .await
        .unwrap()
        .value;

    ctx.advance_and_roll(SIX_VRF_NUMBER).await; // hit six
    ctx.advance_and_roll(SEVEN_VRF_NUMBER).await; // end game

    // when
    ctx.alice_contract()
        .methods()
        .claim_rewards(bet_game_id, vec![(Roll::Six, Modifier::Burnt)])
        .with_variable_output_policy(VariableOutputPolicy::Exactly(1))
        .call()
        .await
        .unwrap();

    // then
    let leveled_strap = Strap::new(2, StrapKind::Shirt, Modifier::Burnt);
    let leveled_asset_id = strap_asset_id(&ctx, &leveled_strap);
    let balance = ctx
        .alice()
        .get_asset_balance(&leveled_asset_id)
        .await
        .unwrap();
    assert_eq!(balance, 1);
}

#[tokio::test]
async fn claim_rewards__does_not_include_modifier_if_not_specified() {
    // given
    let base_contract_id = contract_id();
    let base_strap = Strap::new(1, StrapKind::Shirt, Modifier::Nothing);
    let base_strap_asset = base_contract_id.asset_id(&strap_to_sub_id(&base_strap));

    let ctx = TestContext::new_with_extra_assets(vec![AssetConfig {
        id: base_strap_asset,
        num_coins: 1,
        coin_amount: 1,
    }])
    .await;

    ctx.advance_and_roll(SEVEN_VRF_NUMBER).await; // seed modifiers
    ctx.advance_and_roll(0).await; // trigger Burnt modifier

    ctx.alice_contract()
        .methods()
        .purchase_modifier(Roll::Six, Modifier::Burnt)
        .call_params(CallParameters::new(1, ctx.chip_asset_id(), 1_000_000))
        .unwrap()
        .call()
        .await
        .unwrap();

    place_strap_bet(&ctx, &base_strap, Roll::Six, 1).await;

    let bet_game_id = ctx
        .alice_contract()
        .methods()
        .current_game_id()
        .call()
        .await
        .unwrap()
        .value;

    ctx.advance_and_roll(SIX_VRF_NUMBER).await;
    ctx.advance_and_roll(SEVEN_VRF_NUMBER).await;

    // when
    ctx.alice_contract()
        .methods()
        .claim_rewards(bet_game_id, Vec::new())
        .with_variable_output_policy(VariableOutputPolicy::Exactly(1))
        .call()
        .await
        .unwrap();

    // then
    let leveled_plain = Strap::new(2, StrapKind::Shirt, Modifier::Nothing);
    let leveled_plain_asset = strap_asset_id(&ctx, &leveled_plain);
    let plain_balance = ctx
        .alice()
        .get_asset_balance(&leveled_plain_asset)
        .await
        .unwrap();
    assert_eq!(plain_balance, 1);

    let leveled_burnt = Strap::new(2, StrapKind::Shirt, Modifier::Burnt);
    let leveled_burnt_asset = strap_asset_id(&ctx, &leveled_burnt);
    let burnt_balance = ctx
        .alice()
        .get_asset_balance(&leveled_burnt_asset)
        .await
        .unwrap();
    assert_eq!(burnt_balance, 0);
}

fn roll_from_vrf_bucket(vrf_bucket: u64) -> Roll {
    match vrf_bucket % 36 {
        0 => Roll::Two,
        1 | 2 => Roll::Three,
        3 | 4 | 5 => Roll::Four,
        6 | 7 | 8 | 9 => Roll::Five,
        10 | 11 | 12 | 13 | 14 => Roll::Six,
        15 | 16 | 17 | 18 | 19 | 20 => Roll::Seven,
        21 | 22 | 23 | 24 | 25 => Roll::Eight,
        26 | 27 | 28 | 29 => Roll::Nine,
        30 | 31 | 32 => Roll::Ten,
        33 | 34 => Roll::Eleven,
        _ => Roll::Twelve,
    }
}

fn multiplier_for_roll(cfg: &strapped_types::PayoutConfig, roll: &Roll) -> u64 {
    match roll {
        Roll::Two => cfg.two_payout_multiplier,
        // Roll::Three => panic!("{}", cfg.three_payout_multiplier),
        Roll::Three => cfg.three_payout_multiplier,
        Roll::Four => cfg.four_payout_multiplier,
        Roll::Five => cfg.five_payout_multiplier,
        Roll::Six => cfg.six_payout_multiplier,
        Roll::Seven => cfg.seven_payout_multiplier,
        Roll::Eight => cfg.eight_payout_multiplier,
        Roll::Nine => cfg.nine_payout_multiplier,
        Roll::Ten => cfg.ten_payout_multiplier,
        Roll::Eleven => cfg.eleven_payout_multiplier,
        Roll::Twelve => cfg.twelve_payout_multiplier,
    }
}

async fn place_chip_bet(ctx: &TestContext, roll: Roll, amount: u64) {
    ctx.alice_contract()
        .methods()
        .place_bet(roll, Bet::Chip, amount)
        .call_params(CallParameters::new(amount, ctx.chip_asset_id(), 1_000_000))
        .unwrap()
        .call()
        .await
        .unwrap();
}

async fn place_strap_bet(ctx: &TestContext, strap: &Strap, roll: Roll, amount: u64) {
    let asset_id = strap_asset_id(ctx, strap);
    ctx.alice_contract()
        .methods()
        .place_bet(roll, Bet::Strap(strap.clone()), amount)
        .call_params(CallParameters::new(amount, asset_id, 1_000_000))
        .unwrap()
        .call()
        .await
        .unwrap();
}

fn strap_asset_id(ctx: &TestContext, strap: &Strap) -> AssetId {
    let sub_id = strap_to_sub_id(strap);
    ctx.contract_id().asset_id(&sub_id)
}
