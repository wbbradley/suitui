use prost_types::FieldMask;
use sui_rpc::{Client, field::FieldMaskUtil, proto::sui::rpc::v2::GetCheckpointRequest};
use tokio::sync::mpsc;

use crate::transaction_fetcher::GasCostSummary;

#[derive(Clone)]
pub struct CheckpointData {
    pub sequence_number: u64,
    pub digest: String,
    pub epoch: Option<u64>,
    pub timestamp: Option<prost_types::Timestamp>,
    pub total_network_transactions: Option<u64>,
    pub content_digest: String,
    pub previous_digest: Option<String>,
    pub gas_summary: Option<GasCostSummary>,
    pub is_end_of_epoch: bool,
    pub transaction_count: usize,
    pub transaction_digests: Vec<String>,
}

impl CheckpointData {
    pub fn empty() -> Self {
        CheckpointData {
            sequence_number: 0,
            digest: String::new(),
            epoch: None,
            timestamp: None,
            total_network_transactions: None,
            content_digest: String::new(),
            previous_digest: None,
            gas_summary: None,
            is_end_of_epoch: false,
            transaction_count: 0,
            transaction_digests: vec![],
        }
    }
}

pub struct CheckpointFetchResult {
    pub sequence_number: u64,
    pub rpc_url: String,
    pub outcome: Result<CheckpointData, String>,
}

pub fn spawn_checkpoint_fetch(
    sequence_number: u64,
    rpc_url: String,
    tx: mpsc::UnboundedSender<CheckpointFetchResult>,
) {
    let rpc_url_clone = rpc_url.clone();
    tokio::spawn(async move {
        let outcome = fetch_checkpoint(sequence_number, &rpc_url_clone).await;
        let _ = tx.send(CheckpointFetchResult {
            sequence_number,
            rpc_url,
            outcome,
        });
    });
}

async fn fetch_checkpoint(seq: u64, rpc_url: &str) -> Result<CheckpointData, String> {
    let mut client = Client::new(rpc_url).map_err(|e| e.to_string())?;

    let request = GetCheckpointRequest::by_sequence_number(seq).with_read_mask(FieldMask::from_str(
        "sequence_number,digest,summary.epoch,summary.timestamp,summary.total_network_transactions,summary.content_digest,summary.previous_digest,summary.epoch_rolling_gas_cost_summary,summary.end_of_epoch_data,contents.transactions",
    ));

    let resp = client
        .ledger_client()
        .get_checkpoint(request)
        .await
        .map_err(|e| e.to_string())?;

    let checkpoint = resp
        .into_inner()
        .checkpoint
        .ok_or("no checkpoint returned")?;

    let summary = checkpoint.summary_opt();

    let gas_summary = summary
        .and_then(|s| s.epoch_rolling_gas_cost_summary_opt())
        .map(|g| GasCostSummary {
            computation_cost: g.computation_cost_opt().unwrap_or(0),
            storage_cost: g.storage_cost_opt().unwrap_or(0),
            storage_rebate: g.storage_rebate_opt().unwrap_or(0),
        });

    let is_end_of_epoch = summary.and_then(|s| s.end_of_epoch_data_opt()).is_some();

    let contents = checkpoint.contents_opt();
    let transaction_digests: Vec<String> = contents
        .map(|c| {
            c.transactions
                .iter()
                .filter_map(|t| t.transaction_opt().map(|d| d.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let transaction_count = transaction_digests.len();

    Ok(CheckpointData {
        sequence_number: checkpoint.sequence_number_opt().unwrap_or(seq),
        digest: checkpoint.digest_opt().unwrap_or("").to_string(),
        epoch: summary.and_then(|s| s.epoch_opt()),
        timestamp: summary.and_then(|s| s.timestamp_opt()).cloned(),
        total_network_transactions: summary.and_then(|s| s.total_network_transactions_opt()),
        content_digest: summary
            .and_then(|s| s.content_digest_opt())
            .unwrap_or("")
            .to_string(),
        previous_digest: summary
            .and_then(|s| s.previous_digest_opt())
            .map(|d| d.to_string()),
        gas_summary,
        is_end_of_epoch,
        transaction_count,
        transaction_digests,
    })
}
