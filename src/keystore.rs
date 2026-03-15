use std::path::Path;

use sui_crypto::{
    SuiSigner,
    ed25519::Ed25519PrivateKey,
    secp256k1::Secp256k1PrivateKey,
    secp256r1::Secp256r1PrivateKey,
};
use sui_sdk_types::{Address, Transaction, UserSignature};

#[derive(Debug)]
pub struct KeyEntry {
    pub address: Address,
    scheme: u8,
    private_key_bytes: [u8; 32],
}

impl KeyEntry {
    #[allow(dead_code)]
    pub fn sign_transaction(&self, tx: &Transaction) -> Result<UserSignature, String> {
        match self.scheme {
            0x00 => {
                let key = Ed25519PrivateKey::new(self.private_key_bytes);
                key.sign_transaction(tx).map_err(|e| e.to_string())
            }
            0x01 => {
                let key =
                    Secp256k1PrivateKey::new(self.private_key_bytes).map_err(|e| e.to_string())?;
                key.sign_transaction(tx).map_err(|e| e.to_string())
            }
            0x02 => {
                let key = Secp256r1PrivateKey::new(self.private_key_bytes);
                key.sign_transaction(tx).map_err(|e| e.to_string())
            }
            other => Err(format!("unknown key scheme: 0x{other:02x}")),
        }
    }
}

pub fn load_keystore(path: &Path) -> Result<Vec<KeyEntry>, String> {
    let contents = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read keystore at {}: {e}", path.display()))?;
    let entries: Vec<String> = serde_json::from_str(&contents)
        .map_err(|e| format!("failed to parse keystore JSON: {e}"))?;

    use base64ct::{Base64, Encoding};
    let mut keys = Vec::new();
    for b64 in &entries {
        let bytes =
            Base64::decode_vec(b64).map_err(|e| format!("invalid base64 in keystore: {e}"))?;
        if bytes.len() != 33 {
            return Err(format!(
                "expected 33 bytes (1 scheme + 32 key), got {}",
                bytes.len()
            ));
        }
        let scheme = bytes[0];
        let mut private_key_bytes = [0u8; 32];
        private_key_bytes.copy_from_slice(&bytes[1..33]);

        let address = derive_address(scheme, &private_key_bytes)?;
        keys.push(KeyEntry {
            address,
            scheme,
            private_key_bytes,
        });
    }
    Ok(keys)
}

fn derive_address(scheme: u8, private_key_bytes: &[u8; 32]) -> Result<Address, String> {
    match scheme {
        0x00 => {
            let key = Ed25519PrivateKey::new(*private_key_bytes);
            Ok(key.public_key().derive_address())
        }
        0x01 => {
            let key = Secp256k1PrivateKey::new(*private_key_bytes).map_err(|e| e.to_string())?;
            Ok(key.public_key().derive_address())
        }
        0x02 => {
            let key = Secp256r1PrivateKey::new(*private_key_bytes);
            Ok(key.public_key().derive_address())
        }
        other => Err(format!("unknown key scheme: 0x{other:02x}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_keystore_ed25519() {
        use base64ct::{Base64, Encoding};

        let private_bytes = [42u8; 32];
        let mut entry_bytes = vec![0x00u8];
        entry_bytes.extend_from_slice(&private_bytes);
        let b64 = Base64::encode_string(&entry_bytes);

        let dir = tempfile::tempdir().unwrap();
        let keystore_path = dir.path().join("sui.keystore");
        std::fs::write(&keystore_path, serde_json::to_string(&vec![b64]).unwrap()).unwrap();

        let keys = load_keystore(&keystore_path).unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].scheme, 0x00);

        let expected = Ed25519PrivateKey::new(private_bytes)
            .public_key()
            .derive_address();
        assert_eq!(keys[0].address, expected);
    }

    #[test]
    fn load_keystore_empty() {
        let dir = tempfile::tempdir().unwrap();
        let keystore_path = dir.path().join("sui.keystore");
        std::fs::write(&keystore_path, "[]").unwrap();

        let keys = load_keystore(&keystore_path).unwrap();
        assert!(keys.is_empty());
    }

    #[test]
    fn load_keystore_invalid_base64() {
        let dir = tempfile::tempdir().unwrap();
        let keystore_path = dir.path().join("sui.keystore");
        std::fs::write(
            &keystore_path,
            serde_json::to_string(&vec!["not-valid!!!".to_string()]).unwrap(),
        )
        .unwrap();

        let result = load_keystore(&keystore_path);
        assert!(result.is_err());
    }

    #[test]
    fn load_keystore_wrong_length() {
        use base64ct::{Base64, Encoding};

        let b64 = Base64::encode_string(&[0x00, 1, 2, 3]);

        let dir = tempfile::tempdir().unwrap();
        let keystore_path = dir.path().join("sui.keystore");
        std::fs::write(&keystore_path, serde_json::to_string(&vec![b64]).unwrap()).unwrap();

        let result = load_keystore(&keystore_path);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expected 33 bytes"));
    }

    #[test]
    fn key_entry_fields_correct() {
        use base64ct::{Base64, Encoding};

        let private_bytes = [7u8; 32];
        let mut entry_bytes = vec![0x00u8];
        entry_bytes.extend_from_slice(&private_bytes);
        let b64 = Base64::encode_string(&entry_bytes);

        let dir = tempfile::tempdir().unwrap();
        let keystore_path = dir.path().join("sui.keystore");
        std::fs::write(&keystore_path, serde_json::to_string(&vec![b64]).unwrap()).unwrap();

        let keys = load_keystore(&keystore_path).unwrap();
        assert_eq!(keys[0].private_key_bytes, private_bytes);
        assert_eq!(keys[0].scheme, 0x00);
    }
}
