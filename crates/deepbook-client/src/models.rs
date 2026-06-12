use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerStatus {
    #[serde(flatten)]
    pub raw: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictState {
    #[serde(flatten)]
    pub raw: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuoteAsset {
    #[serde(default, alias = "type", alias = "coin_type", alias = "coinType")]
    pub coin_type: Option<String>,

    #[serde(default, alias = "symbol", alias = "asset", alias = "name")]
    pub symbol: Option<String>,

    #[serde(default)]
    pub decimals: Option<u8>,

    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultSummary {
    #[serde(flatten)]
    pub raw: Map<String, Value>,
}

impl VaultSummary {
    #[must_use]
    pub fn is_present(&self) -> bool {
        !self.raw.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleListItem {
    #[serde(
        default,
        alias = "oracleId",
        alias = "oracleID",
        alias = "id",
        alias = "object_id",
        alias = "objectId"
    )]
    pub oracle_id: Option<String>,

    #[serde(
        default,
        alias = "underlyingAsset",
        alias = "underlying",
        alias = "asset"
    )]
    pub underlying_asset: Option<String>,

    #[serde(default, alias = "state", alias = "lifecycle", alias = "oracle_state")]
    pub status: Option<String>,

    #[serde(default, alias = "expiry", alias = "expiryMs", alias = "expiry_ms")]
    pub expiry_ms: Option<i64>,

    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl OracleListItem {
    #[must_use]
    pub fn is_btc(&self) -> bool {
        self.underlying_asset
            .as_deref()
            .map(|value| value.eq_ignore_ascii_case("BTC"))
            .unwrap_or(false)
    }

    #[must_use]
    pub fn is_active_or_live(&self) -> bool {
        is_active_or_live(self.status.as_deref())
    }
}

pub fn parse_oracle_list_from_value(value: Value) -> Result<Vec<OracleListItem>, serde_json::Error> {
    let body = unwrap_data_owned(value);
    serde_json::from_value(body)
}

pub fn parse_quote_assets_from_value(value: Value) -> Result<Vec<QuoteAsset>, serde_json::Error> {
    let body = unwrap_data_owned(value);
    serde_json::from_value(body)
}

fn unwrap_data_owned(value: Value) -> Value {
    match value {
        Value::Object(mut map) => map.remove("data").unwrap_or(Value::Object(map)),
        other => other,
    }
}

fn is_active_or_live(status: Option<&str>) -> bool {
    status
        .map(|s| {
            let normalized = s.trim().to_ascii_lowercase();
            normalized == "active" || normalized == "live"
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_oracle_ids_from_wrapped_data() {
        let value = serde_json::json!({
            "data": [
                {
                    "oracle_id": "0xabc",
                    "underlying_asset": "BTC",
                    "status": "active"
                }
            ]
        });

        let parsed = parse_oracle_list_from_value(value).expect("oracle list parses");

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].oracle_id.as_deref(), Some("0xabc"));
        assert!(parsed[0].is_btc());
        assert!(parsed[0].is_active_or_live());
    }

    #[test]
    fn missing_optional_fields_do_not_fail_deserialization() {
        let value = serde_json::json!([
            {
                "oracle_id": "0xabc"
            }
        ]);

        let parsed = parse_oracle_list_from_value(value).expect("oracle list parses");

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].oracle_id.as_deref(), Some("0xabc"));
        assert_eq!(parsed[0].underlying_asset, None);
        assert_eq!(parsed[0].status, None);
        assert_eq!(parsed[0].expiry_ms, None);
    }

    #[test]
    fn parses_quote_assets_from_wrapped_data() {
        let value = serde_json::json!({
            "data": [
                {
                    "coinType": "0x2::sui::SUI",
                    "symbol": "SUI",
                    "decimals": 9
                }
            ]
        });

        let parsed = parse_quote_assets_from_value(value).expect("quote assets parse");

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].symbol.as_deref(), Some("SUI"));
        assert_eq!(parsed[0].decimals, Some(9));
    }
}
