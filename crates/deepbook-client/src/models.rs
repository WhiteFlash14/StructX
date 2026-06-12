use chrono::{DateTime, TimeZone, Utc};
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

    #[serde(
        default,
        alias = "expiry",
        alias = "expiryMs",
        alias = "expiry_ms",
        alias = "expiryTimestampMs",
        deserialize_with = "deserialize_optional_i64"
    )]
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

#[derive(Debug, Clone, Serialize)]
pub struct OracleState {
    pub oracle_id: Option<String>,
    pub underlying_asset: Option<String>,
    pub status: Option<String>,
    pub expiry_ms: Option<i64>,
    pub min_strike: Option<u64>,
    pub max_strike: Option<u64>,
    pub tick_size: Option<u64>,
    pub raw: Value,
}

impl OracleState {
    #[must_use]
    pub fn from_value(raw: Value) -> Self {
        let body = unwrap_data_ref(&raw).unwrap_or(&raw);

        Self {
            oracle_id: find_string(
                body,
                &[
                    "oracle_id",
                    "oracleId",
                    "oracleID",
                    "id",
                    "object_id",
                    "objectId",
                ],
            ),
            underlying_asset: find_string(
                body,
                &["underlying_asset", "underlyingAsset", "underlying", "asset"],
            ),
            status: find_string(
                body,
                &["status", "state", "lifecycle", "lifecycle_status", "oracle_state"],
            ),
            expiry_ms: find_i64(
                body,
                &[
                    "expiry_ms",
                    "expiryMs",
                    "expiry",
                    "expiry_timestamp_ms",
                    "expiryTimestampMs",
                    "expiration_ms",
                    "expirationMs",
                ],
            )
            .and_then(normalize_epoch_millis),
            min_strike: find_u64(
                body,
                &[
                    "min_strike",
                    "minStrike",
                    "min_strike_price",
                    "minStrikePrice",
                    "minimum_strike",
                    "minimumStrike",
                ],
            ),
            max_strike: find_u64(
                body,
                &[
                    "max_strike",
                    "maxStrike",
                    "max_strike_price",
                    "maxStrikePrice",
                    "maximum_strike",
                    "maximumStrike",
                ],
            ),
            tick_size: find_u64(
                body,
                &[
                    "tick_size",
                    "tickSize",
                    "strike_tick_size",
                    "strikeTickSize",
                    "strike_step",
                    "strikeStep",
                ],
            ),
            raw,
        }
    }

    #[must_use]
    pub fn is_active_or_live(&self) -> bool {
        is_active_or_live(self.status.as_deref())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LatestPrice {
    pub timestamp_ms: Option<i64>,
    pub price: Option<f64>,
    pub raw: Value,
}

impl LatestPrice {
    #[must_use]
    pub fn from_value(raw: Value) -> Self {
        let body = unwrap_data_ref(&raw).unwrap_or(&raw);

        Self {
            timestamp_ms: find_i64(
                body,
                &[
                    "timestamp_ms",
                    "timestampMs",
                    "time_ms",
                    "timeMs",
                    "updated_at_ms",
                    "updatedAtMs",
                    "created_at_ms",
                    "createdAtMs",
                    "timestamp",
                    "time",
                ],
            )
            .and_then(normalize_epoch_millis),
            price: find_f64(
                body,
                &[
                    "price",
                    "spot",
                    "spot_price",
                    "spotPrice",
                    "index_price",
                    "indexPrice",
                ],
            ),
            raw,
        }
    }

    #[must_use]
    pub fn timestamp_datetime(&self) -> Option<DateTime<Utc>> {
        self.timestamp_ms
            .and_then(|ms| Utc.timestamp_millis_opt(ms).single())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LatestSvi {
    pub timestamp_ms: Option<i64>,
    pub spot: Option<f64>,
    pub forward: Option<f64>,
    pub raw: Value,
}

impl LatestSvi {
    #[must_use]
    pub fn from_value(raw: Value) -> Self {
        let body = unwrap_data_ref(&raw).unwrap_or(&raw);

        Self {
            timestamp_ms: find_i64(
                body,
                &[
                    "timestamp_ms",
                    "timestampMs",
                    "time_ms",
                    "timeMs",
                    "updated_at_ms",
                    "updatedAtMs",
                    "created_at_ms",
                    "createdAtMs",
                    "timestamp",
                    "time",
                ],
            )
            .and_then(normalize_epoch_millis),
            spot: find_f64(body, &["spot", "spot_price", "spotPrice"]),
            forward: find_f64(body, &["forward", "forward_price", "forwardPrice"]),
            raw,
        }
    }

    #[must_use]
    pub fn timestamp_datetime(&self) -> Option<DateTime<Utc>> {
        self.timestamp_ms
            .and_then(|ms| Utc.timestamp_millis_opt(ms).single())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AskBounds {
    pub raw: Value,
}

impl AskBounds {
    #[must_use]
    pub fn from_value(raw: Value) -> Self {
        Self { raw }
    }

    #[must_use]
    pub fn exists(&self) -> bool {
        !self.raw.is_null()
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

fn unwrap_data_ref(value: &Value) -> Option<&Value> {
    value.as_object().and_then(|map| map.get("data"))
}

fn is_active_or_live(status: Option<&str>) -> bool {
    status
        .map(|s| {
            let normalized = s.trim().to_ascii_lowercase();
            normalized == "active" || normalized == "live"
        })
        .unwrap_or(false)
}

fn find_string(value: &Value, keys: &[&str]) -> Option<String> {
    find_value(value, keys).and_then(|v| match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    })
}

fn find_i64(value: &Value, keys: &[&str]) -> Option<i64> {
    find_value(value, keys).and_then(value_to_i64)
}

fn find_u64(value: &Value, keys: &[&str]) -> Option<u64> {
    find_value(value, keys).and_then(value_to_u64)
}

fn find_f64(value: &Value, keys: &[&str]) -> Option<f64> {
    find_value(value, keys).and_then(value_to_f64)
}

fn find_value<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    match value {
        Value::Object(map) => {
            for wanted in keys {
                if let Some(found) = map.get(*wanted) {
                    return Some(found);
                }
            }

            for nested in map.values() {
                if let Some(found) = find_value(nested, keys) {
                    return Some(found);
                }
            }

            None
        }
        Value::Array(items) => items.iter().find_map(|item| find_value(item, keys)),
        _ => None,
    }
}

fn value_to_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Number(n) => n.as_i64().or_else(|| n.as_u64().and_then(|v| i64::try_from(v).ok())),
        Value::String(s) => {
            let trimmed = s.trim();

            trimmed
                .parse::<i64>()
                .ok()
                .or_else(|| DateTime::parse_from_rfc3339(trimmed).ok().map(|dt| dt.timestamp_millis()))
        }
        _ => None,
    }
}

fn value_to_u64(value: &Value) -> Option<u64> {
    match value {
        Value::Number(n) => n.as_u64().or_else(|| n.as_i64().and_then(|v| u64::try_from(v).ok())),
        Value::String(s) => s.trim().parse::<u64>().ok(),
        _ => None,
    }
}

fn value_to_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

fn normalize_epoch_millis(value: i64) -> Option<i64> {
    if value <= 0 {
        return None;
    }

    if value < 10_000_000_000 {
        return value.checked_mul(1_000);
    }

    if value < 10_000_000_000_000 {
        return Some(value);
    }

    if value < 10_000_000_000_000_000 {
        return Some(value / 1_000);
    }

    Some(value / 1_000_000)
}

fn deserialize_optional_i64<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;

    Ok(value.as_ref().and_then(value_to_i64).and_then(normalize_epoch_millis))
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

    #[test]
    fn oracle_state_extracts_nested_grid_fields() {
        let raw = serde_json::json!({
            "data": {
                "oracle": {
                    "id": "0xabc",
                    "underlying_asset": "BTC",
                    "status": "active",
                    "expiry_ms": 1_900_000_000_000_i64
                },
                "config": {
                    "min_strike": "50000000000000",
                    "tick_size": "1000000000"
                }
            }
        });

        let state = OracleState::from_value(raw);

        assert_eq!(state.oracle_id.as_deref(), Some("0xabc"));
        assert_eq!(state.min_strike, Some(50_000_000_000_000));
        assert_eq!(state.tick_size, Some(1_000_000_000));
    }
}