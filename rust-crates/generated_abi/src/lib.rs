use std::path::{
    Path,
    PathBuf,
};

use fuels::{
    accounts::wallet::Wallet,
    macros::abigen,
    programs::contract::Contract,
    types::{
        Bytes32,
        ContractId,
        SubAssetId,
    },
};

pub mod strapped_types {
    use super::*;

    abigen!(Contract(
        name = "MyContract",
        abi = "sway-projects/strapped/out/release/strapped-abi.json"
    ));
}

pub mod vrf_types {
    use super::*;

    abigen!(Contract(
        name = "FakeVRFContract",
        abi = "sway-projects/fake-vrf-contract/out/release/fake-vrf-contract-abi.json"
    ));
}

pub mod pseudo_vrf_types {
    use super::*;

    abigen!(Contract(
        name = "PseudoVRFContract",
        abi =
            "sway-projects/pseudo-vrf-contract/out/release/pseudo-vrf-contract-abi.json"
    ));
}

#[cfg(feature = "test-helpers")]
pub mod test_helpers;

pub(crate) fn manifest_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn strapped_bin_path() -> PathBuf {
    manifest_path("../../sway-projects/strapped/out/release/strapped.bin")
}

pub fn contract_id() -> ContractId {
    Contract::load_from(strapped_bin_path(), Default::default())
        .expect("failed to load strapped contract binary")
        .contract_id()
}

pub async fn get_contract_instance(
    wallet: Wallet,
) -> (strapped_types::MyContract<Wallet>, ContractId) {
    let contract = Contract::load_from(strapped_bin_path(), Default::default())
        .expect("failed to load strapped contract binary");
    let res = contract
        .deploy(&wallet, Default::default())
        .await
        .expect("failed to deploy strapped contract");

    let contract_id = res.contract_id;
    let instance = strapped_types::MyContract::new(contract_id.clone(), wallet);

    (instance, contract_id)
}

pub async fn separate_contract_instance(
    id: &ContractId,
    wallet: Wallet,
) -> strapped_types::MyContract<Wallet> {
    strapped_types::MyContract::new(*id, wallet)
}

pub fn strap_to_sub_id(strap: &strapped_types::Strap) -> SubAssetId {
    use strapped_types::{
        Modifier,
        StrapKind,
    };

    let level_bytes = strap.level;
    let kind_bytes = match strap.kind {
        StrapKind::Shirt => 0u8,
        StrapKind::Pants => 1u8,
        StrapKind::Shoes => 2u8,
        StrapKind::Dress => 3u8,
        StrapKind::Hat => 4u8,
        StrapKind::Glasses => 5u8,
        StrapKind::Watch => 6u8,
        StrapKind::Ring => 7u8,
        StrapKind::Necklace => 8u8,
        StrapKind::Earring => 9u8,
        StrapKind::Bracelet => 10u8,
        StrapKind::Tattoo => 11u8,
        StrapKind::Skirt => 12u8,
        StrapKind::Piercing => 13u8,
        StrapKind::Coat => 14u8,
        StrapKind::Scarf => 15u8,
        StrapKind::Gloves => 16u8,
        StrapKind::Gown => 17u8,
        StrapKind::Belt => 18u8,
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
        Modifier::Groovy => 10u8,
        Modifier::Delicate => 11u8,
    };
    let mut sub_id = [0u8; 32];
    sub_id[0] = level_bytes;
    sub_id[1] = kind_bytes;
    sub_id[2] = modifier_bytes;
    SubAssetId::from(sub_id)
}
