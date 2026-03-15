pub const OBJECT_NOT_FOUND: &str = "object not found";

use futures::StreamExt;
use prost_types::FieldMask;
use sui_rpc::{
    Client,
    field::FieldMaskUtil,
    proto::sui::rpc::v2::{
        DynamicField,
        GetObjectRequest,
        ListDynamicFieldsRequest,
        Object,
        Owner,
        dynamic_field,
        owner,
    },
};
use sui_sdk_types::Address;
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct ObjectData {
    pub version: u64,
    pub digest: String,
    pub owner: OwnerInfo,
    pub object_type: String,
    pub json: Option<serde_json::Value>,
    pub previous_transaction: String,
    pub storage_rebate: u64,
    pub balance: Option<u64>,
}

impl ObjectData {
    pub fn empty() -> Self {
        ObjectData {
            version: 0,
            digest: String::new(),
            owner: OwnerInfo::Unknown,
            object_type: String::new(),
            json: None,
            previous_transaction: String::new(),
            storage_rebate: 0,
            balance: None,
        }
    }
}

#[derive(Clone)]
pub enum OwnerInfo {
    Address(String),
    Object(String),
    Shared,
    Immutable,
    Unknown,
}

#[derive(Clone)]
pub struct DynFieldInfo {
    pub field_id: String,
    pub kind: DynFieldKind,
    pub value_type: String,
    pub child_id: Option<String>,
}

#[derive(Clone)]
pub enum DynFieldKind {
    Field,
    Object,
    Unknown,
}

pub struct ObjectFetchResult {
    pub object_id: Address,
    pub rpc_url: String,
    pub outcome: Result<ObjectData, String>,
}

pub struct DynFieldsFetchResult {
    pub parent_id: Address,
    pub rpc_url: String,
    pub outcome: Result<Vec<DynFieldInfo>, String>,
}

pub fn spawn_object_fetch(
    object_id: Address,
    rpc_url: String,
    tx: mpsc::UnboundedSender<ObjectFetchResult>,
) {
    let rpc_url_clone = rpc_url.clone();
    tokio::spawn(async move {
        let outcome = fetch_object(&object_id, &rpc_url_clone).await;
        let _ = tx.send(ObjectFetchResult {
            object_id,
            rpc_url,
            outcome,
        });
    });
}

pub fn spawn_dyn_fields_fetch(
    parent_id: Address,
    rpc_url: String,
    tx: mpsc::UnboundedSender<DynFieldsFetchResult>,
) {
    let rpc_url_clone = rpc_url.clone();
    tokio::spawn(async move {
        let outcome = fetch_dyn_fields(&parent_id, &rpc_url_clone).await;
        let _ = tx.send(DynFieldsFetchResult {
            parent_id,
            rpc_url,
            outcome,
        });
    });
}

async fn fetch_object(object_id: &Address, rpc_url: &str) -> Result<ObjectData, String> {
    let mut client = Client::new(rpc_url).map_err(|e| e.to_string())?;
    let request = GetObjectRequest::const_default()
        .with_object_id(object_id.to_string())
        .with_read_mask(FieldMask::from_str(
            "object_id,version,digest,owner,object_type,json,previous_transaction,storage_rebate,balance",
        ));
    let resp = client
        .ledger_client()
        .get_object(request)
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("not found") || msg.contains("NOT_FOUND") {
                OBJECT_NOT_FOUND.to_string()
            } else {
                msg
            }
        })?;
    let obj = resp.into_inner().object.ok_or(OBJECT_NOT_FOUND)?;
    Ok(convert_object(&obj))
}

async fn fetch_dyn_fields(parent_id: &Address, rpc_url: &str) -> Result<Vec<DynFieldInfo>, String> {
    let client = Client::new(rpc_url).map_err(|e| e.to_string())?;
    let request = ListDynamicFieldsRequest::const_default()
        .with_parent(parent_id.to_string())
        .with_page_size(100)
        .with_read_mask(FieldMask::from_str("kind,field_id,value_type,child_id"));
    let stream = client.list_dynamic_fields(request);
    futures::pin_mut!(stream);

    let mut fields = Vec::new();
    while let Some(result) = stream.next().await {
        let df = result.map_err(|e| e.to_string())?;
        fields.push(convert_dyn_field(&df));
    }
    Ok(fields)
}

fn convert_object(obj: &Object) -> ObjectData {
    ObjectData {
        version: obj.version_opt().unwrap_or(0),
        digest: obj.digest_opt().unwrap_or("").to_string(),
        owner: obj
            .owner_opt()
            .map(convert_owner)
            .unwrap_or(OwnerInfo::Unknown),
        object_type: obj.object_type_opt().unwrap_or("").to_string(),
        json: obj.json_opt().map(prost_value_to_json),
        previous_transaction: obj.previous_transaction_opt().unwrap_or("").to_string(),
        storage_rebate: obj.storage_rebate_opt().unwrap_or(0),
        balance: obj.balance_opt(),
    }
}

fn convert_owner(owner: &Owner) -> OwnerInfo {
    use owner::OwnerKind;
    match OwnerKind::try_from(owner.kind.unwrap_or(0)) {
        Ok(OwnerKind::Address) | Ok(OwnerKind::ConsensusAddress) => {
            OwnerInfo::Address(owner.address_opt().unwrap_or("").to_string())
        }
        Ok(OwnerKind::Object) => OwnerInfo::Object(owner.address_opt().unwrap_or("").to_string()),
        Ok(OwnerKind::Shared) => OwnerInfo::Shared,
        Ok(OwnerKind::Immutable) => OwnerInfo::Immutable,
        _ => OwnerInfo::Unknown,
    }
}

fn convert_dyn_field(df: &DynamicField) -> DynFieldInfo {
    use dynamic_field::DynamicFieldKind;
    DynFieldInfo {
        field_id: df.field_id_opt().unwrap_or("").to_string(),
        kind: match DynamicFieldKind::try_from(df.kind.unwrap_or(0)) {
            Ok(DynamicFieldKind::Field) => DynFieldKind::Field,
            Ok(DynamicFieldKind::Object) => DynFieldKind::Object,
            _ => DynFieldKind::Unknown,
        },
        value_type: df.value_type_opt().unwrap_or("").to_string(),
        child_id: df.child_id_opt().map(|s| s.to_string()),
    }
}

pub(crate) fn prost_value_to_json(value: &prost_types::Value) -> serde_json::Value {
    use prost_types::value::Kind;
    match &value.kind {
        None | Some(Kind::NullValue(_)) => serde_json::Value::Null,
        Some(Kind::NumberValue(n)) => serde_json::json!(*n),
        Some(Kind::StringValue(s)) => serde_json::Value::String(s.clone()),
        Some(Kind::BoolValue(b)) => serde_json::Value::Bool(*b),
        Some(Kind::StructValue(s)) => {
            let map = s
                .fields
                .iter()
                .map(|(k, v)| (k.clone(), prost_value_to_json(v)))
                .collect();
            serde_json::Value::Object(map)
        }
        Some(Kind::ListValue(l)) => {
            serde_json::Value::Array(l.values.iter().map(prost_value_to_json).collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prost_value_to_json_string() {
        let val = prost_types::Value {
            kind: Some(prost_types::value::Kind::StringValue("hello".into())),
        };
        assert_eq!(prost_value_to_json(&val), serde_json::json!("hello"));
    }

    #[test]
    fn prost_value_to_json_struct() {
        let val = prost_types::Value {
            kind: Some(prost_types::value::Kind::StructValue(prost_types::Struct {
                fields: [(
                    "key".into(),
                    prost_types::Value {
                        kind: Some(prost_types::value::Kind::NumberValue(42.0)),
                    },
                )]
                .into_iter()
                .collect(),
            })),
        };
        assert_eq!(prost_value_to_json(&val), serde_json::json!({"key": 42.0}));
    }

    #[test]
    fn prost_value_to_json_null() {
        let val = prost_types::Value { kind: None };
        assert_eq!(prost_value_to_json(&val), serde_json::Value::Null);
    }
}
