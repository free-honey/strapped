use crate::strapped_types::{
    Modifier,
    Strap,
    StrapKind,
};
use fuels::{
    prelude::{
        Contract,
        ContractId,
        LoadConfiguration,
        TxPolicies,
        WalletUnlocked,
    },
    types::Bytes32,
};

#[cfg(any(test, feature = "test-helpers"))]
pub mod test_helpers;

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

pub async fn get_contract_instance(
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

pub async fn separate_contract_instance(
    id: &ContractId,
    wallet: WalletUnlocked,
) -> strapped_types::MyContract<WalletUnlocked> {
    strapped_types::MyContract::new(id.clone(), wallet)
}

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
