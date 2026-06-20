use std::env;

use anyhow::anyhow;
use deepbook_client::{DEFAULT_SUI_TESTNET_RPC_URL, PREDICT_OBJECT_ID, PREDICT_SERVER_URL};
use serde_json::Value;
use uuid::Uuid;

use crate::intent::{IntentConfidence, RiskStyle, StrategyTemplateId};
use crate::intent_proposal::{
    CompiledProposalLeg, ExecutionProposal, PayoffRow, ProposalQuoteMetadata,
    QuoteIntentPlanRequest,
};
use crate::market_catalog::{now_ms, CatalogMarketSnapshot, ExpiryPreference, MarketSearchQuery};
use crate::market_store::MarketStore;
use crate::{CompileStrategyJsonArgs, DisplayPrice};

const DEFAULT_MAX_QUOTE_AGE_MS: u64 = 15_000;

pub async fn quote_intent_plan<S: MarketStore + ?Sized>(
    store: &S,
    request: QuoteIntentPlanRequest,
) -> anyhow::Result<ExecutionProposal> {
    let budget = request
        .budget
        .or(request.intent_plan.budget)
        .ok_or_else(|| anyhow!("budget is required to quote intent plan"))?;

    if budget == 0 {
        return Err(anyhow!("budget must be greater than zero"));
    }

    let requested_market = resolve_selected_market(store, &request).await?;
    validate_market_for_template(&requested_market, &request.intent_plan.strategy_template)?;

    let backend_strategy_id = map_template_to_backend_strategy(
        &request.intent_plan.strategy_template,
        &request.intent_plan.risk_style,
    );
    let candidate_markets = compile_candidate_markets(store, &request, &requested_market).await?;
    let all_catalog_oracle_ids = load_all_catalog_oracle_ids(store).await?;

    let mut selected_market = requested_market.clone();
    let mut raw_compiled_strategy = None;
    let mut compile_failures = Vec::new();

    for candidate in candidate_markets {
        let compile_args = build_compile_args(
            &request,
            &backend_strategy_id,
            budget,
            build_excluded_oracle_ids(&all_catalog_oracle_ids, &candidate.oracle_id),
        )?;

        match compile_with_existing_service(compile_args).await {
            Ok(compiled) => {
                selected_market = candidate;
                raw_compiled_strategy = Some(compiled);
                break;
            }
            Err(err) => {
                compile_failures.push(format!("{}: {}", candidate.oracle_id, err));
            }
        }
    }

    let raw_compiled_strategy = raw_compiled_strategy.ok_or_else(|| {
        anyhow!(
            "failed to compile strategy through existing StructX service; attempts: {}",
            compile_failures.join(" | ")
        )
    })?;

    let legs = extract_proposal_legs(&raw_compiled_strategy, &selected_market);
    let total_premium = extract_u64_any(
        &raw_compiled_strategy,
        &[
            "premiumRequiredRaw",
            "total_premium",
            "totalPremium",
            "premium",
            "estimated_premium",
            "estimatedPremium",
            "cost",
            "max_cost",
            "maxCost",
        ],
    )
    .unwrap_or_else(|| budget_nanos_to_protocol_raw(budget));

    let max_payout = extract_u64_any(
        &raw_compiled_strategy,
        &[
            "maxGrossPayoutRaw",
            "max_payout",
            "maxPayout",
            "gross_max_payout",
            "grossMaxPayout",
        ],
    )
    .unwrap_or_else(|| {
        legs.iter()
            .map(|leg| leg.quantity)
            .max()
            .unwrap_or_default()
    });

    let max_loss = extract_u64_any(
        &raw_compiled_strategy,
        &["maxLossRaw", "max_loss", "maxLoss"],
    )
    .unwrap_or(total_premium);

    let payoff_table = extract_payoff_rows(&raw_compiled_strategy)
        .unwrap_or_else(|| fallback_payoff_table(&legs, total_premium));

    let net_pnl_table = payoff_table
        .iter()
        .map(|row| PayoffRow {
            label: row.label.clone(),
            settlement_lower: row.settlement_lower,
            settlement_upper: row.settlement_upper,
            gross_payout: row.gross_payout,
            net_pnl: row.net_pnl,
        })
        .collect();

    let quoted_at_ms = now_ms();
    let max_quote_age_ms = request.max_quote_age_ms.unwrap_or(DEFAULT_MAX_QUOTE_AGE_MS);

    let mut warnings = request.intent_plan.warnings.clone();
    warnings.push(
        "This proposal is terminal-expiry based. It pays according to settlement, not whether the market touches a level before expiry."
            .to_string(),
    );
    if request.intent_plan.confidence != IntentConfidence::High {
        warnings.push(format!(
            "Intent confidence is {:?}; review the selected market and legs carefully before signing.",
            request.intent_plan.confidence
        ));
    }
    if !selected_market
        .preferred_quote_asset
        .eq_ignore_ascii_case(&request.intent_plan.quote_asset)
    {
        warnings.push(format!(
            "Selected market prefers quote asset {} but intent requested {}.",
            selected_market.preferred_quote_asset, request.intent_plan.quote_asset
        ));
    }
    if selected_market.oracle_id != requested_market.oracle_id {
        warnings.push(format!(
            "The initially selected market could not be quoted cleanly, so StructX fell back to oracle {} for the final proposal.",
            selected_market.oracle_id
        ));
    }

    Ok(ExecutionProposal {
        proposal_id: format!("proposal_{}", Uuid::new_v4()),
        user_address: request.user_address,
        raw_prompt: request.intent_plan.raw_prompt.clone(),
        selected_market: selected_market.clone(),
        reason_for_selection: format!(
            "Selected active {} market matching '{}'.",
            selected_market.underlying, request.intent_plan.market_query
        ),
        strategy_template: request.intent_plan.strategy_template,
        backend_strategy_id,
        legs,
        total_premium,
        max_loss,
        max_payout,
        payoff_table,
        net_pnl_table,
        quote_metadata: ProposalQuoteMetadata {
            quote_batch_id: format!("quote_{}", Uuid::new_v4()),
            quoted_at_ms,
            max_quote_age_ms,
            source: "structx_service_compile_strategy_json_value".to_string(),
            oracle_id: selected_market.oracle_id.clone(),
            market_fetched_at_ms: selected_market.fetched_at_ms,
        },
        assumptions: request.intent_plan.assumptions,
        warnings,
        requires_user_signature: true,
        raw_compiled_strategy,
    })
}

async fn resolve_selected_market<S: MarketStore + ?Sized>(
    store: &S,
    request: &QuoteIntentPlanRequest,
) -> anyhow::Result<CatalogMarketSnapshot> {
    if let Some(ref market_id) = request.selected_market_id {
        return store
            .get_market(market_id)
            .await?
            .ok_or_else(|| anyhow!("selected market not found in catalog: {market_id}"));
    }

    let query = MarketSearchQuery {
        text: request.intent_plan.market_query.clone(),
        category_hint: request.intent_plan.category_hint.clone(),
        market_kind_hint: request.intent_plan.market_kind_hint.clone(),
        require_active: true,
        quote_asset: Some(request.intent_plan.quote_asset.clone()),
        expiry_preference: Some(ExpiryPreference::NearestActive),
    };

    let candidates = store.search_markets(query).await?;
    candidates.into_iter().next().ok_or_else(|| {
        anyhow!(
            "no active market found for '{}'",
            request.intent_plan.market_query
        )
    })
}

async fn compile_candidate_markets<S: MarketStore + ?Sized>(
    store: &S,
    request: &QuoteIntentPlanRequest,
    requested_market: &CatalogMarketSnapshot,
) -> anyhow::Result<Vec<CatalogMarketSnapshot>> {
    let query = MarketSearchQuery {
        text: request.intent_plan.market_query.clone(),
        category_hint: request.intent_plan.category_hint.clone(),
        market_kind_hint: None,
        require_active: true,
        quote_asset: Some(request.intent_plan.quote_asset.clone()),
        expiry_preference: Some(ExpiryPreference::NearestActive),
    };

    let mut candidates = store.search_markets(query).await?;
    candidates.retain(|market| market.underlying == requested_market.underlying);
    candidates.sort_by_key(|a| a.expiry_ms);

    let mut ordered = Vec::with_capacity(candidates.len().saturating_add(1));
    ordered.push(requested_market.clone());
    for candidate in candidates {
        if candidate.oracle_id != requested_market.oracle_id {
            ordered.push(candidate);
        }
    }

    Ok(ordered)
}

fn validate_market_for_template(
    market: &CatalogMarketSnapshot,
    template: &StrategyTemplateId,
) -> anyhow::Result<()> {
    if !market.is_active() {
        return Err(anyhow!("selected market is not active"));
    }

    match template {
        StrategyTemplateId::DirectionalAbove
        | StrategyTemplateId::DirectionalBelow
        | StrategyTemplateId::RangeInside
        | StrategyTemplateId::BreakoutOutside
        | StrategyTemplateId::OneSidedTail
        | StrategyTemplateId::UpsideRocket
        | StrategyTemplateId::CustomPiecewise
        | StrategyTemplateId::SmartBudget => Ok(()),
    }
}

fn map_template_to_backend_strategy(
    template: &StrategyTemplateId,
    risk_style: &RiskStyle,
) -> String {
    match template {
        StrategyTemplateId::DirectionalAbove => "MOONSHOT_UPSIDE".to_string(),
        StrategyTemplateId::DirectionalBelow => "PORTFOLIO_CRASH_SHIELD".to_string(),
        StrategyTemplateId::RangeInside => "CENTER_BAND_CONDOR".to_string(),
        StrategyTemplateId::BreakoutOutside => "BREAKOUT_PROTECTION".to_string(),
        StrategyTemplateId::OneSidedTail => "PORTFOLIO_CRASH_SHIELD".to_string(),
        StrategyTemplateId::UpsideRocket => "MOONSHOT_UPSIDE".to_string(),
        StrategyTemplateId::CustomPiecewise => "SMART_BUDGET_SELECTOR".to_string(),
        StrategyTemplateId::SmartBudget => match risk_style {
            RiskStyle::Conservative | RiskStyle::HigherHitRate => {
                "SMART_BUDGET_SELECTOR".to_string()
            }
            RiskStyle::Aggressive | RiskStyle::TailHeavy => "CONVEX_TAIL_LADDER".to_string(),
            RiskStyle::Balanced => "SMART_BUDGET_SELECTOR".to_string(),
        },
    }
}

fn build_compile_args(
    request: &QuoteIntentPlanRequest,
    backend_strategy_id: &str,
    budget_nanos: u64,
    exclude_oracle_ids: Vec<String>,
) -> anyhow::Result<CompileStrategyJsonArgs> {
    let style = match request.intent_plan.risk_style {
        RiskStyle::Conservative => "higher-hit-rate",
        RiskStyle::Balanced => "balanced",
        RiskStyle::Aggressive => "tail-heavy",
        RiskStyle::TailHeavy => "tail-heavy",
        RiskStyle::HigherHitRate => "higher-hit-rate",
    };

    Ok(CompileStrategyJsonArgs {
        server_url: env::var("STRUCTX_PREDICT_SERVER_URL")
            .unwrap_or_else(|_| PREDICT_SERVER_URL.to_string()),
        predict_id: env::var("STRUCTX_PREDICT_ID")
            .unwrap_or_else(|_| PREDICT_OBJECT_ID.to_string()),
        rpc_url: env::var("STRUCTX_RPC_URL")
            .unwrap_or_else(|_| DEFAULT_SUI_TESTNET_RPC_URL.to_string()),
        owner: request
            .user_address
            .clone()
            .unwrap_or_else(|| "0x0".to_string()),
        strategy: backend_strategy_id.to_string(),
        budget_dusdc: nanos_to_display_dusdc(budget_nanos),
        style: style.to_string(),
        expiry_preference: "nearest_active".to_string(),
        slippage_bps: 500,
        bucket_step: DisplayPrice(250.0),
        custom_k1_price: None,
        custom_k2_price: None,
        custom_k3_price: None,
        custom_k4_price: None,
        levels_each_side: 4,
        max_quote_market_attempts: 1,
        portfolio_exposure_dusdc: 5_000.0,
        over_hedge_cap_bps: 12_000,
        convex_gamma_bps: 15_000,
        dead_zone_bps: 200,
        moonshot_range_weight_bps: 6_000,
        moonshot_tail_gamma_bps: 15_000,
        downside_range_weight_bps: 6_000,
        downside_tail_gamma_bps: 15_000,
        upside_near_range_weight_bps: 4_000,
        upside_upper_range_weight_bps: 3_500,
        upside_tail_gamma_bps: 15_000,
        downside_near_range_weight_bps: 4_000,
        downside_lower_range_weight_bps: 3_500,
        downside_step_tail_gamma_bps: 15_000,
        condor_center_weight_bps: 6_000,
        barrier_side: "up".to_string(),
        barrier_near_range_weight_bps: 7_000,
        barrier_tail_gamma_bps: 15_000,
        exclude_oracle_ids,
    })
}

async fn load_all_catalog_oracle_ids<S: MarketStore + ?Sized>(
    store: &S,
) -> anyhow::Result<Vec<String>> {
    Ok(store
        .load_latest_catalog()
        .await?
        .map(|catalog| {
            catalog
                .markets
                .into_iter()
                .map(|market| market.oracle_id)
                .collect()
        })
        .unwrap_or_default())
}

fn build_excluded_oracle_ids(
    all_catalog_oracle_ids: &[String],
    target_oracle_id: &str,
) -> Vec<String> {
    all_catalog_oracle_ids
        .iter()
        .filter(|oracle_id| oracle_id.as_str() != target_oracle_id)
        .cloned()
        .collect()
}

async fn compile_with_existing_service(args: CompileStrategyJsonArgs) -> anyhow::Result<Value> {
    crate::compile_strategy_json_value(args)
        .await
        .map_err(|err| anyhow!(err.to_string()))
}

fn extract_proposal_legs(raw: &Value, market: &CatalogMarketSnapshot) -> Vec<CompiledProposalLeg> {
    let leg_arrays = [
        "legs",
        "compiled_legs",
        "compiledLegs",
        "strategy.legs",
        "quote_plan.legs",
        "quotePlan.legs",
    ];

    for path in leg_arrays {
        if let Some(value) = get_path(raw, path) {
            if let Some(arr) = value.as_array() {
                let legs: Vec<CompiledProposalLeg> = arr
                    .iter()
                    .map(|item| {
                        let kind = extract_string_any(
                            item,
                            &[
                                "kind",
                                "leg_kind",
                                "legKind",
                                "type",
                                "position_type",
                                "positionType",
                            ],
                        )
                        .unwrap_or_else(|| "unknown".to_string());

                        CompiledProposalLeg {
                            kind,
                            oracle_id: market.oracle_id.clone(),
                            expiry_ms: market.expiry_ms,
                            strike: extract_u64_any(item, &["strikeRaw", "strike", "k"]),
                            lower: extract_u64_any(
                                item,
                                &["lowerRaw", "lower", "lower_strike", "lowerStrike"],
                            ),
                            upper: extract_u64_any(
                                item,
                                &["upperRaw", "upper", "upper_strike", "upperStrike"],
                            ),
                            quantity: extract_u64_any(
                                item,
                                &["quantityRaw", "quantity", "qty", "size", "payout"],
                            )
                            .unwrap_or_default(),
                            ask_price: extract_u64_any(
                                item,
                                &["askPriceRaw", "ask", "ask_price", "askPrice"],
                            ),
                            premium: extract_u64_any(
                                item,
                                &["premiumRaw", "premium", "cost", "max_cost", "maxCost"],
                            ),
                            role: extract_string_any(item, &["role"]),
                            label: extract_string_any(item, &["label", "display", "kind"]),
                        }
                    })
                    .collect();

                if !legs.is_empty() {
                    return legs;
                }
            }
        }
    }

    vec![]
}

fn extract_payoff_rows(raw: &Value) -> Option<Vec<PayoffRow>> {
    let value = get_path(raw, "payoff_table")
        .or_else(|| get_path(raw, "payoffTable"))
        .or_else(|| get_path(raw, "net_pnl_table"))
        .or_else(|| get_path(raw, "netPnlTable"))?;

    let arr = value.as_array()?;

    let rows: Vec<PayoffRow> = arr
        .iter()
        .map(|item| PayoffRow {
            label: extract_string_any(item, &["label", "bucket", "region", "condition"])
                .unwrap_or_else(|| "scenario".to_string()),
            settlement_lower: extract_f64_any(
                item,
                &["settlement_lower", "settlementLower", "lower"],
            ),
            settlement_upper: extract_f64_any(
                item,
                &["settlement_upper", "settlementUpper", "upper"],
            ),
            gross_payout: extract_u64_any(
                item,
                &["grossPayoutRaw", "gross_payout", "grossPayout", "payout"],
            )
            .unwrap_or_default(),
            net_pnl: extract_i128_any(item, &["netPnlRaw", "net_pnl", "netPnl", "pnl"])
                .unwrap_or_default(),
        })
        .collect();

    Some(rows)
}

fn fallback_payoff_table(legs: &[CompiledProposalLeg], total_premium: u64) -> Vec<PayoffRow> {
    if legs.is_empty() {
        return vec![PayoffRow {
            label: "No executable legs extracted".to_string(),
            settlement_lower: None,
            settlement_upper: None,
            gross_payout: 0,
            net_pnl: -(total_premium as i128),
        }];
    }

    legs.iter()
        .map(|leg| PayoffRow {
            label: leg
                .label
                .clone()
                .unwrap_or_else(|| format!("{} leg", leg.kind)),
            settlement_lower: leg.lower.map(|v| v as f64),
            settlement_upper: leg.upper.or(leg.strike).map(|v| v as f64),
            gross_payout: leg.quantity,
            net_pnl: leg.quantity as i128 - total_premium as i128,
        })
        .collect()
}

fn extract_string_any(raw: &Value, paths: &[&str]) -> Option<String> {
    paths.iter().find_map(|path| {
        let value = get_path(raw, path)?;
        value
            .as_str()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    })
}

fn extract_u64_any(raw: &Value, paths: &[&str]) -> Option<u64> {
    paths
        .iter()
        .find_map(|path| parse_u64(get_path(raw, path)?))
}

fn extract_i128_any(raw: &Value, paths: &[&str]) -> Option<i128> {
    paths.iter().find_map(|path| {
        let value = get_path(raw, path)?;
        if let Some(n) = value.as_i64() {
            return Some(n as i128);
        }
        if let Some(n) = value.as_u64() {
            return Some(n as i128);
        }
        if let Some(f) = value.as_f64() {
            return Some(f.round() as i128);
        }
        if let Some(s) = value.as_str() {
            return s.replace(',', "").parse::<i128>().ok();
        }
        None
    })
}

fn extract_f64_any(raw: &Value, paths: &[&str]) -> Option<f64> {
    paths.iter().find_map(|path| {
        let value = get_path(raw, path)?;
        if let Some(f) = value.as_f64() {
            return Some(f);
        }
        if let Some(n) = value.as_i64() {
            return Some(n as f64);
        }
        if let Some(n) = value.as_u64() {
            return Some(n as f64);
        }
        if let Some(s) = value.as_str() {
            return s.replace(',', "").parse::<f64>().ok();
        }
        None
    })
}

fn parse_u64(value: &Value) -> Option<u64> {
    if let Some(n) = value.as_u64() {
        return Some(n);
    }
    if let Some(n) = value.as_i64() {
        return u64::try_from(n).ok();
    }
    if let Some(f) = value.as_f64() {
        if f.is_finite() && f >= 0.0 {
            return Some(f.round() as u64);
        }
    }
    if let Some(s) = value.as_str() {
        return s.replace(',', "").parse::<u64>().ok();
    }
    None
}

fn get_path<'a>(raw: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = raw;
    for part in path.split('.') {
        current = current.get(part)?;
    }
    Some(current)
}

fn nanos_to_display_dusdc(raw_nanos: u64) -> String {
    let whole = raw_nanos / 1_000_000_000;
    let frac = raw_nanos % 1_000_000_000;
    if frac == 0 {
        return whole.to_string();
    }
    let mut frac_string = format!("{frac:09}");
    while frac_string.ends_with('0') {
        frac_string.pop();
    }
    format!("{whole}.{frac_string}")
}

fn budget_nanos_to_protocol_raw(raw_nanos: u64) -> u64 {
    raw_nanos / 1_000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_nanos_budget_to_display_string() {
        assert_eq!(nanos_to_display_dusdc(100_000_000_000), "100");
        assert_eq!(nanos_to_display_dusdc(12_500_000_000), "12.5");
    }

    #[test]
    fn extracts_payoff_rows_from_existing_compile_shape() {
        let raw = serde_json::json!({
            "payoffTable": [
                {
                    "condition": "BTC settles >= 100k",
                    "grossPayoutRaw": "25000000",
                    "netPnlRaw": "5000000"
                }
            ]
        });

        let rows = extract_payoff_rows(&raw).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].gross_payout, 25_000_000);
        assert_eq!(rows[0].net_pnl, 5_000_000);
    }
}