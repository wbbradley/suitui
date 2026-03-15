use futures::StreamExt;
use sui_rpc::{
    Client,
    proto::sui::rpc::v2::{GetServiceInfoRequest, ListBalancesRequest},
};
use sui_sdk_types::Address;
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct CoinBalance {
    pub coin_type: String,
    pub total_balance: u64,
}

pub struct CoinFetchResult {
    pub address: Address,
    pub rpc_url: String,
    pub outcome: Result<Vec<CoinBalance>, String>,
}

pub fn format_balance(raw: u64, decimals: u32) -> String {
    if decimals == 0 {
        return raw.to_string();
    }
    let divisor = 10u64.pow(decimals);
    let whole = raw / divisor;
    let frac = raw % divisor;
    if frac == 0 {
        return whole.to_string();
    }
    let frac_str = format!("{:0>width$}", frac, width = decimals as usize);
    let trimmed = frac_str.trim_end_matches('0');
    format!("{whole}.{trimmed}")
}

pub fn parse_amount(input: &str, decimals: u32) -> Result<u64, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("amount is empty".into());
    }
    let parts: Vec<&str> = input.split('.').collect();
    if parts.len() > 2 {
        return Err("invalid number format".into());
    }
    let whole: u64 = if parts[0].is_empty() {
        0
    } else {
        parts[0].parse().map_err(|_| "invalid number".to_string())?
    };
    let frac_raw: u64 = if parts.len() == 2 {
        let frac_str = parts[1];
        if frac_str.len() > decimals as usize {
            return Err(format!("too many decimal places (max {decimals})"));
        }
        let padded = format!("{:0<width$}", frac_str, width = decimals as usize);
        padded
            .parse()
            .map_err(|_| "invalid fractional part".to_string())?
    } else {
        0
    };
    let divisor = 10u64.pow(decimals);
    let total = whole
        .checked_mul(divisor)
        .ok_or("amount too large")?
        .checked_add(frac_raw)
        .ok_or("amount too large")?;
    if total == 0 {
        return Err("amount must be greater than zero".into());
    }
    Ok(total)
}

pub fn short_coin_type(full: &str) -> &str {
    full.rsplit("::").next().unwrap_or(full)
}

pub struct ChainIdResult {
    pub rpc_url: String,
    pub outcome: Result<String, String>,
}

pub fn spawn_chain_id_fetch(rpc_url: String, tx: mpsc::UnboundedSender<ChainIdResult>) {
    let rpc_url_clone = rpc_url.clone();
    tokio::spawn(async move {
        let outcome = fetch_chain_id(&rpc_url_clone).await;
        let _ = tx.send(ChainIdResult { rpc_url, outcome });
    });
}

async fn fetch_chain_id(rpc_url: &str) -> Result<String, String> {
    let mut client = Client::new(rpc_url).map_err(|e| e.to_string())?;
    let resp = client
        .ledger_client()
        .get_service_info(GetServiceInfoRequest::default())
        .await
        .map_err(|e| e.to_string())?;
    resp.into_inner()
        .chain_id
        .ok_or_else(|| "chain_id not returned".into())
}

pub fn spawn_fetch(address: Address, rpc_url: String, tx: mpsc::UnboundedSender<CoinFetchResult>) {
    let rpc_url_clone = rpc_url.clone();
    tokio::spawn(async move {
        let outcome = fetch_balances(&address, &rpc_url_clone).await;
        let _ = tx.send(CoinFetchResult {
            address,
            rpc_url,
            outcome,
        });
    });
}

async fn fetch_balances(address: &Address, rpc_url: &str) -> Result<Vec<CoinBalance>, String> {
    let client = Client::new(rpc_url).map_err(|e| e.to_string())?;
    let request = ListBalancesRequest::const_default()
        .with_owner(address.to_string())
        .with_page_size(1000);

    let stream = client.list_balances(request);
    futures::pin_mut!(stream);

    let mut balances = Vec::new();
    while let Some(result) = stream.next().await {
        let bal = result.map_err(|e| e.to_string())?;
        let coin_type = bal.coin_type_opt().unwrap_or("unknown").to_string();
        let total_balance = bal.balance_opt().unwrap_or(0);
        balances.push(CoinBalance {
            coin_type,
            total_balance,
        });
    }

    // Sort: SUI first, then alphabetical
    balances.sort_by(|a, b| {
        let a_is_sui = a.coin_type.ends_with("::SUI");
        let b_is_sui = b.coin_type.ends_with("::SUI");
        match (a_is_sui, b_is_sui) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.coin_type.cmp(&b.coin_type),
        }
    });

    Ok(balances)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_balance_zero() {
        assert_eq!(format_balance(0, 9), "0");
    }

    #[test]
    fn format_balance_one_sui() {
        assert_eq!(format_balance(1_000_000_000, 9), "1");
    }

    #[test]
    fn format_balance_fractional() {
        assert_eq!(format_balance(1_500_000_000, 9), "1.5");
    }

    #[test]
    fn format_balance_small_fraction() {
        assert_eq!(format_balance(500_000, 9), "0.0005");
    }

    #[test]
    fn format_balance_zero_decimals() {
        assert_eq!(format_balance(42, 0), "42");
    }

    #[test]
    fn short_coin_type_sui() {
        assert_eq!(short_coin_type("0x2::sui::SUI"), "SUI");
    }

    #[test]
    fn short_coin_type_custom() {
        assert_eq!(short_coin_type("0xabc::mod::Type"), "Type");
    }

    #[test]
    fn short_coin_type_bare() {
        assert_eq!(short_coin_type("bare"), "bare");
    }

    #[test]
    fn parse_amount_whole() {
        assert_eq!(parse_amount("1", 9).unwrap(), 1_000_000_000);
    }

    #[test]
    fn parse_amount_fractional() {
        assert_eq!(parse_amount("1.5", 9).unwrap(), 1_500_000_000);
    }

    #[test]
    fn parse_amount_small() {
        assert_eq!(parse_amount("0.0005", 9).unwrap(), 500_000);
    }

    #[test]
    fn parse_amount_empty_err() {
        assert!(parse_amount("", 9).is_err());
    }

    #[test]
    fn parse_amount_zero_err() {
        assert!(parse_amount("0", 9).is_err());
    }

    #[test]
    fn parse_amount_too_many_decimals() {
        assert!(parse_amount("1.1234567890", 9).is_err());
    }

    #[test]
    fn parse_amount_not_a_number() {
        assert!(parse_amount("abc", 9).is_err());
    }

    #[test]
    fn parse_amount_round_trip() {
        let raw = parse_amount("1.5", 9).unwrap();
        assert_eq!(format_balance(raw, 9), "1.5");
    }
}
