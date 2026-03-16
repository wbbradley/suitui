use std::time::Duration;

use prost_types::FieldMask;
use sui_crypto::{
    SuiSigner,
    ed25519::Ed25519PrivateKey,
    secp256k1::Secp256k1PrivateKey,
    secp256r1::Secp256r1PrivateKey,
};
use sui_rpc::{Client, field::FieldMaskUtil, proto::sui::rpc::v2::ExecuteTransactionRequest};
use sui_sdk_types::{Address, StructTag};
use sui_transaction_builder::{TransactionBuilder, intent::CoinWithBalance};
use tokio::sync::mpsc;

pub struct TransferParams {
    pub sender: Address,
    pub recipient: Address,
    pub coin_type: String,
    pub amount_raw: u64,
    pub key_scheme: u8,
    pub private_key_bytes: [u8; 32],
}

pub enum TransferResult {
    Success { digest: String },
    Error(String),
}

pub struct TransferExecuteResult {
    pub result: TransferResult,
}

pub fn spawn_execute_transfer(
    params: TransferParams,
    rpc_url: String,
    tx: mpsc::UnboundedSender<TransferExecuteResult>,
) {
    // TransactionBuilder holds non-Send trait objects, so we must run on a
    // dedicated thread with its own runtime instead of tokio::spawn.
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("failed to create runtime");
        let result = rt.block_on(execute_transfer(params, &rpc_url));
        let _ = tx.send(TransferExecuteResult { result });
    });
}

async fn execute_transfer(params: TransferParams, rpc_url: &str) -> TransferResult {
    match execute_transfer_inner(params, rpc_url).await {
        Ok(digest) => TransferResult::Success { digest },
        Err(e) => TransferResult::Error(e),
    }
}

async fn execute_transfer_inner(params: TransferParams, rpc_url: &str) -> Result<String, String> {
    let struct_tag: StructTag = params
        .coin_type
        .parse()
        .map_err(|e| format!("invalid coin type: {e}"))?;

    let is_sui = struct_tag == StructTag::sui();

    let mut builder = TransactionBuilder::new();

    let coin = if is_sui {
        builder.intent(CoinWithBalance::sui(params.amount_raw))
    } else {
        builder.intent(CoinWithBalance::new(struct_tag, params.amount_raw))
    };

    let recipient_arg = builder.pure(&params.recipient);
    builder.transfer_objects(vec![coin], recipient_arg);
    builder.set_sender(params.sender);

    let mut client =
        Client::new(rpc_url).map_err(|e| format!("failed to create RPC client: {e}"))?;

    let transaction = builder
        .build(&mut client)
        .await
        .map_err(|e| format!("failed to build transaction: {e}"))?;

    let signature = sign_transaction(&params.key_scheme, &params.private_key_bytes, &transaction)?;

    let request = ExecuteTransactionRequest::new(transaction.into())
        .with_signatures(vec![signature.into()])
        .with_read_mask(FieldMask::from_str("digest"));

    let response = client
        .execute_transaction_and_wait_for_checkpoint(request, Duration::from_secs(30))
        .await
        .map_err(|e| {
            format!("{e} (note: the transaction may have succeeded even if this timed out)")
        })?;

    let digest = response
        .into_inner()
        .transaction
        .and_then(|t| t.digest)
        .unwrap_or_else(|| "unknown".into());

    Ok(digest)
}

fn sign_transaction(
    scheme: &u8,
    private_key_bytes: &[u8; 32],
    tx: &sui_sdk_types::Transaction,
) -> Result<sui_sdk_types::UserSignature, String> {
    match scheme {
        0x00 => {
            let key = Ed25519PrivateKey::new(*private_key_bytes);
            key.sign_transaction(tx).map_err(|e| e.to_string())
        }
        0x01 => {
            let key = Secp256k1PrivateKey::new(*private_key_bytes).map_err(|e| e.to_string())?;
            key.sign_transaction(tx).map_err(|e| e.to_string())
        }
        0x02 => {
            let key = Secp256r1PrivateKey::new(*private_key_bytes);
            key.sign_transaction(tx).map_err(|e| e.to_string())
        }
        other => Err(format!("unknown key scheme: 0x{other:02x}")),
    }
}

#[cfg(test)]
mod tests {
    use sui_sdk_types::StructTag;

    #[test]
    fn parse_sui_coin_type() {
        let parsed: StructTag = "0x2::sui::SUI".parse().unwrap();
        assert_eq!(parsed, StructTag::sui());
    }

    #[test]
    fn parse_custom_coin_type() {
        let parsed: Result<StructTag, _> = "0xabc::mod_name::USDC".parse();
        assert!(parsed.is_ok());
    }

    #[test]
    fn parse_invalid_coin_type() {
        let parsed: Result<StructTag, _> = "not a valid type".parse();
        assert!(parsed.is_err());
    }
}
