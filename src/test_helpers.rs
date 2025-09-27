use crate::vrf_types;
use fuels::prelude::{
    AssetConfig,
    AssetId,
    Contract,
    ContractId,
    LoadConfiguration,
    TxPolicies,
    WalletUnlocked,
    WalletsConfig,
    launch_custom_provider_and_get_wallets,
};

pub async fn get_vrf_contract_instance(
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

pub struct TestContext {
    alice: WalletUnlocked,
    owner: WalletUnlocked,
}

impl TestContext {
    pub async fn new() -> Self {
        Self::new_with_extra_assets(vec![]).await
    }

    pub async fn new_with_extra_assets(extra_assets: Vec<AssetConfig>) -> Self {
        let mut base_assets = vec![
            AssetConfig {
                id: AssetId::zeroed(),
                num_coins: 1,               // Single coin (UTXO)
                coin_amount: 1_000_000_000, // Amount per coin
            },
            AssetConfig {
                id: AssetId::from([1u8; 32]),
                num_coins: 1,               // Single coin (UTXO)
                coin_amount: 1_000_000_000, // Amount per coin
            },
        ];
        base_assets.extend(extra_assets);
        let mut wallets = launch_custom_provider_and_get_wallets(
            WalletsConfig::new_multiple_assets(3 /* Three wallets */, base_assets),
            None,
            None,
        )
        .await
        .unwrap();
        let owner = wallets.pop().unwrap();
        let alice = wallets.pop().unwrap();
        Self { alice, owner }
    }

    pub fn alice(&self) -> WalletUnlocked {
        self.alice.clone()
    }

    pub fn owner(&self) -> WalletUnlocked {
        self.owner.clone()
    }

    pub async fn advance_to_block_height(&self, height: u32) {
        let provider = self.owner.provider().unwrap();
        let current_height = provider.latest_block_height().await.unwrap();
        let blocks_to_advance = height.saturating_sub(current_height);
        provider
            .produce_blocks(blocks_to_advance, None)
            .await
            .unwrap();
    }
}

pub async fn get_wallet() -> WalletUnlocked {
    // Launch a local network and deploy the contract
    let mut wallets = launch_custom_provider_and_get_wallets(
        WalletsConfig::new(
            Some(1),             // Single wallet
            Some(1),             // Single coin (UTXO)
            Some(1_000_000_000), // Amount per coin
        ),
        None,
        None,
    )
    .await
    .unwrap();
    wallets.pop().unwrap()
}
