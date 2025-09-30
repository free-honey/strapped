use crate::{
    get_contract_instance,
    separate_contract_instance,
    strapped_types,
    strapped_types::{
        Modifier,
        Roll,
        Strap,
        StrapKind,
    },
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
pub fn generate_straps(seed: u64) -> Vec<(Roll, Strap)> {
    let mut straps: Vec<(Roll, Strap)> = Vec::new();
    let mut multiple = 1;
    while seed % multiple == 0 && seed != 0 {
        let inner = seed / multiple;
        let strap = u64_to_strap(inner);
        let slot = u64_to_slot(inner);
        straps.push((slot, strap));
        multiple = multiple * 2;
    }
    straps
}

pub fn u64_to_strap(num: u64) -> Strap {
    let level = 1;
    let modifier = Modifier::Nothing;
    let modulo = num % 141;
    let kind = if modulo < 20 {
        StrapKind::Shirt // weight 20
    } else if modulo < 40 {
        StrapKind::Pants // weight 20
    } else if modulo < 60 {
        StrapKind::Shoes // weight 20
    } else if modulo < 70 {
        StrapKind::Hat // weight 10
    } else if modulo < 80 {
        StrapKind::Glasses // weight 10
    } else if modulo < 90 {
        StrapKind::Watch // weight 10
    } else if modulo < 100 {
        StrapKind::Ring // weight 10
    } else if modulo < 105 {
        StrapKind::Necklace // weight 5
    } else if modulo < 110 {
        StrapKind::Earring // weight 5
    } else if modulo < 115 {
        StrapKind::Bracelet // weight 5
    } else if modulo < 120 {
        StrapKind::Tattoo // weight 5
    } else if modulo < 125 {
        StrapKind::Skirt // weight 5
    } else if modulo < 130 {
        StrapKind::Piercing // weight 5
    } else if modulo < 135 {
        StrapKind::Coat // weight 5
    } else if modulo < 137 {
        StrapKind::Scarf // weight 5
    } else if modulo < 139 {
        StrapKind::Gloves // weight 2
    } else if modulo < 141 {
        StrapKind::Gown // weight 2
    } else {
        StrapKind::Belt // weight 1
    };
    Strap::new(level, kind, modifier)
}
// two -> twelve, never seven
pub fn u64_to_slot(num: u64) -> Roll {
    let modulo = num % 10;

    match modulo {
        0 => Roll::Two,
        1 => Roll::Three,
        2 => Roll::Four,
        3 => Roll::Five,
        4 => Roll::Six,
        5 => Roll::Eight,
        6 => Roll::Nine,
        7 => Roll::Ten,
        8 => Roll::Eleven,
        _ => Roll::Twelve,
    }
}

pub fn roll_to_vrf_number(roll: &Roll) -> u64 {
    match roll {
        Roll::Two => 0,
        Roll::Three => 1,
        Roll::Four => 3,
        Roll::Five => 6,
        Roll::Six => 10,
        Roll::Seven => 15,
        Roll::Eight => 21,
        Roll::Nine => 26,
        Roll::Ten => 30,
        Roll::Eleven => 33,
        Roll::Twelve => 35,
    }
}

pub fn modifier_triggers_for_roll(roll: u64) -> Vec<(Roll, Roll, Modifier)> {
    let mut triggers = Vec::new();
    let mut multiple = 1;
    while roll % multiple == 0 && roll != 0 {
        let inner = roll / multiple;
        let (trigger_roll, modifier) = u64_to_modifier(inner);
        let activated_roll = u64_to_trigger_roll(inner);
        triggers.push((trigger_roll, activated_roll, modifier));
        multiple = multiple * 3;
    }
    triggers
}

pub fn u64_to_modifier(num: u64) -> (Roll, Modifier) {
    let modulo = num % 10;

    match modulo {
        0 => (Roll::Two, Modifier::Burnt),
        1 => (Roll::Three, Modifier::Lucky),
        2 => (Roll::Four, Modifier::Holy),
        3 => (Roll::Five, Modifier::Holey),
        4 => (Roll::Six, Modifier::Scotch),
        8 => (Roll::Seven, Modifier::Evil),
        5 => (Roll::Eight, Modifier::Soaked),
        6 => (Roll::Nine, Modifier::Moldy),
        7 => (Roll::Ten, Modifier::Starched),
        8 => (Roll::Eleven, Modifier::Groovy),
        _ => (Roll::Twelve, Modifier::Delicate),
    }
}

pub fn u64_to_trigger_roll(num: u64) -> Roll {
    let modulo = num % 4;

    match modulo {
        0 => Roll::Two,
        1 => Roll::Three,
        2 => Roll::Eleven,
        _ => Roll::Twelve,
    }
}
