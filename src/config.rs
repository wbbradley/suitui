use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sui_sdk_types::{Address, Ed25519PublicKey, Secp256k1PublicKey, Secp256r1PublicKey};

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

#[derive(Serialize, Deserialize)]
struct RawConfig {
    keystore: serde_yaml::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    external_keys: Option<serde_yaml::Value>,
    envs: Vec<RawEnv>,
    active_env: Option<String>,
    active_address: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct RawEnv {
    alias: String,
    rpc: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ws: Option<serde_yaml::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    basic_auth: Option<serde_yaml::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    chain_id: Option<String>,
}

#[derive(Deserialize)]
struct AliasEntry {
    alias: String,
    public_key_base64: String,
}

fn derive_address_from_public_key_base64(b64: &str) -> Result<Address> {
    use base64ct::{Base64, Encoding};
    let bytes = Base64::decode_vec(b64).map_err(|e| anyhow::anyhow!("invalid base64: {e}"))?;
    let (&scheme, pubkey_bytes) = bytes.split_first().context("empty public key")?;
    match scheme {
        0x00 => {
            let key: [u8; 32] = pubkey_bytes
                .try_into()
                .map_err(|_| anyhow::anyhow!("invalid ed25519 key length"))?;
            Ok(Ed25519PublicKey::new(key).derive_address())
        }
        0x01 => {
            let key: [u8; 33] = pubkey_bytes
                .try_into()
                .map_err(|_| anyhow::anyhow!("invalid secp256k1 key length"))?;
            Ok(Secp256k1PublicKey::new(key).derive_address())
        }
        0x02 => {
            let key: [u8; 33] = pubkey_bytes
                .try_into()
                .map_err(|_| anyhow::anyhow!("invalid secp256r1 key length"))?;
            Ok(Secp256r1PublicKey::new(key).derive_address())
        }
        other => Err(anyhow::anyhow!("unknown key scheme: 0x{other:02x}")),
    }
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
        let contents = std::fs::read_to_string(config_path)
            .with_context(|| format!("failed to read {}", config_path.display()))?;
        let mut raw: RawConfig = serde_yaml::from_str(&contents)
            .with_context(|| format!("failed to parse {}", config_path.display()))?;
        raw.active_address = active_address.map(|a| a.to_string());
        raw.active_env = active_env.map(String::from);
        let yaml = serde_yaml::to_string(&raw).context("failed to serialize config")?;
        std::fs::write(config_path, yaml)
            .with_context(|| format!("failed to write {}", config_path.display()))?;
        Ok(())
    })();
    if let Err(e) = result {
        eprintln!("warning: failed to save config: {e}");
    }
}

pub fn load_wallet_data(config_path: &Path) -> Result<WalletData> {
    let contents = std::fs::read_to_string(config_path).with_context(|| {
        format!(
            "failed to load wallet config from {}",
            config_path.display()
        )
    })?;
    let raw: RawConfig = serde_yaml::from_str(&contents).with_context(|| {
        format!(
            "failed to parse wallet config from {}",
            config_path.display()
        )
    })?;

    // Load aliases from sui.aliases sibling file
    let aliases_path = config_path
        .parent()
        .context("config path has no parent directory")?
        .join("sui.aliases");

    let accounts: Vec<Account> = if aliases_path.exists() {
        let aliases_contents = std::fs::read_to_string(&aliases_path)
            .with_context(|| format!("failed to read {}", aliases_path.display()))?;
        let entries: Vec<AliasEntry> = serde_json::from_str(&aliases_contents)
            .with_context(|| format!("failed to parse {}", aliases_path.display()))?;
        entries
            .into_iter()
            .filter_map(|entry| {
                match derive_address_from_public_key_base64(&entry.public_key_base64) {
                    Ok(address) => Some(Account {
                        address,
                        alias: entry.alias,
                    }),
                    Err(e) => {
                        eprintln!("warning: skipping alias '{}': {e}", entry.alias);
                        None
                    }
                }
            })
            .collect()
    } else {
        eprintln!(
            "warning: no aliases file found at {}; accounts list will be empty",
            aliases_path.display()
        );
        Vec::new()
    };

    let envs: Vec<Env> = raw
        .envs
        .into_iter()
        .map(|e| Env {
            alias: e.alias,
            rpc: e.rpc,
            chain_id: e.chain_id,
        })
        .collect();

    let active_address = raw
        .active_address
        .as_deref()
        .map(|s| s.parse::<Address>())
        .transpose()
        .context("failed to parse active_address")?;

    Ok(WalletData {
        accounts,
        envs,
        active_address,
        active_env: raw.active_env,
        config_path: config_path.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use base64ct::Encoding;

    use super::*;

    #[test]
    fn save_active_state_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("client.yaml");

        let initial_config = RawConfig {
            keystore: serde_yaml::Value::Mapping(serde_yaml::Mapping::new()),
            external_keys: None,
            envs: vec![RawEnv {
                alias: "testnet".into(),
                rpc: "https://fullnode.testnet.sui.io:443".into(),
                ws: None,
                basic_auth: None,
                chain_id: None,
            }],
            active_env: None,
            active_address: None,
        };
        let yaml = serde_yaml::to_string(&initial_config).unwrap();
        std::fs::write(&config_path, yaml).unwrap();

        let addr = Address::from_bytes([7u8; 32]).unwrap();
        save_active_state(&config_path, Some(addr), Some("testnet"));

        let reloaded_contents = std::fs::read_to_string(&config_path).unwrap();
        let reloaded: RawConfig = serde_yaml::from_str(&reloaded_contents).unwrap();
        assert_eq!(reloaded.active_address.as_deref(), Some(&*addr.to_string()));
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

    #[test]
    fn load_wallet_data_with_aliases() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("client.yaml");
        let aliases_path = dir.path().join("sui.aliases");

        // Create a minimal client.yaml
        let config = RawConfig {
            keystore: serde_yaml::Value::Mapping(serde_yaml::Mapping::new()),
            external_keys: None,
            envs: vec![RawEnv {
                alias: "testnet".into(),
                rpc: "https://fullnode.testnet.sui.io:443".into(),
                ws: None,
                basic_auth: None,
                chain_id: Some("abc123".into()),
            }],
            active_env: Some("testnet".into()),
            active_address: None,
        };
        std::fs::write(&config_path, serde_yaml::to_string(&config).unwrap()).unwrap();

        // Create a known ed25519 public key (scheme byte 0x00 + 32 zero bytes)
        let mut key_bytes = vec![0x00u8]; // ed25519 scheme
        key_bytes.extend_from_slice(&[0u8; 32]);
        let b64 = base64ct::Base64::encode_string(&key_bytes);

        let expected_address = Ed25519PublicKey::new([0u8; 32]).derive_address();

        let aliases = vec![serde_json::json!({
            "alias": "test-alias",
            "public_key_base64": b64,
        })];
        std::fs::write(&aliases_path, serde_json::to_string(&aliases).unwrap()).unwrap();

        let data = load_wallet_data(&config_path).unwrap();
        assert_eq!(data.accounts.len(), 1);
        assert_eq!(data.accounts[0].alias, "test-alias");
        assert_eq!(data.accounts[0].address, expected_address);
        assert_eq!(data.envs.len(), 1);
        assert_eq!(data.envs[0].alias, "testnet");
        assert_eq!(data.envs[0].chain_id.as_deref(), Some("abc123"));
        assert_eq!(data.active_env.as_deref(), Some("testnet"));
    }

    #[test]
    fn load_wallet_data_missing_aliases() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("client.yaml");

        let addr = Address::from_bytes([42u8; 32]).unwrap();
        let config = RawConfig {
            keystore: serde_yaml::Value::Mapping(serde_yaml::Mapping::new()),
            external_keys: None,
            envs: vec![RawEnv {
                alias: "devnet".into(),
                rpc: "https://fullnode.devnet.sui.io:443".into(),
                ws: None,
                basic_auth: None,
                chain_id: None,
            }],
            active_env: Some("devnet".into()),
            active_address: Some(addr.to_string()),
        };
        std::fs::write(&config_path, serde_yaml::to_string(&config).unwrap()).unwrap();

        let data = load_wallet_data(&config_path).unwrap();
        assert!(data.accounts.is_empty());
        assert_eq!(data.envs.len(), 1);
        assert_eq!(data.active_address, Some(addr));
    }

    #[test]
    fn derive_address_ed25519() {
        use base64ct::{Base64, Encoding};
        let mut key_bytes = vec![0x00u8];
        key_bytes.extend_from_slice(&[1u8; 32]);
        let b64 = Base64::encode_string(&key_bytes);

        let expected = Ed25519PublicKey::new([1u8; 32]).derive_address();
        let actual = derive_address_from_public_key_base64(&b64).unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn derive_address_invalid_base64() {
        let result = derive_address_from_public_key_base64("not-valid-base64!!!");
        assert!(result.is_err());
    }

    #[test]
    fn derive_address_unknown_scheme() {
        use base64ct::{Base64, Encoding};
        let mut key_bytes = vec![0xFFu8];
        key_bytes.extend_from_slice(&[0u8; 32]);
        let b64 = Base64::encode_string(&key_bytes);

        let result = derive_address_from_public_key_base64(&b64);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("unknown key scheme")
        );
    }
}
