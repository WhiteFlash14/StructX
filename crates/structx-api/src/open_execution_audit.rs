use serde::{Deserialize, Serialize};

use crate::position_ledger::{LegKind, MintedLeg, PositionLedger};
use crate::storage;

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

pub async fn audit_open_execution(
    input: OpenExecutionAuditInput,
) -> anyhow::Result<OpenExecutionAuditOutcome> {
    validate_open_execution_input(&input)?;

    let artifact = serde_json::json!({
        "digest": input.tx_digest.clone(),
        "effects": input.execution_result.get("effects").cloned().unwrap_or(serde_json::Value::Null),
        "events": extract_events(&input.execution_result),
        "objectChanges": extract_object_changes(&input.execution_result),
    });

    let artifact_digest =
        artifact.get("digest").and_then(serde_json::Value::as_str).unwrap_or("unknown");
    let path = std::env::temp_dir()
        .join(format!("structx_audit_{}.json", storage::safe_component(artifact_digest)));
    std::fs::write(&path, serde_json::to_vec_pretty(&artifact)?)?;

    let execution_status = input
        .execution_result
        .get("effects")
        .and_then(|effects| effects.get("status"))
        .and_then(|status| status.get("status"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown")
        .to_ascii_lowercase();

    let minted_legs = parse_minted_legs_from_events(&extract_events(&input.execution_result));
    let total_cost_raw_num: u128 = minted_legs
        .iter()
        .filter_map(|leg| {
            leg.get("costRaw")
                .and_then(serde_json::Value::as_str)
                .and_then(|value| value.parse::<u128>().ok())
        })
        .sum();

    let compiled_strategy_id = input
        .raw_compiled_strategy
        .get("compiledStrategyId")
        .and_then(serde_json::Value::as_str)
        .map(|s| s.to_string());

    let oracle_id = input
        .raw_compiled_strategy
        .get("oracleId")
        .and_then(serde_json::Value::as_str)
        .map(|s| s.to_string())
        .or_else(|| input.intent_proposal.as_ref().map(|p| p.selected_market.oracle_id.clone()))
        .unwrap_or_else(|| "0x0".to_string());

    let compiled_expiry_ms = input
        .raw_compiled_strategy
        .get("expiryMs")
        .and_then(serde_json::Value::as_str)
        .map(|s| s.to_string())
        .or_else(|| input.intent_proposal.as_ref().map(|p| p.selected_market.expiry_ms.to_string()))
        .unwrap_or_else(|| "0".to_string());

    let strategy_label = input
        .raw_compiled_strategy
        .get("strategy")
        .and_then(serde_json::Value::as_str)
        .map(|s| s.to_string())
        .or_else(|| input.intent_proposal.as_ref().map(|p| p.backend_strategy_id.clone()));

    let mut warnings = Vec::new();
    let mut demo_status = serde_json::json!({
        "ok": false,
        "warnings": ["manager_id or user_address missing; skipped deep audit service"]
    });

    if !execution_succeeded(&input.execution_result) {
        warnings.push(
            "Transaction did not succeed, so StructX skipped canonical position ledger merge."
                .to_string(),
        );
    } else if minted_legs.is_empty() {
        warnings.push(
            "Transaction succeeded but no mint events were decoded, so no open position was merged into the ledger."
                .to_string(),
        );
    } else if let (Some(owner), Some(manager_id)) = (&input.user_address, &input.manager_id) {
        demo_status = structx_service::position_service::demo_status_json_value(
            Some(deepbook_client::DEFAULT_SUI_TESTNET_RPC_URL.to_string()),
            manager_id,
            owner,
            &path,
            false,
        )
        .await
        .map_err(|err| anyhow::anyhow!(err.to_string()))?;
    } else {
        warnings.push(
            "manager_id or user_address missing; skipped deep audit service and canonical ledger verification."
                .to_string(),
        );
    }

    let manager_balance_raw = demo_status
        .get("managerBalanceRaw")
        .and_then(serde_json::Value::as_str)
        .map(|s| s.to_string());
    let manager_balance_display = demo_status
        .get("managerBalanceDisplay")
        .and_then(serde_json::Value::as_str)
        .map(|s| s.to_string());

    let position_verification =
        demo_status.get("positionVerification").cloned().unwrap_or(serde_json::json!({
            "status": "unknown",
            "verifiedCount": 0,
            "mismatchCount": minted_legs.len(),
            "items": []
        }));

    if position_verification.get("status").and_then(serde_json::Value::as_str) == Some("partial") {
        warnings.push(
            "Position verification is partial. Range legs verified. Binary manager-key verification is a known issue under investigation."
                .to_string(),
        );
    }
    if let Some(service_warnings) =
        demo_status.get("warnings").and_then(serde_json::Value::as_array)
    {
        for warning in service_warnings {
            if let Some(text) = warning.as_str() {
                warnings.push(text.to_string());
            }
        }
    }

    let mut position_ids = Vec::new();
    let mut ledger_sync_status = if !execution_succeeded(&input.execution_result) {
        "transaction_failed".to_string()
    } else if minted_legs.is_empty() {
        "no_mint_events_found".to_string()
    } else {
        "execution_not_merged".to_string()
    };

    let audit_success = demo_status.get("ok").and_then(serde_json::Value::as_bool).unwrap_or(false)
        && execution_succeeded(&input.execution_result)
        && !minted_legs.is_empty();

    if audit_success {
        if let (Some(owner), Some(manager_id)) = (&input.user_address, &input.manager_id) {
            let opened_at = storage::unix_now();
            let mut ledger = match PositionLedger::load(owner, manager_id) {
                Ok(l) => l,
                Err(err) => {
                    warnings.push(format!(
                        "Could not load existing position ledger: {err}. New positions will start fresh."
                    ));
                    PositionLedger::empty(owner, manager_id)
                }
            };

            for raw in &minted_legs {
                if let Some(leg) = minted_leg_from_audit_json(
                    raw,
                    &oracle_id,
                    &compiled_expiry_ms,
                    &strategy_label,
                ) {
                    let position_id = PositionLedger::position_id(owner, manager_id, &leg);
                    position_ids.push(position_id);
                    ledger.apply_mint(&leg, &input.tx_digest, opened_at);
                }
            }

            if let Err(err) = ledger.save() {
                warnings.push(format!("Could not persist position ledger to disk: {err}."));
                ledger_sync_status = "ledger_save_failed".to_string();
            } else {
                ledger_sync_status = "merged_into_position_ledger".to_string();
            }

            let record = serde_json::json!({
                "schemaVersion": 1,
                "digest": input.tx_digest,
                "owner": owner,
                "managerId": manager_id,
                "compiledStrategyId": compiled_strategy_id,
                "oracleId": oracle_id,
                "expiryMs": compiled_expiry_ms,
                "strategy": strategy_label,
                "totalCostRaw": total_cost_raw_num.to_string(),
                "mintedLegs": minted_legs,
                "createdAtUnix": opened_at,
            });
            let record_path = storage::audit_record_path(&input.tx_digest);
            if let Err(err) = storage::atomic_write_json(&record_path, &record) {
                warnings.push(format!("Could not persist audit record to disk: {err}."));
            }
        } else {
            ledger_sync_status = "missing_owner_or_manager".to_string();
        }
    }

    Ok(OpenExecutionAuditOutcome {
        ok: audit_success,
        tx_digest: input.tx_digest.clone(),
        user_address: input.user_address,
        manager_id: input.manager_id,
        position_ids,
        ledger_sync_status,
        warnings,
        raw_audit_result: serde_json::json!({ "service": demo_status }),
        execution_status,
        explorer_url: format!(
            "https://suiexplorer.com/txblock/{}?network=testnet",
            input.tx_digest
        ),
        total_cost_raw: total_cost_raw_num.to_string(),
        total_cost_display: format_dusdc_raw_u128(total_cost_raw_num),
        minted_legs,
        position_verification,
        manager_balance_raw,
        manager_balance_display,
        artifact_path: path.to_string_lossy().to_string(),
        compiled_strategy_id,
    })
}

fn validate_open_execution_input(input: &OpenExecutionAuditInput) -> anyhow::Result<()> {
    if input.tx_digest.trim().is_empty() {
        anyhow::bail!("tx_digest is required");
    }
    if input.execution_result.is_null() {
        anyhow::bail!("execution_result is required");
    }
    if input.raw_compiled_strategy.is_null() {
        anyhow::bail!("raw_compiled_strategy is required");
    }
    Ok(())
}

fn execution_succeeded(raw: &serde_json::Value) -> bool {
    raw.get("effects")
        .and_then(|effects| effects.get("status"))
        .and_then(|status| status.get("status"))
        .and_then(serde_json::Value::as_str)
        .map(|status| status.eq_ignore_ascii_case("success"))
        .unwrap_or(false)
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

fn extract_events(raw: &serde_json::Value) -> Vec<serde_json::Value> {
    raw.get("events").and_then(|events| events.as_array()).cloned().unwrap_or_default()
}

fn extract_object_changes(raw: &serde_json::Value) -> Vec<serde_json::Value> {
    raw.get("objectChanges").and_then(|changes| changes.as_array()).cloned().unwrap_or_default()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_input(status: &str) -> OpenExecutionAuditInput {
        OpenExecutionAuditInput {
            source: OpenExecutionAuditSource::NormalModeIntent,
            proposal_id: Some("proposal_test".to_string()),
            user_address: Some("0xuser".to_string()),
            manager_id: Some("0xmanager".to_string()),
            tx_digest: "digest_test".to_string(),
            execution_result: serde_json::json!({
                "effects": {
                    "status": {
                        "status": status
                    }
                },
                "events": [],
                "objectChanges": []
            }),
            raw_compiled_strategy: serde_json::json!({
                "strategy": "test"
            }),
            intent_proposal: None,
        }
    }

    #[test]
    fn rejects_missing_digest() {
        let mut input = sample_input("success");
        input.tx_digest.clear();

        let err = validate_open_execution_input(&input).unwrap_err();
        assert!(err.to_string().contains("tx_digest"));
    }

    #[test]
    fn detects_successful_execution() {
        assert!(execution_succeeded(&sample_input("success").execution_result));
        assert!(!execution_succeeded(&sample_input("failure").execution_result));
    }

    #[tokio::test]
    async fn failed_transaction_skips_merge() {
        let outcome = audit_open_execution(sample_input("failure")).await.unwrap();

        assert!(!outcome.ok);
        assert_eq!(outcome.ledger_sync_status, "transaction_failed");
    }
}
