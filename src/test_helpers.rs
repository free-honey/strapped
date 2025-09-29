use crate::{
    get_contract_instance,
    separate_contract_instance,
    strapped_types,
    vrf_types,
};
use fuels::{
    prelude::{
        AssetConfig,
        AssetId,
        CallParameters,
        Contract,
        ContractId,
        Execution,
        LoadConfiguration,
        TxPolicies,
        WalletUnlocked,
        WalletsConfig,
        launch_custom_provider_and_get_wallets,
    },
    types::Bits256,
};
use strapped_types::MyContract;
use vrf_types::FakeVRFContract;

const CHIP_ASSET_BYTES: [u8; 32] = [1u8; 32];
const DEFAULT_ROLL_FREQUENCY: u32 = 10;
const DEFAULT_FUND_AMOUNT: u64 = 1_000_000;

pub async fn get_vrf_contract_instance(
    wallet: WalletUnlocked,
) -> (FakeVRFContract<WalletUnlocked>, ContractId) {
    let id = Contract::load_from(
        "fake-vrf-contract/out/debug/fake-vrf-contract.bin",
        LoadConfiguration::default(),
    )
    .unwrap()
    .deploy(&wallet, TxPolicies::default())
    .await
    .unwrap();

    let instance = FakeVRFContract::new(id.clone(), wallet);

    (instance, id.into())
}

pub struct TestContext {
    alice: WalletUnlocked,
    owner: WalletUnlocked,
    contract_id: ContractId,
    chip_asset_id: AssetId,
    owner_instance: MyContract<WalletUnlocked>,
    alice_instance: MyContract<WalletUnlocked>,
    vrf_instance: FakeVRFContract<WalletUnlocked>,
}

impl TestContext {
    pub async fn new() -> Self {
        Self::new_with_extra_assets(vec![]).await
    }

    pub async fn new_with_extra_assets(extra_assets: Vec<AssetConfig>) -> Self {
        let chip_asset_id = AssetId::from(CHIP_ASSET_BYTES);
        let mut base_assets = vec![
            AssetConfig {
                id: AssetId::zeroed(),
                num_coins: 1,
                coin_amount: 1_000_000_000,
            },
            AssetConfig {
                id: chip_asset_id,
                num_coins: 1,
                coin_amount: 1_000_000_000,
            },
        ];
        base_assets.extend(extra_assets);
        let mut wallets = launch_custom_provider_and_get_wallets(
            WalletsConfig::new_multiple_assets(3, base_assets),
            None,
            None,
        )
        .await
        .unwrap();

        let owner = wallets.pop().unwrap();
        let alice = wallets.pop().unwrap();

        let (owner_instance, contract_id) = get_contract_instance(owner.clone()).await;
        let alice_instance =
            separate_contract_instance(&contract_id, alice.clone()).await;
        let (vrf_instance, vrf_contract_id) =
            get_vrf_contract_instance(owner.clone()).await;

        owner_instance
            .methods()
            .initialize(
                Bits256(*vrf_contract_id),
                chip_asset_id,
                DEFAULT_ROLL_FREQUENCY,
            )
            .call()
            .await
            .unwrap();

        owner_instance
            .methods()
            .fund()
            .call_params(CallParameters::new(
                DEFAULT_FUND_AMOUNT,
                chip_asset_id,
                1_000_000,
            ))
            .unwrap()
            .call()
            .await
            .unwrap();

        Self {
            alice,
            owner,
            contract_id,
            chip_asset_id,
            owner_instance,
            alice_instance,
            vrf_instance,
        }
    }

    pub fn alice(&self) -> WalletUnlocked {
        self.alice.clone()
    }

    pub fn owner(&self) -> WalletUnlocked {
        self.owner.clone()
    }

    pub fn contract_id(&self) -> ContractId {
        self.contract_id
    }

    pub fn chip_asset_id(&self) -> AssetId {
        self.chip_asset_id
    }

    pub fn owner_contract(&self) -> MyContract<WalletUnlocked> {
        self.owner_instance.clone()
    }

    pub fn alice_contract(&self) -> MyContract<WalletUnlocked> {
        self.alice_instance.clone()
    }

    pub fn vrf_contract(&self) -> FakeVRFContract<WalletUnlocked> {
        self.vrf_instance.clone()
    }

    pub fn owner_instance(&self) -> MyContract<WalletUnlocked> {
        self.owner_contract()
    }

    pub fn alice_instance(&self) -> MyContract<WalletUnlocked> {
        self.alice_contract()
    }

    pub fn vrf_instance(&self) -> FakeVRFContract<WalletUnlocked> {
        self.vrf_contract()
    }

    pub async fn advance_and_roll(&self, vrf_number: u64) {
        if let Some(next_height) = self
            .owner_instance
            .methods()
            .next_roll_height()
            .simulate(Execution::StateReadOnly)
            .await
            .unwrap()
            .value
        {
            self.advance_to_block_height(next_height).await;
        }

        self.vrf_instance
            .methods()
            .set_number(vrf_number)
            .call()
            .await
            .unwrap();

        self.owner_instance
            .methods()
            .roll_dice()
            .with_contracts(&[&self.vrf_instance])
            .call()
            .await
            .unwrap();
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
    let mut wallets = launch_custom_provider_and_get_wallets(
        WalletsConfig::new(Some(1), Some(1), Some(1_000_000_000)),
        None,
        None,
    )
    .await
    .unwrap();
    wallets.pop().unwrap()
}
