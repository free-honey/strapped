#![allow(non_snake_case)]

use crate::strapped_types::*;
use fuels::types::Bits256;
use fuels::{prelude::*, types::ContractId};

pub mod strapped_types {
    use fuels::macros::abigen;

    abigen!(Contract(
        name = "MyContract",
        abi = "strapped/out/debug/strapped-abi.json"
    ));
}

pub mod vrf_types {
    use fuels::macros::abigen;

    abigen!(Contract(
        name = "VRFContract",
        abi = "vrf-contract/out/debug/vrf-contract-abi.json"
    ));
}

async fn get_contract_instance(
    wallet: WalletUnlocked,
) -> (strapped_types::MyContract<WalletUnlocked>, ContractId) {
    let id = Contract::load_from(
        "strapped/out/debug/strapped.bin",
        LoadConfiguration::default(),
    )
    .unwrap()
    .deploy(&wallet, TxPolicies::default())
    .await
    .unwrap();

    let instance = strapped_types::MyContract::new(id.clone(), wallet);

    (instance, id.into())
}

async fn separate_contract_instance(
    id: &ContractId,
    wallet: WalletUnlocked,
) -> strapped_types::MyContract<WalletUnlocked> {
    strapped_types::MyContract::new(id.clone(), wallet)
}

async fn get_vrf_contract_instance(
    wallet: WalletUnlocked,
) -> (vrf_types::VRFContract<WalletUnlocked>, ContractId) {
    let id = Contract::load_from(
        "vrf-contract/out/debug/vrf-contract.bin",
        LoadConfiguration::default(),
    )
    .unwrap()
    .deploy(&wallet, TxPolicies::default())
    .await
    .unwrap();

    let instance = vrf_types::VRFContract::new(id.clone(), wallet);

    (instance, id.into())
}

async fn get_wallet() -> WalletUnlocked {
    // Launch a local network and deploy the contract
    let mut wallets = launch_custom_provider_and_get_wallets(
        WalletsConfig::new(
            Some(1),             /* Single wallet */
            Some(1),             /* Single coin (UTXO) */
            Some(1_000_000_000), /* Amount per coin */
        ),
        None,
        None,
    )
    .await
    .unwrap();
    wallets.pop().unwrap()
}

struct TestContext {
    alice: WalletUnlocked,
    // bob: WalletUnlocked,
    owner: WalletUnlocked,
}

impl TestContext {
    async fn new() -> Self {
        let mut wallets = launch_custom_provider_and_get_wallets(
            WalletsConfig::new_multiple_assets(
                3, /* Three wallets */
                vec![
                    AssetConfig {
                        id: AssetId::zeroed(),
                        num_coins: 1,               /* Single coin (UTXO) */
                        coin_amount: 1_000_000_000, /* Amount per coin */
                    },
                    AssetConfig {
                        id: AssetId::from([1u8; 32]),
                        num_coins: 1,               /* Single coin (UTXO) */
                        coin_amount: 1_000_000_000, /* Amount per coin */
                    },
                ],
            ),
            None,
            None,
        )
        .await
        .unwrap();
        let owner = wallets.pop().unwrap();
        // let bob = wallets.pop().unwrap();
        let alice = wallets.pop().unwrap();
        Self { alice, owner }
    }

    pub fn alice(&self) -> WalletUnlocked {
        self.alice.clone()
    }

    // pub fn bob(&self) -> WalletUnlocked {
    //     self.bob.clone()
    // }

    pub fn owner(&self) -> WalletUnlocked {
        self.owner.clone()
    }
}

#[tokio::test]
async fn can_get_contract_id() {
    let wallet = get_wallet().await;
    let (_instance, _id) = get_contract_instance(wallet).await;
}

#[tokio::test]
async fn roll_dice__adds_roll_to_roll_history() {
    let ctx = TestContext::new().await;
    let owner = ctx.owner();
    // given
    let (instance, _id) = get_contract_instance(owner.clone()).await;
    let (vrf_instance, vrf_id) = get_vrf_contract_instance(owner).await;
    let first_number = 10;
    let second_number = 34;
    instance
        .methods()
        .set_vrf_contract_id(Bits256(*vrf_id))
        .call()
        .await
        .unwrap();
    vrf_instance
        .methods()
        .set_number(first_number)
        .call()
        .await
        .unwrap();

    // when
    instance
        .methods()
        .roll_dice()
        .with_contracts(&[&vrf_instance])
        .call()
        .await
        .unwrap();
    // update vrf
    vrf_instance
        .methods()
        .set_number(second_number)
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .roll_dice()
        .with_contracts(&[&vrf_instance])
        .call()
        .await
        .unwrap();

    // then
    let actual = instance
        .methods()
        .roll_history()
        .call()
        .await
        .unwrap()
        .value;
    // 0-35
    // where 35 is Twelve and 0 is Two
    // 10 % 36
    // so 10 is Six
    // 34 % 36
    // so 34 is Eleven
    let expected = vec![Roll::Six, Roll::Eleven];
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn place_bet__adds_bets_to_list() {
    let asset_id = AssetId::new([1; 32]);
    let ctx = TestContext::new().await;
    let alice = ctx.alice();
    // given
    let (instance, _id) = get_contract_instance(alice.clone()).await;
    let bet_amount = 100;
    let bet = strapped_types::Bet::Chip;
    let roll = strapped_types::Roll::Six;
    instance
        .methods()
        .set_chip_asset_id(asset_id)
        .call()
        .await
        .unwrap();

    // when
    let call_params = CallParameters::new(bet_amount, asset_id, 1_000_000);
    instance
        .methods()
        .place_bet(roll.clone(), bet.clone(), bet_amount)
        .call_params(call_params)
        .unwrap()
        .call()
        .await
        .unwrap();

    // then
    let actual = instance
        .methods()
        .get_my_bets(roll)
        .call()
        .await
        .unwrap()
        .value;
    let expected = vec![(bet, bet_amount, 0)];
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn place_bet__fails_if_funds_not_transferred() {
    let ctx = TestContext::new().await;
    let alice = ctx.alice();
    // given
    let (instance, _id) = get_contract_instance(alice.clone()).await;
    let bet_amount = 100;
    let bet = strapped_types::Bet::Chip;
    let roll = strapped_types::Roll::Six;

    // when
    let result = instance
        .methods()
        .place_bet(roll.clone(), bet.clone(), bet_amount)
        .call()
        .await;

    // then
    assert!(result.is_err());
}

#[tokio::test]
async fn roll_dice__if_seven_rolled_move_to_next_game() {
    let ctx = TestContext::new().await;
    let owner = ctx.owner();
    // given
    let (instance, _id) = get_contract_instance(owner.clone()).await;
    let (vrf_instance, vrf_id) = get_vrf_contract_instance(owner).await;
    let first_number = 10;
    let second_number = 34;
    instance
        .methods()
        .set_vrf_contract_id(Bits256(*vrf_id))
        .call()
        .await
        .unwrap();
    // update vrf
    vrf_instance
        .methods()
        .set_number(first_number)
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .roll_dice()
        .with_contracts(&[&vrf_instance])
        .call()
        .await
        .unwrap();
    // update vrf
    vrf_instance
        .methods()
        .set_number(second_number)
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .roll_dice()
        .with_contracts(&[&vrf_instance])
        .call()
        .await
        .unwrap();
    // update vrf to something that will resolve to Seven
    let third_number = 19; // 22 % 36 = 22 which is Seven
    vrf_instance
        .methods()
        .set_number(third_number)
        .call()
        .await
        .unwrap();

    // when
    instance
        .methods()
        .roll_dice()
        .with_contracts(&[&vrf_instance])
        .call()
        .await
        .unwrap();

    // then
    let actual = instance
        .methods()
        .roll_history()
        .call()
        .await
        .unwrap()
        .value;
    let expected: Vec<Roll> = Vec::new();
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn roll_dice__if_seven_adds_new_strap_reward() {
    let ctx = TestContext::new().await;
    let owner = ctx.owner();
    // given
    let (instance, _id) = get_contract_instance(owner.clone()).await;
    let (vrf_instance, vrf_id) = get_vrf_contract_instance(owner).await;
    instance
        .methods()
        .set_vrf_contract_id(Bits256(*vrf_id))
        .call()
        .await
        .unwrap();
    // update vrf to something that will resolve to Seven
    let seven_vrf_number = 19; // 22 % 36 = 22 which is Seven
    vrf_instance
        .methods()
        .set_number(seven_vrf_number)
        .call()
        .await
        .unwrap();

    // when
    instance
        .methods()
        .roll_dice()
        .with_contracts(&[&vrf_instance])
        .call()
        .await
        .unwrap();

    // then
    let actual = instance
        .methods()
        .strap_rewards()
        .call()
        .await
        .unwrap()
        .value;
    let expected_roll = Roll::Eight;
    let expected_strap = Strap {
        level: 1,
        kind: StrapKind::Shirt,
        modifier: Modifier::Nothing,
    };
    let expected = vec![(expected_roll, expected_strap)];
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn claim_rewards__adds_chips_to_wallet() {
    let ctx = TestContext::new().await;
    let owner = ctx.owner();
    let chip_asset_id = AssetId::new([1; 32]);

    // given
    // init contracts
    let (instance, contract_id) = get_contract_instance(owner.clone()).await;
    let alice_instance = separate_contract_instance(&contract_id, ctx.alice()).await;
    let (vrf_instance, vrf_contract_id) = get_vrf_contract_instance(owner).await;
    instance
        .methods()
        .set_vrf_contract_id(Bits256(*vrf_contract_id))
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .set_chip_asset_id(chip_asset_id)
        .call()
        .await
        .unwrap();

    // fund contract with chips
    let call_params = CallParameters::new(1_000_000, chip_asset_id, 1_000_000);
    instance
        .methods()
        .fund()
        .call_params(call_params)
        .unwrap()
        .call()
        .await
        .unwrap();

    // place bet
    let bet_amount = 100;
    let bet = strapped_types::Bet::Chip;
    let roll = Roll::Six;
    let call_params = CallParameters::new(bet_amount, chip_asset_id, 1_000_000);
    alice_instance
        .methods()
        .place_bet(roll.clone(), bet.clone(), bet_amount)
        .call_params(call_params)
        .unwrap()
        .call()
        .await
        .unwrap();

    let bet_game_id = alice_instance
        .methods()
        .current_game_id()
        .call()
        .await
        .unwrap()
        .value;

    // roll the correct number
    let first_number = 10; // 10 % 36 = 10 which is Six
    vrf_instance
        .methods()
        .set_number(first_number)
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .roll_dice()
        .with_contracts(&[&vrf_instance])
        .call()
        .await
        .unwrap();

    // roll seven
    let seven_vrf_number = 19; // 22 % 36 = 22 which is Seven
    vrf_instance
        .methods()
        .set_number(seven_vrf_number)
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .roll_dice()
        .with_contracts(&[&vrf_instance])
        .call()
        .await
        .unwrap();

    // when

    // claim reward
    let wallet_balance = ctx.alice().get_asset_balance(&chip_asset_id).await.unwrap();
    alice_instance
        .methods()
        .claim_rewards(bet_game_id)
        .with_variable_output_policy(VariableOutputPolicy::Exactly(1))
        .call()
        .await
        .unwrap();

    // then

    // Six pays 2:1 for each roll
    let expected = wallet_balance + bet_amount * 2 - bet_amount;
    let actual = ctx.alice().get_asset_balance(&chip_asset_id).await.unwrap();
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn claim_rewards__cannot_claim_rewards_for_current_game() {
    let ctx = TestContext::new().await;
    let owner = ctx.owner();
    let chip_asset_id = AssetId::new([1; 32]);

    // given
    // init contracts
    let (instance, contract_id) = get_contract_instance(owner.clone()).await;
    let alice_instance = separate_contract_instance(&contract_id, ctx.alice()).await;
    let (vrf_instance, vrf_contract_id) = get_vrf_contract_instance(owner).await;
    instance
        .methods()
        .set_vrf_contract_id(Bits256(*vrf_contract_id))
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .set_chip_asset_id(chip_asset_id)
        .call()
        .await
        .unwrap();

    // place bet
    let bet_amount = 100;
    let bet = strapped_types::Bet::Chip;
    let roll = Roll::Six;
    let call_params = CallParameters::new(bet_amount, chip_asset_id, 1_000_000);
    alice_instance
        .methods()
        .place_bet(roll.clone(), bet.clone(), bet_amount)
        .call_params(call_params)
        .unwrap()
        .call()
        .await
        .unwrap();
    vrf_instance
        .methods()
        .set_number(10) // 10 % 36 = 10 which is Six
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .roll_dice()
        .with_contracts(&[&vrf_instance])
        .call()
        .await
        .unwrap();

    let bet_game_id = alice_instance
        .methods()
        .current_game_id()
        .call()
        .await
        .unwrap()
        .value;

    // when
    let result = instance.methods().claim_rewards(bet_game_id).call().await;

    // then
    assert!(result.is_err());
}

#[tokio::test]
async fn claim_rewards__do_not_reward_bets_placed_after_roll() {
    let ctx = TestContext::new().await;
    let owner = ctx.owner();
    let chip_asset_id = AssetId::new([1; 32]);

    // given
    // init contracts
    let (instance, contract_id) = get_contract_instance(owner.clone()).await;
    let alice_instance = separate_contract_instance(&contract_id, ctx.alice()).await;
    let (vrf_instance, vrf_contract_id) = get_vrf_contract_instance(owner).await;
    instance
        .methods()
        .set_vrf_contract_id(Bits256(*vrf_contract_id))
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .set_chip_asset_id(chip_asset_id)
        .call()
        .await
        .unwrap();

    // fund contract with chips
    let call_params = CallParameters::new(1_000_000, chip_asset_id, 1_000_000);
    instance
        .methods()
        .fund()
        .call_params(call_params)
        .unwrap()
        .call()
        .await
        .unwrap();

    // roll the correct number
    let first_number = 10; // 10 % 36 = 10 which is Six
    vrf_instance
        .methods()
        .set_number(first_number)
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .roll_dice()
        .with_contracts(&[&vrf_instance])
        .call()
        .await
        .unwrap();

    // place bet
    let bet_amount = 100;
    let bet = strapped_types::Bet::Chip;
    let roll = Roll::Six;
    let call_params = CallParameters::new(bet_amount, chip_asset_id, 1_000_000);
    alice_instance
        .methods()
        .place_bet(roll.clone(), bet.clone(), bet_amount)
        .call_params(call_params)
        .unwrap()
        .call()
        .await
        .unwrap();

    let bet_game_id = alice_instance
        .methods()
        .current_game_id()
        .call()
        .await
        .unwrap()
        .value;

    // roll seven
    let seven_vrf_number = 19; // 22 % 36 = 22 which is Seven
    vrf_instance
        .methods()
        .set_number(seven_vrf_number)
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .roll_dice()
        .with_contracts(&[&vrf_instance])
        .call()
        .await
        .unwrap();

    // when

    // claim reward
    let wallet_balance = ctx.alice().get_asset_balance(&chip_asset_id).await.unwrap();
    alice_instance
        .methods()
        .claim_rewards(bet_game_id)
        .with_variable_output_policy(VariableOutputPolicy::Exactly(1))
        .call()
        .await
        .unwrap_err();

    // then
    let expected = wallet_balance;
    let actual = ctx.alice().get_asset_balance(&chip_asset_id).await.unwrap();
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn claim_rewards__cannot_claim_rewards_twice() {
    let ctx = TestContext::new().await;
    let owner = ctx.owner();
    let chip_asset_id = AssetId::new([1; 32]);

    // given
    // init contracts
    let (instance, contract_id) = get_contract_instance(owner.clone()).await;
    let alice_instance = separate_contract_instance(&contract_id, ctx.alice()).await;
    let (vrf_instance, vrf_contract_id) = get_vrf_contract_instance(owner).await;
    instance
        .methods()
        .set_vrf_contract_id(Bits256(*vrf_contract_id))
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .set_chip_asset_id(chip_asset_id)
        .call()
        .await
        .unwrap();

    // fund contract with chips
    let call_params = CallParameters::new(1_000_000, chip_asset_id, 1_000_000);
    instance
        .methods()
        .fund()
        .call_params(call_params)
        .unwrap()
        .call()
        .await
        .unwrap();

    // place bet
    let bet_amount = 100;
    let bet = strapped_types::Bet::Chip;
    let roll = Roll::Six;
    let call_params = CallParameters::new(bet_amount, chip_asset_id, 1_000_000);
    alice_instance
        .methods()
        .place_bet(roll.clone(), bet.clone(), bet_amount)
        .call_params(call_params)
        .unwrap()
        .call()
        .await
        .unwrap();

    let bet_game_id = alice_instance
        .methods()
        .current_game_id()
        .call()
        .await
        .unwrap()
        .value;

    // roll the correct number
    let first_number = 10; // 10 % 36 = 10 which is Six
    vrf_instance
        .methods()
        .set_number(first_number)
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .roll_dice()
        .with_contracts(&[&vrf_instance])
        .call()
        .await
        .unwrap();

    // roll seven
    let seven_vrf_number = 19; // 22 % 36 = 22 which is Seven
    vrf_instance
        .methods()
        .set_number(seven_vrf_number)
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .roll_dice()
        .with_contracts(&[&vrf_instance])
        .call()
        .await
        .unwrap();

    // claim reward
    alice_instance
        .methods()
        .claim_rewards(bet_game_id)
        .with_variable_output_policy(VariableOutputPolicy::Exactly(1))
        .call()
        .await
        .unwrap();

    // when
    let wallet_balance = ctx.alice().get_asset_balance(&chip_asset_id).await.unwrap();
    // try claiming again
    alice_instance
        .methods()
        .claim_rewards(bet_game_id)
        .with_variable_output_policy(VariableOutputPolicy::Exactly(1))
        .call()
        .await
        .unwrap_err();

    let expected = wallet_balance;
    let actual = ctx.alice().get_asset_balance(&chip_asset_id).await.unwrap();
    assert_eq!(expected, actual);
}
