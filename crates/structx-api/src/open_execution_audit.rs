use serde::{Deserialize, Serialize};

use crate::position_ledger::{LegKind, MintedLeg};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OpenExecutionAuditSource {
    AdvancedMode,
    NormalModeIntent,
    ChainSync,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenExecutionAuditInput {
    pub source: OpenExecutionAuditSource,
    pub proposal_id: Option<String>,
    pub user_address: Option<String>,
    pub manager_id: Option<String>,
    pub tx_digest: String,
    pub execution_result: serde_json::Value,
    pub raw_compiled_strategy: serde_json::Value,
    pub intent_proposal: Option<structx_service::ExecutionProposal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenExecutionAuditOutcome {
    pub ok: bool,
    pub tx_digest: String,
    pub user_address: Option<String>,
    pub manager_id: Option<String>,
    pub position_ids: Vec<String>,
    pub ledger_sync_status: String,
    pub warnings: Vec<String>,
    pub raw_audit_result: serde_json::Value,
    pub execution_status: String,
    pub explorer_url: String,
    pub total_cost_raw: String,
    pub total_cost_display: String,
    pub minted_legs: Vec<serde_json::Value>,
    pub position_verification: serde_json::Value,
    pub manager_balance_raw: Option<String>,
    pub manager_balance_display: Option<String>,
    pub artifact_path: String,
    pub compiled_strategy_id: Option<String>,
}

pub fn minted_leg_from_audit_json(
    leg: &serde_json::Value,
    oracle_id: &str,
    expiry_ms: &str,
    strategy: &Option<String>,
) -> Option<MintedLeg> {
    let kind_str = leg.get("kind").and_then(serde_json::Value::as_str)?;
    let kind = match kind_str {
        "DOWN" => LegKind::Down,
        "UP" => LegKind::Up,
        "RANGE" => LegKind::Range,
        _ => return None,
    };
    let direction = leg.get("direction").and_then(serde_json::Value::as_str).map(|s| s.to_string());
    let strike_raw = leg
        .get("strikeRaw")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let lower_raw = leg
        .get("lowerRaw")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let upper_raw = leg
        .get("upperRaw")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let quantity_raw = leg
        .get("quantityRaw")
        .and_then(serde_json::Value::as_str)
        .and_then(|s| s.parse::<u128>().ok())
        .unwrap_or(0);
    let cost_raw = leg
        .get("costRaw")
        .and_then(serde_json::Value::as_str)
        .and_then(|s| s.parse::<u128>().ok())
        .unwrap_or(0);
    let event_oracle_id = leg
        .get("oracleId")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty() && *s != "0x0");
    let event_expiry_ms = leg
        .get("expiryMs")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty() && *s != "0");
    Some(MintedLeg {
        kind,
        direction,
        oracle_id: event_oracle_id.unwrap_or(oracle_id).to_string(),
        expiry_ms: event_expiry_ms.unwrap_or(expiry_ms).to_string(),
        strike_raw,
        lower_raw,
        upper_raw,
        quantity_raw,
        cost_raw,
        role: None,
        strategy: strategy.clone(),
    })
}

pub fn parse_minted_legs_from_events(events: &[serde_json::Value]) -> Vec<serde_json::Value> {
    let mut legs = Vec::new();

    for event in events {
        let event_type = event.get("type").and_then(serde_json::Value::as_str).unwrap_or("");
        let parsed = event.get("parsedJson").cloned().unwrap_or(serde_json::Value::Null);
        let index = legs.len();
        let event_oracle_id =
            parsed.get("oracle_id").and_then(serde_json::Value::as_str).unwrap_or("").to_string();
        let event_expiry_ms = parsed
            .get("expiry")
            .and_then(json_value_as_u128_string)
            .unwrap_or_else(|| "0".to_string());

        if event_type.ends_with("::predict::PositionMinted") {
            let quantity_raw = parsed
                .get("quantity")
                .and_then(json_value_as_u128_string)
                .unwrap_or_else(|| "0".to_string());
            let cost_raw = parsed
                .get("cost")
                .and_then(json_value_as_u128_string)
                .unwrap_or_else(|| "0".to_string());
            let strike_raw = parsed
                .get("strike")
                .and_then(json_value_as_u128_string)
                .unwrap_or_else(|| "0".to_string());
            let is_up = parsed.get("is_up").and_then(serde_json::Value::as_bool).unwrap_or(false);

            legs.push(serde_json::json!({
                "index": index,
                "event": "PositionMinted",
                "kind": if is_up { "UP" } else { "DOWN" },
                "direction": if is_up { "up" } else { "down" },
                "oracleId": event_oracle_id,
                "expiryMs": event_expiry_ms,
                "strike": format_raw_price_e9_str(&strike_raw),
                "strikeRaw": strike_raw,
                "lower": serde_json::Value::Null,
                "upper": serde_json::Value::Null,
                "quantityRaw": quantity_raw.clone(),
                "quantityDisplay": format_dusdc_raw_str(&quantity_raw),
                "costRaw": cost_raw.clone(),
                "costDisplay": format_dusdc_raw_str(&cost_raw)
            }));
        } else if event_type.ends_with("::predict::RangeMinted") {
            let quantity_raw = parsed
                .get("quantity")
                .and_then(json_value_as_u128_string)
                .unwrap_or_else(|| "0".to_string());
            let cost_raw = parsed
                .get("cost")
                .and_then(json_value_as_u128_string)
                .unwrap_or_else(|| "0".to_string());
            let lower_raw = parsed
                .get("lower")
                .and_then(json_value_as_u128_string)
                .unwrap_or_else(|| "0".to_string());
            let upper_raw = parsed
                .get("upper")
                .and_then(json_value_as_u128_string)
                .unwrap_or_else(|| "0".to_string());

            legs.push(serde_json::json!({
                "index": index,
                "event": "RangeMinted",
                "kind": "RANGE",
                "direction": serde_json::Value::Null,
                "oracleId": event_oracle_id,
                "expiryMs": event_expiry_ms,
                "strike": serde_json::Value::Null,
                "strikeRaw": serde_json::Value::Null,
                "lower": format_raw_price_e9_str(&lower_raw),
                "lowerRaw": lower_raw,
                "upper": format_raw_price_e9_str(&upper_raw),
                "upperRaw": upper_raw,
                "quantityRaw": quantity_raw.clone(),
                "quantityDisplay": format_dusdc_raw_str(&quantity_raw),
                "costRaw": cost_raw.clone(),
                "costDisplay": format_dusdc_raw_str(&cost_raw)
            }));
        }
    }

    legs
}

fn json_value_as_u128_string(value: &serde_json::Value) -> Option<String> {
    if let Some(s) = value.as_str() {
        Some(s.to_string())
    } else if let Some(n) = value.as_u64() {
        Some(n.to_string())
    } else if let Some(n) = value.as_i64() {
        u128::try_from(n).ok().map(|v| v.to_string())
    } else {
        None
    }
}

fn format_dusdc_raw_u128(raw: u128) -> String {
    let whole = raw / 1_000_000;
    let frac = raw % 1_000_000;
    if frac == 0 {
        format!("{whole} dUSDC")
    } else {
        let mut frac_string = format!("{frac:06}");
        while frac_string.ends_with('0') {
            frac_string.pop();
        }
        format!("{whole}.{frac_string} dUSDC")
    }
}

fn format_dusdc_raw_str(raw: &str) -> String {
    raw.parse::<u128>().map(format_dusdc_raw_u128).unwrap_or_else(|_| "0 dUSDC".to_string())
}

fn format_raw_price_e9_str(raw: &str) -> String {
    let raw = raw.parse::<u128>().unwrap_or(0);
    let whole = raw / 1_000_000_000;
    let frac = raw % 1_000_000_000;
    if frac == 0 {
        whole.to_string()
    } else {
        let mut frac_string = format!("{frac:09}");
        while frac_string.ends_with('0') {
            frac_string.pop();
        }
        format!("{whole}.{frac_string}")
    }
}
