use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sui_config::Config;
use sui_keys::keystore::Alias;
use sui_sdk::{sui_client_config::SuiClientConfig, wallet_context::WalletContext};
use sui_sdk_types::Address;

#[derive(Clone, Debug)]
pub struct Account {
    pub address: Address,
    pub alias: String,
}

#[derive(Clone, Debug)]
pub struct Env {
    pub alias: String,
    pub rpc: String,
    pub chain_id: Option<String>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct WalletData {
    pub accounts: Vec<Account>,
    pub envs: Vec<Env>,
    pub active_address: Option<Address>,
    pub active_env: Option<String>,
    pub config_path: PathBuf,
}

pub fn default_config_path() -> Result<PathBuf> {
    let home = home::home_dir().context("could not determine home directory")?;
    Ok(home.join(".sui/sui_config/client.yaml"))
}

pub fn save_active_state(
    config_path: &Path,
    active_address: Option<Address>,
    active_env: Option<&str>,
) {
    let result: Result<()> = (|| {
        let mut config = SuiClientConfig::load_with_lock(config_path)?;
        config.active_address =
            active_address.map(|a| a.to_string().parse().expect("valid sui address"));
        config.active_env = active_env.map(String::from);
        config.save_with_lock(config_path)?;
        Ok(())
    })();
    if let Err(e) = result {
        eprintln!("warning: failed to save config: {e}");
    }
}

pub fn load_wallet_data(config_path: &Path) -> Result<WalletData> {
    let mut wallet = WalletContext::new(config_path).with_context(|| {
        format!(
            "failed to load wallet config from {}",
            config_path.display()
        )
    })?;

    let accounts: Vec<Account> = wallet
        .addresses_with_alias()
        .into_iter()
        .map(
            |(addr, alias): (&sui_types::base_types::SuiAddress, &Alias)| Account {
                address: addr.to_string().parse().expect("valid sui address"),
                alias: alias.alias.clone(),
            },
        )
        .collect();

    let envs: Vec<Env> = wallet
        .config
        .envs
        .iter()
        .map(|e| Env {
            alias: e.alias.clone(),
            rpc: e.rpc.clone(),
            chain_id: e.chain_id.clone(),
        })
        .collect();

    let active_address = wallet
        .active_address()
        .ok()
        .map(|a| a.to_string().parse().expect("valid sui address"));
    let active_env = wallet.config.active_env.clone();

    Ok(WalletData {
        accounts,
        envs,
        active_address,
        active_env,
        config_path: config_path.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use sui_keys::keystore::{FileBasedKeystore, Keystore};
    use sui_sdk::sui_client_config::SuiClientConfig;

    use super::*;

    #[test]
    fn save_active_state_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let keystore_path = dir.path().join("test.keystore");
        let config_path = dir.path().join("client.yaml");

        let keystore = Keystore::from(FileBasedKeystore::load_or_create(&keystore_path).unwrap());
        let config = SuiClientConfig::new(keystore);
        config.save(&config_path).unwrap();

        let addr = Address::from_bytes([7u8; 32]).unwrap();
        save_active_state(&config_path, Some(addr), Some("testnet"));

        let reloaded = SuiClientConfig::load(&config_path).unwrap();
        let reloaded_addr: Option<Address> = reloaded
            .active_address
            .map(|a| a.to_string().parse().expect("valid sui address"));
        assert_eq!(reloaded_addr, Some(addr));
        assert_eq!(reloaded.active_env.as_deref(), Some("testnet"));
    }

    #[test]
    fn save_active_state_missing_file_no_panic() {
        save_active_state(
            Path::new("/nonexistent/path/client.yaml"),
            None,
            Some("testnet"),
        );
    }
}
