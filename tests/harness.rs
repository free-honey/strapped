#![allow(non_snake_case)]

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
    bob: WalletUnlocked,
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
        let bob = wallets.pop().unwrap();
        let alice = wallets.pop().unwrap();
        Self { alice, bob, owner }
    }

    pub fn alice(&self) -> WalletUnlocked {
        self.alice.clone()
    }

    pub fn bob(&self) -> WalletUnlocked {
        self.bob.clone()
    }

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
async fn roll_dice__changes_the_last_roll() {
    let ctx = TestContext::new().await;
    let owner = ctx.owner();
    // given
    let (instance, _id) = get_contract_instance(owner.clone()).await;
    let (vrf_instance, vrf_id) = get_vrf_contract_instance(owner).await;
    let number = 34;
    vrf_instance
        .methods()
        .set_number(number)
        .call()
        .await
        .unwrap();
    instance
        .methods()
        .set_vrf_contract_id(Bits256(*vrf_id))
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
    let actual = instance.methods().last_roll().call().await.unwrap().value;
    // 34 % 36
    // where 35 is 12 and 0 is 2
    // so 34 is 11
    let expected = strapped_types::Roll::Eleven;
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
    let expected = vec![(bet, bet_amount)];
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
