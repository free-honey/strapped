#![allow(non_snake_case)]

use crate::strapped_types::*;
use fuels::tx::ContractIdExt;
use fuels::types::{Bits256, Bytes32};
use fuels::{prelude::*, types::ContractId};

pub fn strap_to_sub_id(strap: &Strap) -> Bytes32 {
    let level_bytes = strap.level;
    let kind_bytes = match strap.kind {
        StrapKind::Shirt => 0u8,
        StrapKind::Pants => 1u8,
        StrapKind::Shoes => 2u8,
        StrapKind::Hat => 3u8,
        StrapKind::Glasses => 4u8,
        StrapKind::Watch => 5u8,
        StrapKind::Ring => 6u8,
        StrapKind::Necklace => 7u8,
        StrapKind::Earring => 8u8,
        StrapKind::Bracelet => 9u8,
        StrapKind::Tattoo => 10u8,
        StrapKind::Piercing => 11u8,
        StrapKind::Coat => 12u8,
        StrapKind::Scarf => 13u8,
        StrapKind::Gloves => 14u8,
        StrapKind::Belt => 15u8,
    };
    let modifier_bytes = match strap.modifier {
        Modifier::Nothing => 0u8,
        Modifier::Burnt => 1u8,
        Modifier::Lucky => 2u8,
        Modifier::Holy => 3u8,
        Modifier::Holey => 4u8,
        Modifier::Scotch => 5u8,
        Modifier::Soaked => 6u8,
        Modifier::Moldy => 7u8,
        Modifier::Starched => 8u8,
        Modifier::Evil => 9u8,
    };
    let mut sub_id = [0u8; 32];
    sub_id[0] = level_bytes;
    sub_id[1] = kind_bytes;
    sub_id[2] = modifier_bytes;
    Bytes32::from(sub_id)
}

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

mod claim_rewards;
mod place_bet;
mod roll_dice;
