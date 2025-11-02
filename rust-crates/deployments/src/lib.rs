use anyhow::{
    Context,
    Result,
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
const HISTORY_FILE: &str = "history.json";

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
    pub contract_salt: Option<String>,
    #[serde(default)]
    pub vrf_salt: Option<String>,
    #[serde(default)]
    pub vrf_contract_id: Option<String>,
    #[serde(default)]
    pub vrf_bytecode_hash: Option<String>,
    #[serde(default)]
    pub deployment_block_height: Option<u64>,
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

    pub fn load(&self) -> Result<Vec<DeploymentRecord>> {
        read_records(&self.path)
    }

    pub fn append(&self, record: DeploymentRecord) -> Result<()> {
        let mut records = self.load()?;
        records.push(record);
        write_records(&self.path, &records)
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
        let _ = ensure_history(env, None)?;
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
        file.write_all(b"[]").with_context(|| {
            format!("Failed to initialize deployment record file for {}", env)
        })?;
    }

    Ok(file_path)
}

fn read_records(path: impl AsRef<Path>) -> Result<Vec<DeploymentRecord>> {
    let data = fs::read(path.as_ref()).context("Failed to read deployment records")?;
    if data.is_empty() {
        return Ok(Vec::new());
    }
    let records = serde_json::from_slice::<Vec<DeploymentRecord>>(&data)
        .context("Failed to parse deployment records JSON")?;
    Ok(records)
}

fn write_records(path: impl AsRef<Path>, records: &[DeploymentRecord]) -> Result<()> {
    let json = serde_json::to_vec_pretty(records)
        .context("Failed to serialize deployment records")?;
    fs::write(path.as_ref(), json).context("Failed to write deployment records")?;
    Ok(())
}

fn history_file_name(profile: Option<&str>) -> String {
    match profile.and_then(|p| {
        let sanitized = sanitize_profile_tag(p);
        if sanitized.is_empty() {
            None
        } else {
            Some(sanitized)
        }
    }) {
        Some(tag) => format!("history-{}.json", tag),
        None => HISTORY_FILE.to_string(),
    }
}

fn sanitize_profile_tag(input: &str) -> String {
    let mut result = String::new();
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            result.push(ch.to_ascii_lowercase());
        } else if ch == '-' {
            result.push('-');
        } else if (ch.is_ascii_whitespace() || ch == '_') && !result.ends_with('_') {
            result.push('_');
        }
    }
    result.trim_matches('_').to_string()
}

fn ensure_history(env: DeploymentEnv, profile: Option<&str>) -> Result<PathBuf> {
    let root = Path::new(DEPLOYMENTS_ROOT).join(env.dir_name());
    if !root.exists() {
        fs::create_dir_all(&root).with_context(|| {
            format!("Failed to create history directory for {}", env.dir_name())
        })?;
    }
    let filename = history_file_name(profile);
    let file_path = root.join(filename);
    if !file_path.exists() {
        let mut file = fs::File::create(&file_path).with_context(|| {
            format!(
                "Failed to create history record file for {} at {:?}",
                env, file_path
            )
        })?;
        file.write_all(b"[]").with_context(|| {
            format!("Failed to initialize history record file for {}", env)
        })?;
    }
    Ok(file_path)
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
    pub owner_bets: Vec<StoredRollBets>,
    pub alice_bets: Vec<StoredRollBets>,
    pub strap_rewards: Vec<StoredStrapReward>,
    pub owner_claimed: bool,
    pub alice_claimed: bool,
}

#[derive(Clone, Debug)]
pub struct HistoryStore {
    path: PathBuf,
}

impl HistoryStore {
    pub fn new(env: DeploymentEnv, profile: Option<&str>) -> Result<Self> {
        let path = ensure_history(env, profile)?;
        Ok(Self { path })
    }

    pub fn load(&self) -> Result<Vec<StoredGameHistory>> {
        let data = fs::read(&self.path).context("Failed to read game history records")?;
        if data.is_empty() {
            return Ok(Vec::new());
        }
        let records = serde_json::from_slice::<Vec<StoredGameHistory>>(&data)
            .context("Failed to parse game history JSON")?;
        Ok(records)
    }

    pub fn save(&self, records: &[StoredGameHistory]) -> Result<()> {
        let json = serde_json::to_vec_pretty(records)
            .context("Failed to serialize game history records")?;
        fs::write(&self.path, json).context("Failed to write game history records")?;
        Ok(())
    }
}

pub fn record_deployment(
    env: DeploymentEnv,
    contract_id: impl AsRef<str>,
    bytecode_hash: impl AsRef<str>,
    network_url: impl AsRef<str>,
    chip_asset_id: Option<impl AsRef<str>>,
) -> Result<()> {
    let store = DeploymentStore::new(env)?;
    let record = DeploymentRecord {
        deployed_at: Utc::now().to_rfc3339(),
        contract_id: contract_id.as_ref().to_string(),
        bytecode_hash: bytecode_hash.as_ref().to_string(),
        network_url: network_url.as_ref().to_string(),
        chip_asset_id: chip_asset_id.map(|id| id.as_ref().to_string()),
        contract_salt: None,
        vrf_salt: None,
        vrf_contract_id: None,
        vrf_bytecode_hash: None,
        deployment_block_height: None,
    };
    store.append(record)
}
