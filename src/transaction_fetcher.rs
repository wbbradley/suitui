use std::collections::{HashMap, HashSet};

use futures::StreamExt;
use prost_types::FieldMask;
use sui_rpc::{
    Client,
    field::FieldMaskUtil,
    proto::sui::rpc::v2::{
        BatchGetTransactionsRequest,
        ExecutedTransaction,
        ListOwnedObjectsRequest,
        get_transaction_result,
    },
};
use sui_sdk_types::Address;
use tokio::sync::mpsc;

use crate::coin_fetcher::fetch_coin_decimals;

#[derive(Clone)]
pub struct GasCostSummary {
    pub computation_cost: u64,
    pub storage_cost: u64,
    pub storage_rebate: u64,
}

#[derive(Clone)]
pub struct TxBalanceChange {
    pub coin_type: String,
    pub amount: String,
    pub decimals: u32,
}

#[derive(Clone)]
pub struct TransactionSummary {
    pub digest: String,
    pub timestamp: Option<prost_types::Timestamp>,
    pub success: Option<bool>,
    pub gas_used: Option<GasCostSummary>,
    pub balance_changes: Vec<TxBalanceChange>,
}

pub struct TxHistoryFetchResult {
    pub address: Address,
    pub rpc_url: String,
    pub outcome: Result<Vec<TransactionSummary>, String>,
}

pub fn spawn_tx_history_fetch(
    address: Address,
    rpc_url: String,
    tx: mpsc::UnboundedSender<TxHistoryFetchResult>,
) {
    let rpc_url_clone = rpc_url.clone();
    tokio::spawn(async move {
        let outcome = fetch_tx_history(&address, &rpc_url_clone).await;
        let _ = tx.send(TxHistoryFetchResult {
            address,
            rpc_url,
            outcome,
        });
    });
}

async fn fetch_tx_history(
    address: &Address,
    rpc_url: &str,
) -> Result<Vec<TransactionSummary>, String> {
    let mut client = Client::new(rpc_url).map_err(|e| e.to_string())?;

    // Phase 1: list owned objects to collect previous_transaction digests
    let request = ListOwnedObjectsRequest::const_default()
        .with_owner(address.to_string())
        .with_page_size(500)
        .with_read_mask(FieldMask::from_str("previous_transaction"));

    let stream = client.list_owned_objects(request);
    futures::pin_mut!(stream);

    let mut digests = HashSet::new();
    while let Some(result) = stream.next().await {
        let obj = result.map_err(|e| e.to_string())?;
        if let Some(d) = obj.previous_transaction_opt()
            && !d.is_empty()
        {
            digests.insert(d.to_string());
        }
    }

    if digests.is_empty() {
        return Ok(vec![]);
    }

    // Cap at 100 unique digests
    let digests: Vec<String> = digests.into_iter().take(100).collect();

    // Phase 2: batch fetch transaction details
    let mut request = BatchGetTransactionsRequest::const_default();
    request.digests = digests;
    request.read_mask = Some(FieldMask::from_str(
        "digest,timestamp,effects.status,effects.gas_used,balance_changes,checkpoint",
    ));
    let resp = client
        .ledger_client()
        .batch_get_transactions(request)
        .await
        .map_err(|e| e.to_string())?;

    let mut summaries = Vec::new();
    for result in resp.into_inner().transactions {
        if let Some(get_transaction_result::Result::Transaction(tx)) = result.result {
            summaries.push(convert_transaction(&tx));
        }
    }

    // Sort by timestamp descending (most recent first)
    summaries.sort_by(|a, b| {
        let ts_a = a.timestamp.as_ref().map(|t| (t.seconds, t.nanos));
        let ts_b = b.timestamp.as_ref().map(|t| (t.seconds, t.nanos));
        ts_b.cmp(&ts_a)
    });

    // Enrich balance changes with coin decimals
    let unique_coin_types: HashSet<String> = summaries
        .iter()
        .flat_map(|s| s.balance_changes.iter().map(|bc| bc.coin_type.clone()))
        .collect();
    let mut decimals_map = HashMap::new();
    for ct in &unique_coin_types {
        let d = fetch_coin_decimals(&mut client, ct).await;
        decimals_map.insert(ct.clone(), d);
    }
    for summary in &mut summaries {
        for bc in &mut summary.balance_changes {
            if let Some(&d) = decimals_map.get(&bc.coin_type) {
                bc.decimals = d;
            }
        }
    }

    Ok(summaries)
}

fn convert_transaction(tx: &ExecutedTransaction) -> TransactionSummary {
    let effects = tx.effects_opt();
    let status = effects.and_then(|e| e.status_opt());
    let gas = effects.and_then(|e| e.gas_used_opt());

    TransactionSummary {
        digest: tx.digest_opt().unwrap_or("").to_string(),
        timestamp: tx.timestamp_opt().cloned(),
        success: status.and_then(|s| s.success_opt()),
        gas_used: gas.map(|g| GasCostSummary {
            computation_cost: g.computation_cost_opt().unwrap_or(0),
            storage_cost: g.storage_cost_opt().unwrap_or(0),
            storage_rebate: g.storage_rebate_opt().unwrap_or(0),
        }),
        balance_changes: tx
            .balance_changes
            .iter()
            .map(|bc| TxBalanceChange {
                coin_type: bc.coin_type_opt().unwrap_or("").to_string(),
                amount: bc.amount_opt().unwrap_or("").to_string(),
                decimals: 0,
            })
            .collect(),
    }
}

pub fn format_timestamp(ts: &prost_types::Timestamp) -> String {
    let total_secs = ts.seconds;
    let secs_per_day: i64 = 86400;
    let days_since_epoch = total_secs.div_euclid(secs_per_day);
    let time_of_day = total_secs.rem_euclid(secs_per_day);
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;

    let (year, month, day) = days_to_ymd(days_since_epoch);
    format!("{year:04}-{month:02}-{day:02} {hours:02}:{minutes:02}")
}

/// Howard Hinnant's civil_from_days algorithm
fn days_to_ymd(days: i64) -> (i64, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_timestamp_epoch() {
        let ts = prost_types::Timestamp {
            seconds: 0,
            nanos: 0,
        };
        assert_eq!(format_timestamp(&ts), "1970-01-01 00:00");
    }

    #[test]
    fn format_timestamp_one_billion() {
        // 1_000_000_000 = 2001-09-09 01:46:40 UTC
        let ts = prost_types::Timestamp {
            seconds: 1_000_000_000,
            nanos: 0,
        };
        assert_eq!(format_timestamp(&ts), "2001-09-09 01:46");
    }

    #[test]
    fn format_timestamp_2025() {
        // 2025-01-01 00:00 UTC = 1735689600
        let ts = prost_types::Timestamp {
            seconds: 1_735_689_600,
            nanos: 0,
        };
        assert_eq!(format_timestamp(&ts), "2025-01-01 00:00");
    }

    #[test]
    fn format_timestamp_with_time() {
        // 2025-01-01 13:45 UTC = 1735689600 + 13*3600 + 45*60 = 1735739100
        let ts = prost_types::Timestamp {
            seconds: 1_735_739_100,
            nanos: 0,
        };
        assert_eq!(format_timestamp(&ts), "2025-01-01 13:45");
    }
}
