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
use generated_abi::{
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
use proptest::prelude::*;
use tokio::runtime::Runtime;

pub const SIX_VRF_NUMBER: u64 = 10;
pub const SEVEN_VRF_NUMBER: u64 = 15;

// prop strategy for generating a random vrf_number that will roll "Seven"
prop_compose! {
    fn seven_vrf_number()(seven_mult in 1u64..=1000u64, seven_base in 15u64..=20) -> u64 {
        seven_base + (seven_mult * 36)
    }
}

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
        .simulate(Execution::state_read_only())
        .await
        .unwrap()
        .value;

    let target_roll = roll_from_vrf_bucket(vrf_number);
    let winnings = calculate_payout(&payout_config, &target_roll, bet_amount);

    place_chip_bet(&ctx, target_roll.clone(), bet_amount).await;

    let bet_game_id = ctx
        .alice_contract()
        .methods()
        .current_game_id()
        .simulate(Execution::state_read_only())
        .await
        .unwrap()
        .value;

    ctx.advance_and_roll(vrf_number).await;
    // roll seven to end game if not already rolled
    if target_roll != Roll::Seven {
        ctx.advance_and_roll(SEVEN_VRF_NUMBER).await;
    }

    let balance_before: u64 = ctx
        .alice()
        .get_asset_balance(&chip_asset_id)
        .await
        .unwrap()
        .try_into()
        .unwrap();

    // when
    if winnings != 0 {
        ctx.alice_contract()
            .methods()
            .claim_rewards(bet_game_id, Vec::new())
            .with_variable_output_policy(VariableOutputPolicy::EstimateMinimum)
            .call()
            .await
            .unwrap();
    }

    // then
    let expected = balance_before + winnings;
    let actual: u64 = ctx
        .alice()
        .get_asset_balance(&chip_asset_id)
        .await
        .unwrap()
        .try_into()
        .unwrap();
    prop_assert_eq!(expected, actual);
    Ok(())
}

#[tokio::test]
async fn claim_rewards__multiple_hits_results_in_additional_winnings() {
    let ctx = TestContext::new().await;

    // given
    let chip_asset_id = ctx.chip_asset_id();
    let payout_config = ctx
        .owner_contract()
        .methods()
        .payouts()
        .simulate(Execution::state_read_only())
        .await
        .unwrap()
        .value;

    let bet_amount = 100;
    let roll = Roll::Six;
    place_chip_bet(&ctx, roll.clone(), bet_amount).await;

    let bet_game_id = ctx
        .alice_contract()
        .methods()
        .current_game_id()
        .simulate(Execution::state_read_only())
        .await
        .unwrap()
        .value;
    let hits = 3;

    for _ in 0..hits {
        ctx.advance_and_roll(SIX_VRF_NUMBER).await;
    }
    ctx.advance_and_roll(SEVEN_VRF_NUMBER).await;

    let balance_before: u64 = ctx
        .alice()
        .get_asset_balance(&chip_asset_id)
        .await
        .unwrap()
        .try_into()
        .unwrap();

    // when
    ctx.alice_contract()
        .methods()
        .claim_rewards(bet_game_id, Vec::new())
        .with_variable_output_policy(VariableOutputPolicy::Exactly(1))
        .call()
        .await
        .unwrap();

    // then
    let balance_after: u64 = ctx
        .alice()
        .get_asset_balance(&chip_asset_id)
        .await
        .unwrap()
        .try_into()
        .unwrap();
    let winnings = hits * calculate_payout(&payout_config, &roll, bet_amount);
    let expected = balance_before + winnings;
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
        .simulate(Execution::state_read_only())
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
        .simulate(Execution::state_read_only())
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
        .simulate(Execution::state_read_only())
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

// because hashable
fn roll_to_number(roll: &Roll) -> u64 {
    match roll {
        Roll::Two => 2,
        Roll::Three => 3,
        Roll::Four => 4,
        Roll::Five => 5,
        Roll::Six => 6,
        Roll::Seven => 7,
        Roll::Eight => 8,
        Roll::Nine => 9,
        Roll::Ten => 10,
        Roll::Eleven => 11,
        Roll::Twelve => 12,
    }
}

mod _claim_rewards__can_receive_strap_token {
    use super::*;
    use std::collections::HashMap;

    proptest! {
        #![proptest_config(ProptestConfig { cases: 10, .. ProptestConfig::default() })]
        #[test]
        fn claim_rewards__can_receive_strap_token(seven_vrf_number in seven_vrf_number()) {
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                _claim_rewards__can_receive_strap_token(seven_vrf_number).await;
            });
        }
    }

    async fn _claim_rewards__can_receive_strap_token(seven_vrf_number: u64) {
        let ctx = TestContext::new().await;
        ctx.advance_and_roll(seven_vrf_number).await;

        // given
        let bet = 1_000;
        let mut bets_for_roll = HashMap::new();
        let bet_game_id = ctx
            .alice_contract()
            .methods()
            .current_game_id()
            .simulate(Execution::state_read_only())
            .await
            .unwrap()
            .value;
        let generate_straps = generate_straps(seven_vrf_number);
        let available_rewards = ctx
            .owner_contract()
            .methods()
            .strap_rewards()
            .simulate(Execution::state_read_only())
            .await
            .unwrap()
            .value;
        debug_assert_eq!(&generate_straps, &available_rewards);

        for (roll, _strap, _cost) in generate_straps.clone() {
            place_chip_bet(&ctx, roll.clone(), bet).await;
            let roll_number = roll_to_number(&roll);
            let entry = bets_for_roll.entry(roll_number).or_insert(0);
            *entry += bet;
        }

        for (roll, _strap, _cost) in generate_straps.clone() {
            let vrf_number = roll_to_vrf_number(&roll);
            ctx.advance_and_roll(vrf_number).await;
        }

        let mut asset_id_to_strap = HashMap::new();

        ctx.advance_and_roll(SEVEN_VRF_NUMBER).await; // Seven to end game

        let mut expected_straps_rewards = HashMap::new();
        let rolls = generate_straps.iter().map(|(roll, _, _)| roll);
        for roll in rolls {
            for (target_roll, strap, cost) in &generate_straps {
                if target_roll == roll {
                    let strap_asset_id = strap_asset_id(&ctx, strap);
                    asset_id_to_strap.insert(strap_asset_id.clone(), strap.clone());
                    let total_bet =
                        bets_for_roll.get(&roll_to_number(roll)).cloned().unwrap();
                    let won_straps = total_bet / cost;
                    let entry = expected_straps_rewards
                        .entry(strap_asset_id.clone())
                        .or_insert(0);
                    *entry += won_straps;
                }
            }
        }

        let mut balances_before = HashMap::new();
        for (strap_asset_id, _) in expected_straps_rewards.iter() {
            let balance: u64 = ctx
                .alice()
                .get_asset_balance(strap_asset_id)
                .await
                .unwrap()
                .try_into()
                .unwrap();
            balances_before.insert(strap_asset_id.clone(), balance);
        }

        // when
        ctx.alice_contract()
            .methods()
            .claim_rewards(bet_game_id, Vec::new())
            .with_variable_output_policy(VariableOutputPolicy::EstimateMinimum)
            .call()
            .await
            .expect(&format!(
                "Failed to claim rewards with deets: seven_vrf_number: {:?}, generate_straps: {:?}",
                seven_vrf_number, generate_straps
            ));

        // then
        for (strap_asset_id, reward_amount) in expected_straps_rewards.iter() {
            let strap = asset_id_to_strap.get(strap_asset_id).unwrap();
            let balance_before: u64 = *balances_before.get(strap_asset_id).unwrap();
            let balance_after: u64 = ctx
                .alice()
                .get_asset_balance(&strap_asset_id)
                .await
                .unwrap()
                .try_into()
                .unwrap();

            let expected = balance_before + reward_amount;
            if balance_after != expected {
                panic!(
                    "Failed to receive straps {:?},\n particular strap {:?},\n with deets: seven_vrf_number: {:?}\n balance_before: {:?},\n expected_balance_after: {:?},\n balance_after: {:?}",
                    generate_straps,
                    strap,
                    seven_vrf_number,
                    balance_before,
                    expected,
                    balance_after
                );
            } else {
                tracing::error!(
                    "successfully received strap rewards: {:?}",
                    generate_straps
                );
            }
        }
    }
}

#[tokio::test]
async fn claim_rewards__bet_straps_are_levelled_up() {
    // given
    let base_contract_id = contract_id();
    let base_strap = Strap::new(1, StrapKind::Shirt, Modifier::Nothing);
    let base_strap_asset = base_contract_id.asset_id(&strap_to_sub_id(&base_strap));

    let ctx = TestContext::builder()
        .with_extra_assets(vec![AssetConfig {
            id: base_strap_asset,
            num_coins: 1,
            coin_amount: 1,
        }])
        .build()
        .await;

    place_strap_bet(&ctx, &base_strap, Roll::Six, 1).await;

    let bet_game_id = ctx
        .alice_contract()
        .methods()
        .current_game_id()
        .simulate(Execution::state_read_only())
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

    let ctx = TestContext::builder()
        .with_extra_assets(vec![AssetConfig {
            id: base_strap_asset,
            num_coins: 1,
            coin_amount: 1,
        }])
        .build()
        .await;

    place_strap_bet(&ctx, &base_strap, Roll::Six, 1).await;

    let bet_game_id = ctx
        .alice_contract()
        .methods()
        .current_game_id()
        .simulate(Execution::state_read_only())
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
        fn claim_rewards__includes_modifier_in_strap_level_up(seven_vrf_number in seven_vrf_number()) {
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                _claim_rewards__includes_modifier_in_strap_level_up(seven_vrf_number).await;
            });
        }
    }

    async fn _claim_rewards__includes_modifier_in_strap_level_up(
        some_seven_vrf_number: u64,
    ) {
        // given
        let base_contract_id = contract_id();
        let straps = vec![
            Strap::new(1, StrapKind::Shirt, Modifier::Nothing),
            Strap::new(1, StrapKind::Pants, Modifier::Nothing),
            Strap::new(1, StrapKind::Shoes, Modifier::Nothing),
            Strap::new(1, StrapKind::Dress, Modifier::Nothing),
            Strap::new(1, StrapKind::Hat, Modifier::Nothing),
            Strap::new(1, StrapKind::Glasses, Modifier::Nothing),
            Strap::new(1, StrapKind::Watch, Modifier::Nothing),
            Strap::new(1, StrapKind::Ring, Modifier::Nothing),
            Strap::new(1, StrapKind::Necklace, Modifier::Nothing),
            Strap::new(1, StrapKind::Earring, Modifier::Nothing),
            Strap::new(1, StrapKind::Bracelet, Modifier::Nothing),
            Strap::new(1, StrapKind::Tattoo, Modifier::Nothing),
            Strap::new(1, StrapKind::Skirt, Modifier::Nothing),
            Strap::new(1, StrapKind::Piercing, Modifier::Nothing),
            Strap::new(1, StrapKind::Coat, Modifier::Nothing),
            Strap::new(1, StrapKind::Scarf, Modifier::Nothing),
            Strap::new(1, StrapKind::Gloves, Modifier::Nothing),
            Strap::new(1, StrapKind::Gown, Modifier::Nothing),
            Strap::new(1, StrapKind::Belt, Modifier::Nothing),
        ];

        let extra_assets = straps
            .iter()
            .map(|strap| AssetConfig {
                id: base_contract_id.asset_id(&strap_to_sub_id(strap)),
                num_coins: 20,
                coin_amount: 1,
            })
            .collect::<Vec<_>>();

        let ctx = TestContext::builder()
            .with_extra_assets(extra_assets)
            .build()
            .await;

        ctx.advance_and_roll(some_seven_vrf_number).await; // seed modifiers
        let available_triggers = modifier_triggers_for_roll(some_seven_vrf_number);
        let deets = format!(
            "seven_vrf_number: {:?}, available_triggers: {:?}",
            some_seven_vrf_number, available_triggers
        );
        let bet_game_id = ctx
            .alice_contract()
            .methods()
            .current_game_id()
            .simulate(Execution::state_read_only())
            .await
            .unwrap()
            .value;
        let mut strap_expectations: Vec<(Strap, Modifier)> = Vec::new();

        for (idx, (trigger_roll, modifier_roll, modifier)) in
            available_triggers.clone().iter().enumerate()
        {
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
            let strap_a = straps
                .get(idx)
                .expect("not enough base straps configured for test cases");
            place_strap_bet(&ctx, strap_a, modifier_roll.clone(), 1).await;
            strap_expectations.push((strap_a.clone(), modifier.clone()));
            let vrf_number = roll_to_vrf_number(&modifier_roll);
            ctx.advance_and_roll(vrf_number).await;
            if *modifier_roll == Roll::Seven {
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
        for (strap, modifier) in strap_expectations {
            let leveled_strap = Strap::new(2, strap.kind.clone(), modifier);
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
        }
    }
}

#[tokio::test]
async fn claim_rewards__does_not_include_modifier_if_not_specified() {
    // given
    let base_contract_id = contract_id();
    let base_strap = Strap::new(1, StrapKind::Shirt, Modifier::Nothing);
    let base_strap_asset = base_contract_id.asset_id(&strap_to_sub_id(&base_strap));

    let ctx = TestContext::builder()
        .with_extra_assets(vec![AssetConfig {
            id: base_strap_asset,
            num_coins: 1,
            coin_amount: 1,
        }])
        .build()
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
        .simulate(Execution::state_read_only())
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
