#![allow(non_snake_case)]

use fuels::{
    accounts::wallet::Wallet,
    prelude::{
        CallParameters,
        Execution,
        VariableOutputPolicy,
    },
    programs::responses::CallResponse,
    types::AssetId,
};
use generated_abi::{
    strapped_types::{
        self,
        Bet,
        Roll,
        RollEvent,
    },
    test_helpers::{
        TestContext,
        calculate_payout,
        roll_to_vrf_number,
    },
};

const CONTRACT_CALL_GAS_LIMIT: u64 = 1_000_000;

#[tokio::test]
async fn roll_dice__roll_event_includes_snapshot_totals() {
    let ctx = TestContext::new().await;
    let chip_asset_id = ctx.chip_asset_id();
    let payout_config = ctx
        .owner_instance()
        .methods()
        .payouts()
        .simulate(Execution::state_read_only())
        .await
        .unwrap()
        .value;

    // given
    let first_bet = 400;
    let second_bet = 250;
    place_chip_bet(ctx.alice_instance(), chip_asset_id, Roll::Six, first_bet).await;
    place_chip_bet(ctx.owner_instance(), chip_asset_id, Roll::Six, second_bet).await;

    // when
    let response = roll_with_logs(&ctx, roll_to_vrf_number(&Roll::Six)).await;

    // then
    let event = single_roll_event(&response);
    assert_eq!(event.rolled_value, Roll::Six);
    let total_bets = first_bet + second_bet;
    let expected_owed = calculate_payout(&payout_config, &Roll::Six, total_bets);
    let expected_house_pot = contract_chip_balance(&ctx).await;
    assert_eq!(event.roll_total_chips, total_bets);
    assert_eq!(event.chips_owed_total, expected_owed);
    assert_eq!(event.house_pot_total, expected_house_pot);
}

#[tokio::test]
async fn roll_dice__seven_clears_tracked_chip_totals() {
    let ctx = TestContext::new().await;
    let chip_asset_id = ctx.chip_asset_id();

    // given
    let stale_bet = 500;
    place_chip_bet(ctx.alice_instance(), chip_asset_id, Roll::Four, stale_bet).await;

    // when
    roll_with_logs(&ctx, roll_to_vrf_number(&Roll::Seven)).await;
    let response_after_clear =
        roll_with_logs(&ctx, roll_to_vrf_number(&Roll::Four)).await;

    // then
    let event = single_roll_event(&response_after_clear);
    assert_eq!(event.rolled_value, Roll::Four);
    assert_eq!(event.roll_total_chips, 0);
}

#[tokio::test]
async fn roll_dice__owed_total_persists_until_claim() {
    let ctx = TestContext::new().await;
    let chip_asset_id = ctx.chip_asset_id();
    let payout_config = ctx
        .owner_instance()
        .methods()
        .payouts()
        .simulate(Execution::state_read_only())
        .await
        .unwrap()
        .value;

    // given
    let bet_amount = 300;
    let winning_roll = Roll::Six;
    place_chip_bet(
        ctx.alice_instance(),
        chip_asset_id,
        winning_roll.clone(),
        bet_amount,
    )
    .await;
    let bet_game_id = ctx
        .owner_instance()
        .methods()
        .current_game_id()
        .simulate(Execution::state_read_only())
        .await
        .unwrap()
        .value;
    let expected_owed = calculate_payout(&payout_config, &winning_roll, bet_amount);

    // when
    let winning_event =
        single_roll_event(&roll_with_logs(&ctx, roll_to_vrf_number(&winning_roll)).await);

    // then
    assert_eq!(winning_event.chips_owed_total, expected_owed);

    // when
    let seven_event =
        single_roll_event(&roll_with_logs(&ctx, roll_to_vrf_number(&Roll::Seven)).await);
    let carry_event =
        single_roll_event(&roll_with_logs(&ctx, roll_to_vrf_number(&Roll::Eight)).await);

    // then
    assert_eq!(seven_event.chips_owed_total, expected_owed);
    assert_eq!(carry_event.chips_owed_total, expected_owed);

    // when
    ctx.alice_instance()
        .methods()
        .claim_rewards(bet_game_id, Vec::new())
        .with_variable_output_policy(VariableOutputPolicy::Exactly(1))
        .call()
        .await
        .unwrap();
    let post_claim_event =
        single_roll_event(&roll_with_logs(&ctx, roll_to_vrf_number(&Roll::Three)).await);

    // then
    assert_eq!(post_claim_event.chips_owed_total, 0);
}

async fn place_chip_bet(
    contract: strapped_types::MyContract<Wallet>,
    chip_asset_id: AssetId,
    roll: Roll,
    amount: u64,
) {
    contract
        .methods()
        .place_bet(roll, Bet::Chip, amount)
        .call_params(CallParameters::new(
            amount,
            chip_asset_id,
            CONTRACT_CALL_GAS_LIMIT,
        ))
        .unwrap()
        .call()
        .await
        .unwrap();
}

async fn roll_with_logs(ctx: &TestContext, vrf_number: u64) -> CallResponse<()> {
    if let Some(next_height) = ctx
        .owner_instance()
        .methods()
        .next_roll_height()
        .simulate(Execution::state_read_only())
        .await
        .unwrap()
        .value
    {
        ctx.advance_to_block_height(next_height).await;
    }

    ctx.vrf_instance()
        .methods()
        .set_number(vrf_number)
        .call()
        .await
        .unwrap();

    ctx.owner_instance()
        .methods()
        .roll_dice()
        .with_contracts(&[&ctx.vrf_instance()])
        .call()
        .await
        .unwrap()
}

fn single_roll_event(response: &CallResponse<()>) -> RollEvent {
    let events = response
        .decode_logs_with_type::<RollEvent>()
        .expect("roll event should decode");
    assert_eq!(events.len(), 1, "expected exactly one roll event");
    events.into_iter().next().unwrap()
}

async fn contract_chip_balance(ctx: &TestContext) -> u64 {
    ctx.owner()
        .provider()
        .get_contract_asset_balance(&ctx.contract_id(), &ctx.chip_asset_id())
        .await
        .unwrap()
}
