use std::collections::HashMap;

use futures::StreamExt;
use prost_types::FieldMask;
use sui_rpc::{
    Client,
    field::FieldMaskUtil,
    proto::sui::rpc::v2::{ListBalancesRequest, ListOwnedObjectsRequest},
};
use sui_sdk_types::Address;
use tokio::sync::mpsc;

use crate::coin_fetcher::{CoinBalance, SUI_DECIMALS, fetch_coin_decimals};

#[derive(Clone)]
pub struct OwnedObjectSummary {
    pub object_id: String,
    pub object_type: String,
}

#[derive(Clone)]
pub struct AddressData {
    pub balances: Vec<CoinBalance>,
    pub owned_objects: Vec<OwnedObjectSummary>,
}

impl AddressData {
    pub fn empty() -> Self {
        AddressData {
            balances: vec![],
            owned_objects: vec![],
        }
    }
}

pub struct AddressFetchResult {
    pub address: Address,
    pub rpc_url: String,
    pub outcome: Result<AddressData, String>,
}

pub fn spawn_address_fetch(
    address: Address,
    rpc_url: String,
    tx: mpsc::UnboundedSender<AddressFetchResult>,
) {
    let rpc_url_clone = rpc_url.clone();
    tokio::spawn(async move {
        let outcome = fetch_address_data(&address, &rpc_url_clone).await;
        let _ = tx.send(AddressFetchResult {
            address,
            rpc_url,
            outcome,
        });
    });
}

async fn fetch_address_data(address: &Address, rpc_url: &str) -> Result<AddressData, String> {
    let mut client = Client::new(rpc_url).map_err(|e| e.to_string())?;

    // Fetch balances
    let request = ListBalancesRequest::const_default()
        .with_owner(address.to_string())
        .with_page_size(1000);
    let stream = client.list_balances(request);
    futures::pin_mut!(stream);

    let mut raw_balances = Vec::new();
    while let Some(result) = stream.next().await {
        let bal = result.map_err(|e| e.to_string())?;
        let coin_type = bal.coin_type_opt().unwrap_or("unknown").to_string();
        let total_balance = bal.balance_opt().unwrap_or(0);
        raw_balances.push((coin_type, total_balance));
    }

    let unique_types: Vec<String> = raw_balances
        .iter()
        .map(|(ct, _)| ct.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let mut decimals_map = HashMap::new();
    for ct in &unique_types {
        let d = fetch_coin_decimals(&mut client, ct).await;
        decimals_map.insert(ct.clone(), d);
    }

    let mut balances: Vec<CoinBalance> = raw_balances
        .into_iter()
        .map(|(coin_type, total_balance)| {
            let decimals = decimals_map
                .get(&coin_type)
                .copied()
                .unwrap_or(SUI_DECIMALS);
            CoinBalance {
                coin_type,
                total_balance,
                decimals,
            }
        })
        .collect();
    balances.sort_by(|a, b| {
        let a_is_sui = a.coin_type.ends_with("::SUI");
        let b_is_sui = b.coin_type.ends_with("::SUI");
        match (a_is_sui, b_is_sui) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.coin_type.cmp(&b.coin_type),
        }
    });

    // Fetch owned objects
    let request = ListOwnedObjectsRequest::const_default()
        .with_owner(address.to_string())
        .with_page_size(200)
        .with_read_mask(FieldMask::from_str("object_id,version,digest,object_type"));
    let stream = client.list_owned_objects(request);
    futures::pin_mut!(stream);

    let mut owned_objects = Vec::new();
    while let Some(result) = stream.next().await {
        let obj = result.map_err(|e| e.to_string())?;
        owned_objects.push(OwnedObjectSummary {
            object_id: obj.object_id_opt().unwrap_or("").to_string(),
            object_type: obj.object_type_opt().unwrap_or("").to_string(),
        });
    }

    Ok(AddressData {
        balances,
        owned_objects,
    })
}
