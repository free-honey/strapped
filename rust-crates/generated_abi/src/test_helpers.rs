use fuels::{
    accounts::wallet::Wallet,
    prelude::{
        AssetConfig,
        AssetId,
        CallParameters,
        Contract,
        ContractId,
        LoadConfiguration,
        TxPolicies,
        WalletsConfig,
        launch_custom_provider_and_get_wallets,
    },
    programs::calls::Execution,
    types::Bits256,
};

use crate::{
    get_contract_instance,
    separate_contract_instance,
    strapped_types::{
        self,
        Modifier,
        Roll,
        Strap,
        StrapKind,
    },
    vrf_types,
};

pub fn calculate_payout(
    cfg: &strapped_types::PayoutConfig,
    roll: &Roll,
    principal: u64,
) -> u64 {
    let numerator = payout_numerator(cfg, roll);
    let denominator = payout_denominator(cfg, roll);
    (principal / denominator) * numerator
}
pub fn payout_numerator(cfg: &strapped_types::PayoutConfig, roll: &Roll) -> u64 {
    match roll {
        Roll::Two => cfg.two_payout_multiplier.0,
        Roll::Three => cfg.three_payout_multiplier.0,
        Roll::Four => cfg.four_payout_multiplier.0,
        Roll::Five => cfg.five_payout_multiplier.0,
        Roll::Six => cfg.six_payout_multiplier.0,
        Roll::Seven => cfg.seven_payout_multiplier.0,
        Roll::Eight => cfg.eight_payout_multiplier.0,
        Roll::Nine => cfg.nine_payout_multiplier.0,
        Roll::Ten => cfg.ten_payout_multiplier.0,
        Roll::Eleven => cfg.eleven_payout_multiplier.0,
        Roll::Twelve => cfg.twelve_payout_multiplier.0,
    }
}

pub fn payout_denominator(cfg: &strapped_types::PayoutConfig, roll: &Roll) -> u64 {
    match roll {
        Roll::Two => cfg.two_payout_multiplier.1,
        Roll::Three => cfg.three_payout_multiplier.1,
        Roll::Four => cfg.four_payout_multiplier.1,
        Roll::Five => cfg.five_payout_multiplier.1,
        Roll::Six => cfg.six_payout_multiplier.1,
        Roll::Seven => cfg.seven_payout_multiplier.1,
        Roll::Eight => cfg.eight_payout_multiplier.1,
        Roll::Nine => cfg.nine_payout_multiplier.1,
        Roll::Ten => cfg.ten_payout_multiplier.1,
        Roll::Eleven => cfg.eleven_payout_multiplier.1,
        Roll::Twelve => cfg.twelve_payout_multiplier.1,
    }
}

const CHIP_ASSET_BYTES: [u8; 32] = [1u8; 32];
const DEFAULT_ROLL_FREQUENCY: u32 = 10;
const DEFAULT_FUND_AMOUNT: u64 = 1_000_000;

fn fake_vrf_bin_path() -> std::path::PathBuf {
    super::manifest_path(
        "../../sway-projects/fake-vrf-contract/out/release/fake-vrf-contract.bin",
    )
}

pub async fn get_vrf_contract_instance(
    wallet: Wallet,
) -> (vrf_types::FakeVRFContract<Wallet>, ContractId) {
    let contract = Contract::load_from(fake_vrf_bin_path(), LoadConfiguration::default())
        .expect("failed to load fake VRF contract binary");
    let response = contract
        .deploy(&wallet, TxPolicies::default())
        .await
        .expect("failed to deploy fake VRF contract");
    let contract_id = response.contract_id;

    let instance = vrf_types::FakeVRFContract::new(contract_id.clone(), wallet);

    (instance, contract_id)
}

pub struct TestContext {
    alice: Wallet,
    owner: Wallet,
    contract_id: ContractId,
    chip_asset_id: AssetId,
    owner_instance: strapped_types::MyContract<Wallet>,
    alice_instance: strapped_types::MyContract<Wallet>,
    vrf_instance: vrf_types::FakeVRFContract<Wallet>,
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
                coin_amount: 10_000_000_000,
            },
            AssetConfig {
                id: chip_asset_id,
                num_coins: 1,
                coin_amount: 10_000_000_000,
            },
        ];
        base_assets.extend(extra_assets);
        let mut wallets = launch_custom_provider_and_get_wallets(
            WalletsConfig::new_multiple_assets(3, base_assets),
            None,
            None,
        )
        .await
        .expect("failed to launch local provider");

        let owner = wallets.pop().expect("missing owner wallet");
        let alice = wallets.pop().expect("missing alice wallet");

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
            .expect("initialize call failed");

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
            .expect("contract funding failed");

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

    pub fn alice(&self) -> Wallet {
        self.alice.clone()
    }

    pub fn owner(&self) -> Wallet {
        self.owner.clone()
    }

    pub fn contract_id(&self) -> ContractId {
        self.contract_id
    }

    pub fn chip_asset_id(&self) -> AssetId {
        self.chip_asset_id
    }

    pub fn owner_contract(&self) -> strapped_types::MyContract<Wallet> {
        self.owner_instance.clone()
    }

    pub fn alice_contract(&self) -> strapped_types::MyContract<Wallet> {
        self.alice_instance.clone()
    }

    pub fn vrf_contract(&self) -> vrf_types::FakeVRFContract<Wallet> {
        self.vrf_instance.clone()
    }

    pub fn owner_instance(&self) -> strapped_types::MyContract<Wallet> {
        self.owner_contract()
    }

    pub fn alice_instance(&self) -> strapped_types::MyContract<Wallet> {
        self.alice_contract()
    }

    pub fn vrf_instance(&self) -> vrf_types::FakeVRFContract<Wallet> {
        self.vrf_contract()
    }

    pub async fn advance_and_roll(&self, vrf_number: u64) {
        if let Some(next_height) = self
            .owner_instance
            .methods()
            .next_roll_height()
            .simulate(Execution::state_read_only())
            .await
            .expect("simulate next_roll_height failed")
            .value
        {
            self.advance_to_block_height(next_height).await;
        }

        self.vrf_instance
            .methods()
            .set_number(vrf_number)
            .call()
            .await
            .expect("set_number failed");

        self.owner_instance
            .methods()
            .roll_dice()
            .with_contracts(&[&self.vrf_instance])
            .call()
            .await
            .expect("roll_dice failed");
    }

    pub async fn advance_to_block_height(&self, height: u32) {
        let provider = self.owner.provider();
        let current_height = provider
            .latest_block_height()
            .await
            .expect("failed to fetch block height");
        let blocks_to_advance = height.saturating_sub(current_height);
        provider
            .produce_blocks(blocks_to_advance, None)
            .await
            .expect("failed to advance blocks");
    }
}

pub async fn get_wallet() -> Wallet {
    let mut wallets = launch_custom_provider_and_get_wallets(
        WalletsConfig::new(Some(1), Some(1), Some(1_000_000_000)),
        None,
        None,
    )
    .await
    .expect("failed to launch provider for get_wallet");
    wallets.pop().expect("missing wallet")
}

pub fn generate_straps(seed: u64) -> Vec<(Roll, Strap, u64)> {
    let mut straps: Vec<(Roll, Strap, u64)> = Vec::new();
    let mut multiple = 1;
    while seed % multiple == 0 && seed != 0 {
        let inner = seed / multiple;
        let strap = u64_to_strap(inner);
        let slot = u64_to_slot(inner);
        let cost = strap_to_cost(&strap);
        straps.push((slot, strap, cost));
        multiple *= 2;
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
        StrapKind::Coat // weight 2
    } else if modulo < 137 {
        StrapKind::Scarf // weight 2
    } else if modulo < 139 {
        StrapKind::Gloves // weight 2
    } else if modulo < 141 {
        StrapKind::Gown // weight 2
    } else {
        StrapKind::Belt // weight 1
    };
    Strap::new(level, kind, modifier)
}

pub fn u64_to_slot(num: u64) -> Roll {
    match num % 10 {
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

fn strap_to_cost(strap: &Strap) -> u64 {
    match strap.kind {
        StrapKind::Shirt => 10,
        StrapKind::Pants => 10,
        StrapKind::Shoes => 10,
        StrapKind::Dress => 10,
        StrapKind::Hat => 20,
        StrapKind::Glasses => 20,
        StrapKind::Watch => 20,
        StrapKind::Ring => 20,
        StrapKind::Necklace => 50,
        StrapKind::Earring => 50,
        StrapKind::Bracelet => 50,
        StrapKind::Tattoo => 50,
        StrapKind::Skirt => 50,
        StrapKind::Piercing => 50,
        StrapKind::Coat => 100,
        StrapKind::Scarf => 100,
        StrapKind::Gloves => 100,
        StrapKind::Gown => 100,
        StrapKind::Belt => 200,
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
    let mut seen = [false; 11];
    let mut triggers = Vec::new();
    let mut multiple = 1;

    while roll % multiple == 0 && roll != 0 {
        let inner = roll / multiple;
        let (modifier_roll, modifier) = u64_to_modifier(inner);
        let trigger_roll = u64_to_trigger_roll(inner);

        let index = match modifier_roll {
            Roll::Two => 0,
            Roll::Three => 1,
            Roll::Four => 2,
            Roll::Five => 3,
            Roll::Six => 4,
            Roll::Seven => 5,
            Roll::Eight => 6,
            Roll::Nine => 7,
            Roll::Ten => 8,
            Roll::Eleven => 9,
            Roll::Twelve => 10,
        };

        if !seen[index] {
            triggers.push((trigger_roll, modifier_roll, modifier));
            seen[index] = true;
        }

        multiple *= 3;
    }

    triggers
}

pub fn u64_to_modifier(num: u64) -> (Roll, Modifier) {
    match num % 11 {
        0 => (Roll::Two, Modifier::Burnt),
        1 => (Roll::Three, Modifier::Lucky),
        2 => (Roll::Four, Modifier::Holy),
        3 => (Roll::Five, Modifier::Holey),
        4 => (Roll::Six, Modifier::Scotch),
        8 => (Roll::Seven, Modifier::Evil),
        5 => (Roll::Eight, Modifier::Soaked),
        6 => (Roll::Nine, Modifier::Moldy),
        7 => (Roll::Ten, Modifier::Starched),
        9 => (Roll::Eleven, Modifier::Groovy),
        _ => (Roll::Twelve, Modifier::Delicate),
    }
}

pub fn u64_to_trigger_roll(num: u64) -> Roll {
    match num % 4 {
        0 => Roll::Two,
        1 => Roll::Three,
        2 => Roll::Eleven,
        _ => Roll::Twelve,
    }
}
