use chrono::Utc;
use color_eyre::eyre::{Result, WrapErr};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fmt, fs,
    io::Write,
    path::{Path, PathBuf},
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
    pub contract_salt: Option<String>,
    #[serde(default)]
    pub vrf_salt: Option<String>,
    #[serde(default)]
    pub vrf_contract_id: Option<String>,
    #[serde(default)]
    pub vrf_bytecode_hash: Option<String>,
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

    pub fn load(&self) -> Result<Vec<DeploymentRecord>> {
        read_records(&self.path)
    }

    pub fn append(&self, record: DeploymentRecord) -> Result<()> {
        let mut records = self.load()?;
        records.push(record);
        write_records(&self.path, &records)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[allow(dead_code)]
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
    };
    store.append(record)
}

pub fn compute_bytecode_hash(path: impl AsRef<Path>) -> Result<String> {
    let path = path.as_ref();
    let bytes = fs::read(path).wrap_err_with(|| {
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
        fs::create_dir_all(root).wrap_err("Failed to create .deployments directory")?;
    }

    let env_dir = root.join(env.dir_name());
    if !env_dir.exists() {
        fs::create_dir_all(&env_dir).wrap_err_with(|| {
            format!("Failed to create .deployments/{} directory", env.dir_name())
        })?;
    }

    let file_path = env_dir.join(DEPLOYMENTS_FILE);
    if !file_path.exists() {
        let mut file = fs::File::create(&file_path).wrap_err_with(|| {
            format!(
                "Failed to create deployment record file for {} at {:?}",
                env, file_path
            )
        })?;
        file.write_all(b"[]").wrap_err_with(|| {
            format!("Failed to initialize deployment record file for {}", env)
        })?;
    }

    Ok(file_path)
}

fn read_records(path: impl AsRef<Path>) -> Result<Vec<DeploymentRecord>> {
    let data = fs::read(path.as_ref()).wrap_err("Failed to read deployment records")?;
    if data.is_empty() {
        return Ok(Vec::new());
    }
    let records = serde_json::from_slice::<Vec<DeploymentRecord>>(&data)
        .wrap_err("Failed to parse deployment records JSON")?;
    Ok(records)
}

fn write_records(path: impl AsRef<Path>, records: &[DeploymentRecord]) -> Result<()> {
    let json = serde_json::to_vec_pretty(records)
        .wrap_err("Failed to serialize deployment records")?;
    fs::write(path.as_ref(), json).wrap_err("Failed to write deployment records")?;
    Ok(())
}
