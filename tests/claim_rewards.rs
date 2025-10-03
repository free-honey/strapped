#![allow(non_snake_case)]

use fuels::{
    accounts::ViewOnlyAccount,
    prelude::{
        AssetConfig,
        AssetId,
        CallParameters,
        Execution,
        VariableOutputPolicy,
    },
    tx::ContractIdExt,
};
use proptest::prelude::*;
use strapped_contract::{
    contract_id,
    strap_to_sub_id,
    strapped_types::{
        self,
        Bet,
        Modifier,
        Roll,
        Strap,
        StrapKind,
    },
    test_helpers::*,
};
use tokio::runtime::Runtime;

pub const SIX_VRF_NUMBER: u64 = 10;
pub const SEVEN_VRF_NUMBER: u64 = 15;

proptest! {
    #![proptest_config(ProptestConfig { cases: 10, .. ProptestConfig::default() })]
    #[test]
    fn claim_rewards__adds_chips_to_wallet((vrf_number, bet_amount) in (0u64..36, 1u64..=1_000u64)) {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            _claim_rewards__adds_chips_to_wallet(vrf_number, bet_amount).await.unwrap()
        });
    }
}

async fn _claim_rewards__adds_chips_to_wallet(
    vrf_number: u64,
    bet_amount: u64,
) -> Result<(), TestCaseError> {
    let ctx = TestContext::new().await;

    // given
    let chip_asset_id = ctx.chip_asset_id();
    let payout_config = ctx
        .owner_contract()
        .methods()
        .payouts()
        .simulate(Execution::StateReadOnly)
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
        .simulate(Execution::StateReadOnly)
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
        .simulate(Execution::StateReadOnly)
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
        .simulate(Execution::StateReadOnly)
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
        .simulate(Execution::StateReadOnly)
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

mod _claim_rewards__can_receive_strap_token {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig { cases: 10, .. ProptestConfig::default() })]
        #[test]
        fn claim_rewards__includes_modifier_in_strap_level_up(seven_mult in 1u64..=1000u64, seven_base in 15u64..=20) {
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                _claim_rewards__can_receive_strap_token(seven_base, seven_mult).await;
            });
        }
    }

    async fn _claim_rewards__can_receive_strap_token(seven_base: u64, seven_mult: u64) {
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::ERROR)
            .try_init();

        let ctx = TestContext::new().await;
        let seven_vrf_number = seven_base + (seven_mult * 36);
        ctx.advance_and_roll(seven_vrf_number).await;

        // given
        let bet_game_id = ctx
            .alice_contract()
            .methods()
            .current_game_id()
            .simulate(Execution::StateReadOnly)
            .await
            .unwrap()
            .value;
        let generate_straps = generate_straps(seven_vrf_number);
        let (roll, strap) = generate_straps.first().clone().unwrap();
        // tracing::error!("straps generated: {:?}", generate_straps);

        let vrf_number = ctx
            .vrf_contract()
            .methods()
            .get_random(1)
            .simulate(Execution::StateReadOnly)
            .await
            .unwrap()
            .value;
        assert_eq!(
            vrf_number, seven_vrf_number,
            "VRF number does not match expected seven_vrf_number"
        );
        let actual_reward_list = ctx
            .alice_contract()
            .methods()
            .strap_rewards()
            .simulate(Execution::StateReadOnly)
            .await
            .unwrap()
            .value;
        assert_eq!(
            actual_reward_list, generate_straps,
            "rewards list does not match expected for vrf number: {:?}",
            vrf_number
        );

        place_chip_bet(&ctx, roll.clone(), 100).await;
        let vrf_number = roll_to_vrf_number(&roll);
        ctx.advance_and_roll(vrf_number).await;
        ctx.advance_and_roll(SEVEN_VRF_NUMBER).await; // Seven to end game

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
            .with_variable_output_policy(VariableOutputPolicy::EstimateMinimum)
            .call()
            .await
            .unwrap();

        // then
        let balance_after = ctx
            .alice()
            .get_asset_balance(&strap_asset_id)
            .await
            .unwrap();
        // assert_eq!(
        //     balance_after,
        //     balance_before + 1,
        //     "Failed to receive straps {:?}, with deets: seven_vrf_number: {:?}",
        //     generate_straps,
        //     seven_vrf_number
        // );
        let expected = balance_before + 1;
        if balance_after != expected {
            panic!(
                "Failed to receive straps {:?}, with deets: seven_vrf_number: {:?}\n balance_before: {:?}, balance_after: {:?}",
                generate_straps, seven_vrf_number, expected, balance_after
            );
        } else {
            tracing::error!("successfully received strap rewards: {:?}", generate_straps);
        }
    }
}

#[tokio::test]
async fn claim_rewards__will_only_receive_one_strap_reward_per_roll() {
    let ctx = TestContext::new().await;

    ctx.advance_and_roll(SEVEN_VRF_NUMBER).await; // seed strap rewards

    // given
    let (roll, strap) = generate_straps(SEVEN_VRF_NUMBER).first().unwrap().clone();
    place_chip_bet(&ctx, roll.clone(), 100).await;
    place_chip_bet(&ctx, roll.clone(), 100).await;

    let bet_game_id = ctx
        .alice_contract()
        .methods()
        .current_game_id()
        .simulate(Execution::StateReadOnly)
        .await
        .unwrap()
        .value;

    let vrf_number = roll_to_vrf_number(&roll);
    ctx.advance_and_roll(vrf_number).await; // Eight
    ctx.advance_and_roll(SEVEN_VRF_NUMBER).await; // Seven to end game

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
        .simulate(Execution::StateReadOnly)
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
        .simulate(Execution::StateReadOnly)
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

mod _claim_rewards__includes_modifier_in_strap_level_up {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig { cases: 10, .. ProptestConfig::default() })]
        #[test]
        fn claim_rewards__includes_modifier_in_strap_level_up(seven_mult in 1u64..=1000u64, seven_base in 15u64..=20) {
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                _claim_rewards__includes_modifier_in_strap_level_up(seven_base, seven_mult).await;
            });
        }
    }

    async fn _claim_rewards__includes_modifier_in_strap_level_up(
        seven_base: u64,
        seven_mult: u64,
    ) {
        // given
        let base_contract_id = contract_id();
        let base_strap = Strap::new(1, StrapKind::Shirt, Modifier::Nothing);
        let base_strap_asset = base_contract_id.asset_id(&strap_to_sub_id(&base_strap));

        let ctx = TestContext::new_with_extra_assets(vec![AssetConfig {
            id: base_strap_asset,
            num_coins: 20,
            coin_amount: 1,
        }])
        .await;

        let some_seven_vrf_number = seven_base + (seven_mult * 36);
        ctx.advance_and_roll(some_seven_vrf_number).await; // seed modifiers
        let available_triggers = modifier_triggers_for_roll(some_seven_vrf_number);
        let deets = format!(
            "seven_base: {:?}, seven_mult: {:?}, total: {:?}, available_triggers: {:?}",
            seven_base, seven_mult, some_seven_vrf_number, available_triggers
        );
        let bet_game_id = ctx
            .alice_contract()
            .methods()
            .current_game_id()
            .simulate(Execution::StateReadOnly)
            .await
            .unwrap()
            .value;
        let mut seven_rolled = false;
        for (trigger_roll, modifier_roll, modifier) in available_triggers.clone().iter() {
            let vrf_number = roll_to_vrf_number(&trigger_roll);
            ctx.advance_and_roll(vrf_number).await; // trigger modifier

            ctx.alice_contract()
                .methods()
                .purchase_modifier(modifier_roll.clone(), modifier.clone())
                .call_params(CallParameters::new(1, ctx.chip_asset_id(), 1_000_000))
                .unwrap()
                .call()
                .await
                .expect(&deets);
            place_strap_bet(&ctx, &base_strap, modifier_roll.clone(), 1).await;
            let vrf_number = roll_to_vrf_number(&modifier_roll);
            ctx.advance_and_roll(vrf_number).await;
            if *modifier_roll == Roll::Seven {
                seven_rolled = true;
                break;
            }
        }

        ctx.advance_and_roll(SEVEN_VRF_NUMBER).await; // end game

        let enabled_modifiers = available_triggers
            .clone()
            .into_iter()
            .map(|(_, modifier_roll, modifier)| (modifier_roll, modifier))
            .collect::<Vec<_>>();
        // when
        ctx.alice_contract()
            .methods()
            .claim_rewards(bet_game_id, enabled_modifiers)
            .with_variable_output_policy(VariableOutputPolicy::EstimateMinimum)
            .call()
            .await
            .expect(&format!(
                "Failed to claim rewards with modifiers: {:?}",
                available_triggers
            ));

        // then
        for (_, modifier_roll, modifier) in available_triggers {
            let leveled_strap = Strap::new(2, StrapKind::Shirt, modifier);
            let leveled_asset_id = strap_asset_id(&ctx, &leveled_strap);
            let balance = ctx
                .alice()
                .get_asset_balance(&leveled_asset_id)
                .await
                .unwrap();
            assert_eq!(
                balance, 1,
                "Failed check for strap {:?} with deets: {:?}",
                leveled_strap, deets
            );
            if modifier_roll == Roll::Seven {
                break
            }
        }
    }
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
    let (trigger_roll, modifier_roll, modifier) =
        modifier_triggers_for_roll(SEVEN_VRF_NUMBER)
            .first()
            .unwrap()
            .clone();
    let vrf_number = roll_to_vrf_number(&trigger_roll);
    ctx.advance_and_roll(vrf_number).await; // trigger Burnt modifier

    ctx.alice_contract()
        .methods()
        .purchase_modifier(modifier_roll.clone(), modifier.clone())
        .call_params(CallParameters::new(1, ctx.chip_asset_id(), 1_000_000))
        .unwrap()
        .call()
        .await
        .unwrap();

    place_strap_bet(&ctx, &base_strap, modifier_roll.clone(), 1).await;

    let bet_game_id = ctx
        .alice_contract()
        .methods()
        .current_game_id()
        .simulate(Execution::StateReadOnly)
        .await
        .unwrap()
        .value;

    let vrf_number = roll_to_vrf_number(&modifier_roll);
    ctx.advance_and_roll(vrf_number).await; // hit six
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
