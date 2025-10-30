use color_eyre::eyre::{
    Result,
    WrapErr,
    eyre,
};
use eth_keystore::decrypt_key;
use fuels::{
    crypto::SecretKey,
    prelude::{
        Provider,
        Wallet,
        derivation::DEFAULT_DERIVATION_PATH,
        private_key::PrivateKeySigner,
    },
};
use rpassword::prompt_password;
use std::{
    fs,
    path::{
        Path,
        PathBuf,
    },
};

#[derive(Clone, Debug)]
pub struct WalletDescriptor {
    pub name: String,
    pub path: PathBuf,
}

impl WalletDescriptor {
    pub fn new(name: impl Into<String>, path: PathBuf) -> Self {
        Self {
            name: name.into(),
            path,
        }
    }
}

pub fn default_wallet_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME").wrap_err("HOME environment variable not set")?;
    Ok(PathBuf::from(home).join(".fuel").join("wallets"))
}

pub fn resolve_wallet_dir(dir: Option<&str>) -> Result<PathBuf> {
    match dir {
        Some(raw) => {
            let expanded = shellexpand::tilde(raw);
            Ok(PathBuf::from(expanded.into_owned()))
        }
        None => default_wallet_dir(),
    }
}

pub fn list_wallets(dir: &Path) -> Result<Vec<WalletDescriptor>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut wallets = Vec::new();
    for entry in fs::read_dir(dir).wrap_err("Failed to read wallet directory")? {
        let entry = entry.wrap_err("Failed to read wallet entry")?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("wallet") {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| eyre!("Invalid wallet filename {:?}", path))?
            .to_owned();
        wallets.push(WalletDescriptor::new(name, path));
    }
    wallets.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(wallets)
}

pub fn find_wallet(dir: &Path, name: &str) -> Result<WalletDescriptor> {
    let wallets = list_wallets(dir)?;
    wallets
        .into_iter()
        .find(|w| w.name == name)
        .ok_or_else(|| eyre!("Wallet '{name}' not found in {}", dir.to_string_lossy()))
}

pub fn unlock_wallet(
    descriptor: &WalletDescriptor,
    provider: &Provider,
) -> Result<Wallet> {
    let prompt = format!("Enter password for wallet '{}': ", descriptor.name);
    let password = prompt_password(prompt).wrap_err("Failed to read wallet password")?;

    let secret = decrypt_key(&descriptor.path, password.as_bytes())
        .map_err(|_| eyre!("Invalid password for wallet '{}'", descriptor.name))?;

    if let Ok(secret_key) = SecretKey::try_from(secret.as_slice()) {
        let signer = PrivateKeySigner::new(secret_key);
        return Ok(Wallet::new(signer, provider.clone()));
    }

    if let Ok(mnemonic) = std::str::from_utf8(&secret) {
        let word_count = mnemonic.split_whitespace().count();
        if word_count >= 12 {
            // let derivation_path = format!("{DEFAULT_DERIVATION_PATH_PREFIX}/0'/0/0");
            let private_key = SecretKey::new_from_mnemonic_phrase_with_path(
                mnemonic,
                DEFAULT_DERIVATION_PATH,
            )?;
            return Ok(Wallet::new(
                PrivateKeySigner::new(private_key),
                provider.clone(),
            ));
        }
    }

    Err(eyre!(
        "Wallet '{}' contained unsupported key material",
        descriptor.name
    ))
}
