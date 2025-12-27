use anyhow::{
    Context,
    Result,
    anyhow,
};
use chrono::Utc;
use serde::{
    Deserialize,
    Serialize,
};
use sha2::{
    Digest,
    Sha256,
};
use std::{
    fmt,
    fs,
    io::Write,
    path::{
        Path,
        PathBuf,
    },
};

pub const DEPLOYMENTS_ROOT: &str = ".deployments";
const DEPLOYMENTS_FILE: &str = "deployments.json";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeploymentEnv {
    Dev,
    Test,
    Local,
}

impl DeploymentEnv {
    pub fn dir_name(self) -> &'static str {
        match self {
            DeploymentEnv::Dev => "dev",
            DeploymentEnv::Test => "test",
            DeploymentEnv::Local => "local",
        }
    }
}

impl fmt::Display for DeploymentEnv {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            DeploymentEnv::Dev => "Devnet",
            DeploymentEnv::Test => "Testnet",
            DeploymentEnv::Local => "Local",
        };
        write!(f, "{name}")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeploymentRecord {
    pub deployed_at: String,
    pub contract_id: String,
    pub bytecode_hash: String,
    pub network_url: String,
    #[serde(default)]
    pub chip_asset_id: Option<String>,
    #[serde(default)]
    pub chip_asset_ticker: Option<String>,
    #[serde(default)]
    pub contract_salt: Option<String>,
    #[serde(default)]
    pub vrf_salt: Option<String>,
    #[serde(default)]
    pub vrf_contract_id: Option<String>,
    #[serde(default)]
    pub vrf_bytecode_hash: Option<String>,
    #[serde(default)]
    pub deployment_block_height: Option<u64>,
    #[serde(default)]
    pub roll_frequency: Option<u32>,
}

impl DeploymentRecord {
    pub fn is_compatible_with_hash(&self, hash: &str) -> bool {
        self.bytecode_hash == hash
    }
}

#[derive(Debug)]
pub struct DeploymentStore {
    path: PathBuf,
}

impl DeploymentStore {
    pub fn new(env: DeploymentEnv) -> Result<Self> {
        let path = ensure_store(env)?;
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<Option<DeploymentRecord>> {
        read_record(&self.path)
    }

    pub fn save(&self, record: DeploymentRecord) -> Result<()> {
        write_record(&self.path, &record)
    }
}

pub fn compute_bytecode_hash(path: impl AsRef<Path>) -> Result<String> {
    let path = path.as_ref();
    let bytes = fs::read(path).with_context(|| {
        format!(
            "Failed to read contract bytecode for hashing: {}",
            path.display()
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn ensure_structure() -> Result<()> {
    for env in [
        DeploymentEnv::Dev,
        DeploymentEnv::Test,
        DeploymentEnv::Local,
    ] {
        let _ = ensure_store(env)?;
    }
    Ok(())
}

fn ensure_store(env: DeploymentEnv) -> Result<PathBuf> {
    let root = Path::new(DEPLOYMENTS_ROOT);
    if !root.exists() {
        fs::create_dir_all(root).context("Failed to create .deployments directory")?;
    }

    let env_dir = root.join(env.dir_name());
    if !env_dir.exists() {
        fs::create_dir_all(&env_dir).with_context(|| {
            format!("Failed to create .deployments/{} directory", env.dir_name())
        })?;
    }

    let file_path = env_dir.join(DEPLOYMENTS_FILE);
    if !file_path.exists() {
        let mut file = fs::File::create(&file_path).with_context(|| {
            format!(
                "Failed to create deployment record file for {} at {:?}",
                env, file_path
            )
        })?;
        file.write_all(b"").with_context(|| {
            format!("Failed to initialize deployment record file for {}", env)
        })?;
    }

    Ok(file_path)
}

fn read_record(path: impl AsRef<Path>) -> Result<Option<DeploymentRecord>> {
    let data = fs::read(path.as_ref()).context("Failed to read deployment records")?;
    if data.iter().all(u8::is_ascii_whitespace) || data.is_empty() {
        return Ok(None);
    }
    if let Ok(record) = serde_json::from_slice::<DeploymentRecord>(&data) {
        return Ok(Some(record));
    }
    if let Ok(mut records) = serde_json::from_slice::<Vec<DeploymentRecord>>(&data) {
        return Ok(records.pop());
    }
    Err(anyhow!(
        "Failed to parse deployment record JSON; expected a single deployment object"
    ))
}

fn write_record(path: impl AsRef<Path>, record: &DeploymentRecord) -> Result<()> {
    let json = serde_json::to_vec_pretty(record)
        .context("Failed to serialize deployment record")?;
    fs::write(path.as_ref(), json).context("Failed to write deployment record")?;
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredStrap {
    pub level: u8,
    pub kind: String,
    pub modifier: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredStrapReward {
    pub roll: String,
    pub strap: StoredStrap,
    pub cost: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredModifier {
    pub roll: String,
    pub modifier: String,
    pub roll_index: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredBet {
    pub bet_type: String,
    pub amount: u64,
    pub roll_index: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strap: Option<StoredStrap>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredRollBets {
    pub roll: String,
    pub bets: Vec<StoredBet>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredGameHistory {
    pub game_id: u32,
    pub rolls: Vec<String>,
    pub modifiers: Vec<StoredModifier>,
    pub alice_bets: Vec<StoredRollBets>,
    pub strap_rewards: Vec<StoredStrapReward>,
    pub alice_claimed: bool,
}

pub fn record_deployment(
    env: DeploymentEnv,
    contract_id: impl AsRef<str>,
    bytecode_hash: impl AsRef<str>,
    network_url: impl AsRef<str>,
    chip_asset_id: Option<impl AsRef<str>>,
    chip_asset_ticker: Option<impl AsRef<str>>,
) -> Result<()> {
    let store = DeploymentStore::new(env)?;
    let record = DeploymentRecord {
        deployed_at: Utc::now().to_rfc3339(),
        contract_id: contract_id.as_ref().to_string(),
        bytecode_hash: bytecode_hash.as_ref().to_string(),
        network_url: network_url.as_ref().to_string(),
        chip_asset_id: chip_asset_id.map(|id| id.as_ref().to_string()),
        chip_asset_ticker: chip_asset_ticker.map(|ticker| ticker.as_ref().to_string()),
        contract_salt: None,
        vrf_salt: None,
        vrf_contract_id: None,
        vrf_bytecode_hash: None,
        deployment_block_height: None,
        roll_frequency: None,
    };
    store.save(record)
}
