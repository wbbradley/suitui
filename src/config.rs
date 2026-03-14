use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sui_keys::keystore::Alias;
use sui_sdk::wallet_context::WalletContext;
use sui_types::base_types::SuiAddress;

#[derive(Clone, Debug)]
pub struct Account {
    pub address: SuiAddress,
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
    pub active_address: Option<SuiAddress>,
    pub active_env: Option<String>,
    pub config_path: PathBuf,
}

pub fn default_config_path() -> Result<PathBuf> {
    let home = home::home_dir().context("could not determine home directory")?;
    Ok(home.join(".sui/sui_config/client.yaml"))
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
        .map(|(addr, alias): (&SuiAddress, &Alias)| Account {
            address: *addr,
            alias: alias.alias.clone(),
        })
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

    let active_address = wallet.active_address().ok();
    let active_env = wallet.config.active_env.clone();

    Ok(WalletData {
        accounts,
        envs,
        active_address,
        active_env,
        config_path: config_path.to_path_buf(),
    })
}
