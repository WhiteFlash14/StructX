use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{DeepBookClientError, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObjectOwnerKind {
    Shared,
    Immutable,
    AddressOwner,
    ObjectOwner,
    Unknown,
}

impl std::fmt::Display for ObjectOwnerKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Shared => write!(f, "shared"),
            Self::Immutable => write!(f, "immutable"),
            Self::AddressOwner => write!(f, "address-owned"),
            Self::ObjectOwner => write!(f, "object-owned"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuiObjectInfo {
    pub object_id: String,
    pub object_type: Option<String>,
    pub version: Option<u64>,
    pub digest: Option<String>,
    pub owner_kind: ObjectOwnerKind,
    pub initial_shared_version: Option<u64>,
    pub raw: Value,
}

impl SuiObjectInfo {
    pub fn from_get_object_result(expected_object_id: &str, value: Value) -> Result<Self> {
        let data = value.get("data").ok_or_else(|| DeepBookClientError::UnexpectedShape {
            endpoint: "sui_getObject".to_string(),
            message: "missing `data` field".to_string(),
        })?;

        let object_id = data
            .get("objectId")
            .or_else(|| data.get("object_id"))
            .and_then(Value::as_str)
            .unwrap_or(expected_object_id)
            .to_string();

        let object_type = data.get("type").and_then(Value::as_str).map(ToString::to_string);

        let version = data.get("version").and_then(value_to_u64);

        let digest = data.get("digest").and_then(Value::as_str).map(ToString::to_string);

        let owner = data.get("owner");

        let owner_kind = parse_owner_kind(owner);
        let initial_shared_version = parse_initial_shared_version(owner);

        Ok(Self {
            object_id,
            object_type,
            version,
            digest,
            owner_kind,
            initial_shared_version,
            raw: value,
        })
    }

    #[must_use]
    pub fn is_shared(&self) -> bool {
        self.owner_kind == ObjectOwnerKind::Shared
    }

    #[must_use]
    pub fn has_shared_version(&self) -> bool {
        self.initial_shared_version.is_some()
    }
}

fn parse_owner_kind(owner: Option<&Value>) -> ObjectOwnerKind {
    match owner {
        Some(Value::String(value)) if value.eq_ignore_ascii_case("Immutable") => {
            ObjectOwnerKind::Immutable
        }
        Some(Value::Object(map)) if map.contains_key("Shared") => ObjectOwnerKind::Shared,
        Some(Value::Object(map)) if map.contains_key("AddressOwner") => {
            ObjectOwnerKind::AddressOwner
        }
        Some(Value::Object(map)) if map.contains_key("ObjectOwner") => ObjectOwnerKind::ObjectOwner,
        _ => ObjectOwnerKind::Unknown,
    }
}

fn parse_initial_shared_version(owner: Option<&Value>) -> Option<u64> {
    let shared = owner?.as_object()?.get("Shared")?;

    shared
        .get("initial_shared_version")
        .or_else(|| shared.get("initialSharedVersion"))
        .and_then(value_to_u64)
}

fn value_to_u64(value: &Value) -> Option<u64> {
    match value {
        Value::Number(number) => {
            number.as_u64().or_else(|| number.as_i64().and_then(|value| u64::try_from(value).ok()))
        }
        Value::String(value) => value.parse::<u64>().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_shared_object_info() {
        let value = serde_json::json!({
            "data": {
                "objectId": "0xabc",
                "version": "123",
                "digest": "digest123",
                "type": "0xpackage::predict::Predict",
                "owner": {
                    "Shared": {
                        "initial_shared_version": "77"
                    }
                }
            }
        });

        let info =
            SuiObjectInfo::from_get_object_result("0xabc", value).expect("object info parses");

        assert_eq!(info.object_id, "0xabc");
        assert_eq!(info.version, Some(123));
        assert_eq!(info.digest.as_deref(), Some("digest123"));
        assert_eq!(info.owner_kind, ObjectOwnerKind::Shared);
        assert_eq!(info.initial_shared_version, Some(77));
        assert!(info.is_shared());
        assert!(info.has_shared_version());
    }

    #[test]
    fn parses_immutable_object_info() {
        let value = serde_json::json!({
            "data": {
                "objectId": "0xabc",
                "owner": "Immutable"
            }
        });

        let info =
            SuiObjectInfo::from_get_object_result("0xabc", value).expect("object info parses");

        assert_eq!(info.owner_kind, ObjectOwnerKind::Immutable);
        assert_eq!(info.initial_shared_version, None);
    }

    #[test]
    fn errors_on_missing_data() {
        let value = serde_json::json!({});

        let err = SuiObjectInfo::from_get_object_result("0xabc", value)
            .expect_err("missing data should fail");

        assert!(err.to_string().contains("missing `data` field"));
    }
}
