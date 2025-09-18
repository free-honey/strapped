#![allow(non_snake_case)]
use strapped_contract::{
    get_contract_instance,
    test_helpers::*,
};

#[tokio::test]
async fn can_get_contract_id() {
    let wallet = get_wallet().await;
    let (_instance, _id) = get_contract_instance(wallet).await;
}
