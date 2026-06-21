pub mod intent;
pub mod intent_proposal;
pub mod intent_quote_service;
pub mod intent_service;
pub mod market_catalog;
pub mod market_refresh;
pub mod market_store;

use std::io;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration as StdDuration;

use chrono::Duration;
use deepbook_client::{
    DeepBookClient, DeepBookConfig, FreshnessConfig, ObjectOwnerKind, StructxMarketStatus,
    SuiObjectInfo, SuiRpcClient, DUSDC_DECIMALS, PREDICT_OBJECT_ID, SUI_CLOCK_OBJECT_ID,
};
use structx_core::{
    build_manager_balance_tx_kind, build_mint_tx_kind, build_quote_plan, build_quote_tx_kind,
    build_redeem_tx_kind, compile_breakout, compile_bucket_payoff, compile_center_band_condor,
    compile_convex_tail_ladder, compile_downside_convexity, compile_downside_step_ladder,
    compile_expiry_move_note, compile_moonshot_upside, compile_near_barrier_proxy,
    compile_portfolio_crash_shield, compile_upside_step_ladder, guard_quote_preview,
    optimize_breakout_quantities, score_smart_candidate, select_candidate_markets,
    AdvancedCompileResult, AdvancedCompiledLeg, AdvancedLegKind, AdvancedStrategyKind, BarrierSide,
    BreakoutAskInputs, BreakoutStyle, CenterBandCondorInput, ConvexTailLadderInput, DisplayPrice,
    DownsideConvexityInput, DownsideStepLadderInput, ExpiryMoveNoteInput, ManagerPositionRead,
    MintObjectRefs, MoonshotUpsideInput, NearBarrierProxyInput, PayoffBucket,
    PortfolioCrashShieldInput, PriceScale, QuoteAssetDisplay, QuoteCall, QuoteCostGuard,
    QuoteObjectRefs, QuotePreview, QuotePreviewLeg, QuoteTxKind, SelectedMarket, SmartBudgetStyle,
    SmartCandidateMetrics, SmartCandidateScore, Strike, UpsideStepLadderInput,
};
use sui_sdk_types::Address;

pub use intent::{
    Direction, ExpiryPreferenceOverride, IntentConfidence, IntentPlan, RangeIntent, RiskStyle,
    StrategyTemplateId, UserIntentRequest,
};
pub use intent_proposal::{
    CompiledProposalLeg, ExecutionProposal, PayoffRow, ProposalQuoteMetadata,
    QuoteIntentPlanRequest,
};
pub use intent_quote_service::quote_intent_plan;
pub use intent_service::{parse_intent_deterministic, plan_from_intent, IntentPlanningResponse};
pub use market_catalog::{
    CatalogMarketSnapshot, ExpiryPreference, MarketCatalog, MarketCatalogSource, MarketCategory,
    MarketKind, MarketSearchQuery, MarketStatus,
};
pub use market_refresh::{
    build_catalog_from_markets_json, load_catalog_status, load_or_refresh_catalog_from_json,
    normalize_market_json, refresh_catalog_from_existing_markets_json, CatalogBuildReport,
    CatalogStatus,
};
pub use market_store::{DiskMarketStore, MarketStore};

pub struct CompileStrategyJsonArgs {
    pub server_url: String,
    pub predict_id: String,
    pub rpc_url: String,
    pub owner: String,
    pub strategy: String,
    pub budget_dusdc: String,
    pub style: String,
    pub expiry_preference: String,
    pub slippage_bps: u16,
    pub bucket_step: DisplayPrice,
    pub custom_k1_price: Option<DisplayPrice>,
    pub custom_k2_price: Option<DisplayPrice>,
    pub custom_k3_price: Option<DisplayPrice>,
    pub custom_k4_price: Option<DisplayPrice>,
    pub levels_each_side: u32,
    pub max_quote_market_attempts: usize,
    pub portfolio_exposure_dusdc: f64,
    pub over_hedge_cap_bps: u16,
    pub convex_gamma_bps: u16,
    pub dead_zone_bps: u16,
    pub moonshot_range_weight_bps: u16,
    pub moonshot_tail_gamma_bps: u16,
    pub downside_range_weight_bps: u16,
    pub downside_tail_gamma_bps: u16,
    pub upside_near_range_weight_bps: u16,
    pub upside_upper_range_weight_bps: u16,
    pub upside_tail_gamma_bps: u16,
    pub downside_near_range_weight_bps: u16,
    pub downside_lower_range_weight_bps: u16,
    pub downside_step_tail_gamma_bps: u16,
    pub condor_center_weight_bps: u16,
    pub barrier_side: String,
    pub barrier_near_range_weight_bps: u16,
    pub barrier_tail_gamma_bps: u16,
    pub exclude_oracle_ids: Vec<String>,
}

pub fn build_freshness(
    max_price_age_secs: i64,
    max_svi_age_secs: i64,
    min_time_to_expiry_secs: i64,
    strict_freshness: bool,
) -> FreshnessConfig {
    FreshnessConfig {
        max_price_age: Duration::seconds(max_price_age_secs),
        max_svi_age: Duration::seconds(max_svi_age_secs),
        min_time_to_expiry: Duration::seconds(min_time_to_expiry_secs),
        require_price_timestamp: strict_freshness,
        require_svi_timestamp: strict_freshness,
    }
}

pub async fn compile_strategy_json_value(
    args: CompileStrategyJsonArgs,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let advanced_strategy = AdvancedStrategyKind::from_api_value(&args.strategy).ok();

    if args.strategy != "BREAKOUT_PROTECTION" && advanced_strategy.is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "strategy is not wired into compile-strategy-json yet",
        )
        .into());
    }

    if args.expiry_preference != "nearest_active" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "only nearest_active expiry preference is supported in this milestone",
        )
        .into());
    }

    let freshness = build_freshness(60, 60, 300, false);
    let client = build_client(args.server_url.clone(), args.predict_id.clone())?;
    let markets = client.load_structx_markets(freshness).await?;
    let excluded_oracles = args
        .exclude_oracle_ids
        .iter()
        .map(|oracle| oracle.to_ascii_lowercase())
        .collect::<std::collections::HashSet<_>>();
    let candidates = select_candidate_markets(&markets, PriceScale::E9)
        .into_iter()
        .filter(|selected| !excluded_oracles.contains(&selected.oracle_id.to_ascii_lowercase()))
        .collect::<Vec<_>>();

    if candidates.is_empty() {
        return Err(
            io::Error::new(io::ErrorKind::InvalidData, "no quoteable market candidates").into()
        );
    }

    let rpc = SuiRpcClient::new(args.rpc_url.clone(), StdDuration::from_secs(20))?;
    let predict = position_service::resolve_sui_object(&rpc, PREDICT_OBJECT_ID).await?;
    let clock = position_service::resolve_sui_object(&rpc, SUI_CLOCK_OBJECT_ID).await?;

    let asset = QuoteAssetDisplay { symbol: "dUSDC".to_string(), decimals: DUSDC_DECIMALS };

    let mut warnings = vec![
        "DeepBook Predict integration is testnet-only for this milestone.".to_string(),
        "Quote can change before signing; transaction build must apply slippage guard."
            .to_string(),
        "Known issue: binary event-derived MarketKeys can read 0 while range positions verify correctly.".to_string(),
    ];

    let max_attempts = args.max_quote_market_attempts.min(candidates.len());

    for selected in candidates.into_iter().take(max_attempts) {
        let oracle = position_service::resolve_sui_object(&rpc, selected.oracle_id).await?;

        if let Err(err) = validate_quote_object_refs_quiet(&predict, &oracle, &clock) {
            warnings.push(format!("skipped oracle {}: {err}", selected.oracle_id));
            continue;
        }

        if matches!(advanced_strategy, Some(AdvancedStrategyKind::SmartBudgetSelector)) {
            match compile_smart_budget_selector_from_market(
                &args,
                &selected,
                &predict,
                &oracle,
                &clock,
                &rpc,
                &asset,
                warnings.clone(),
            )
            .await
            {
                Ok(output) => return Ok(output),
                Err(err) => {
                    warnings.push(format!("skipped oracle {}: {err}", selected.oracle_id));
                    continue;
                }
            }
        }

        if let Some(strategy_kind) = advanced_strategy {
            match compile_advanced_strategy_json_from_market(
                &args,
                strategy_kind,
                &selected,
                &predict,
                &oracle,
                &clock,
                &rpc,
                &asset,
                warnings.clone(),
            )
            .await
            {
                Ok(output) => return Ok(output),
                Err(err) => {
                    warnings.push(format!("skipped oracle {}: {err}", selected.oracle_id));
                    continue;
                }
            }
        }

        match compile_breakout_strategy_json_from_market(
            &args,
            &selected,
            &predict,
            &oracle,
            &clock,
            &rpc,
            &asset,
            warnings.clone(),
        )
        .await
        {
            Ok(output) => return Ok(output),
            Err(err) => {
                warnings.push(format!("skipped oracle {}: {err}", selected.oracle_id));
            }
        }
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        format!("failed to compile strategy after {max_attempts} market attempts"),
    )
    .into())
}

pub struct DevinspectMintBreakoutJsonArgs {
    pub server_url: String,
    pub predict_id: String,
    pub rpc_url: String,
    pub manager_id: String,
    pub sender: String,
    pub max_total_mint_cost_raw: u64,
    pub slippage_bps: u16,
    pub max_quote_market_attempts: usize,
    pub write_execute_script: bool,
}

pub struct DevinspectRedeemBreakoutJsonArgs {
    pub rpc_url: String,
    pub manager_id: String,
    pub sender: String,
    pub from_execution_json: PathBuf,
    pub auto_size_down: bool,
    pub write_execute_script: bool,
    pub allow_zero_payout_script: bool,
}

pub async fn list_markets_json_value(
    server_url: String,
    predict_id: String,
    freshness: FreshnessConfig,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let client = build_client(server_url, predict_id)?;
    let markets_result = client.load_structx_markets(freshness).await;
    Ok(match markets_result {
        Ok(markets) => {
            let usable = markets.iter().filter(|m| m.structx_status.is_usable()).count();
            let warnings = markets
                .iter()
                .filter(|m| matches!(m.structx_status, StructxMarketStatus::UsableWithWarnings(_)))
                .count();
            serde_json::json!({
                "ok": true,
                "asset": "BTC",
                "network": "sui:testnet",
                "totalCount": markets.len(),
                "usableCount": usable,
                "warningsCount": warnings,
                "markets": markets,
            })
        }
        Err(err) => serde_json::json!({
            "ok": true,
            "asset": "BTC",
            "network": "sui:testnet",
            "totalCount": 0,
            "usableCount": 0,
            "warningsCount": 0,
            "markets": Vec::<serde_json::Value>::new(),
            "softError": err.to_string(),
        }),
    })
}

pub async fn manager_balance_json_value(
    rpc_url: String,
    manager_id: String,
    sender: String,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    validate_sui_address_arg("manager-id", &manager_id)?;
    validate_sui_address_arg("sender", &sender)?;

    let rpc = SuiRpcClient::new(rpc_url, StdDuration::from_secs(20))?;
    let manager = position_service::resolve_sui_object(&rpc, &manager_id).await?;
    position_service::validate_predict_manager_object(&manager)?;

    let tx_kind = build_manager_balance_tx_kind(&manager, &sender)?;
    let response = rpc.dev_inspect_transaction_kind(&tx_kind.sender, &tx_kind.tx_kind_b64).await?;
    let balance_raw = read_manager_balance_from_response(&response)?;
    let asset = QuoteAssetDisplay { symbol: "dUSDC".to_string(), decimals: DUSDC_DECIMALS };

    Ok(serde_json::json!({
        "ok": true,
        "balanceRaw": balance_raw.to_string(),
        "balanceDisplay": asset.format_amount(balance_raw),
        "stdout": format!(
            "Built manager-balance TransactionKind\nsender: {}\ntx_kind_b64_len: {}\n\nmanager balance raw: {}\nmanager balance: {}\n",
            tx_kind.sender,
            tx_kind.tx_kind_b64.len(),
            balance_raw,
            asset.format_amount(balance_raw),
        )
    }))
}

pub async fn devinspect_mint_breakout_json_value(
    args: DevinspectMintBreakoutJsonArgs,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    validate_sui_address_arg("manager-id", &args.manager_id)?;
    validate_sui_address_arg("sender", &args.sender)?;

    let client = build_client(args.server_url.clone(), args.predict_id.clone())?;
    let markets = client.load_structx_markets(build_freshness(60, 60, 300, false)).await?;
    let candidates = select_candidate_markets(&markets, PriceScale::E9);

    if candidates.is_empty() {
        return Err(
            io::Error::new(io::ErrorKind::InvalidData, "no quoteable market candidates").into()
        );
    }

    let max_attempts = args.max_quote_market_attempts.min(candidates.len());
    let mut failures = Vec::new();

    for selected in candidates.into_iter().take(max_attempts) {
        match devinspect_mint_for_selected_market_json(&args, &selected).await {
            Ok(value) => {
                return Ok(serde_json::json!({
                    "ok": true,
                    "attemptCount": failures.len() + 1,
                    "failures": failures,
                    "result": value,
                }));
            }
            Err(err) => failures.push(serde_json::json!({
                "oracleId": selected.oracle_id,
                "expiry": selected.expiry.to_rfc3339(),
                "reason": err.to_string(),
            })),
        }
    }

    Err(io::Error::other(format!(
        "all mint attempts failed: {}",
        serde_json::to_string(&failures)?
    ))
    .into())
}

pub async fn devinspect_redeem_breakout_json_value(
    args: DevinspectRedeemBreakoutJsonArgs,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    validate_sui_address_arg("manager-id", &args.manager_id)?;
    validate_sui_address_arg("sender", &args.sender)?;

    let base_reads = position_service::load_position_reads_from_execution_json(
        args.from_execution_json.as_path(),
    )?;

    if base_reads.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "no PositionMinted or RangeMinted events found",
        )
        .into());
    }

    let oracle_id = first_oracle_id(&base_reads)?;
    let rpc = SuiRpcClient::new(args.rpc_url.clone(), StdDuration::from_secs(20))?;
    let predict = position_service::resolve_sui_object(&rpc, PREDICT_OBJECT_ID).await?;
    let manager = position_service::resolve_sui_object(&rpc, &args.manager_id).await?;
    let oracle = position_service::resolve_sui_object(&rpc, &oracle_id).await?;
    let clock = position_service::resolve_sui_object(&rpc, SUI_CLOCK_OBJECT_ID).await?;

    position_service::validate_predict_manager_object(&manager)?;
    validate_quote_object_refs_quiet(&predict, &oracle, &clock)?;

    let candidates = redeem_bps_candidates(10_000, args.auto_size_down);
    let mut failures = Vec::new();

    for bps in candidates {
        let reads = scale_position_reads(&base_reads, bps)?;
        let tx_kind = build_redeem_tx_kind(
            &reads,
            MintObjectRefs { predict: &predict, manager: &manager, oracle: &oracle, clock: &clock },
            &args.sender,
        )?;

        let response =
            rpc.dev_inspect_transaction_kind(&tx_kind.sender, &tx_kind.tx_kind_b64).await?;

        match redeem_preview_from_response(&response) {
            Ok((total_payout_raw, events)) => {
                let asset =
                    QuoteAssetDisplay { symbol: "dUSDC".to_string(), decimals: DUSDC_DECIMALS };
                let mut warnings = Vec::new();
                if args.write_execute_script {
                    warnings.push(
                        "write_execute_script was requested, but direct API preview mode does not emit helper scripts.".to_string(),
                    );
                }
                if args.allow_zero_payout_script {
                    warnings.push(
                        "allow_zero_payout_script is ignored unless script generation is enabled."
                            .to_string(),
                    );
                }

                return Ok(serde_json::json!({
                    "ok": true,
                    "managerId": args.manager_id,
                    "oracleId": oracle_id,
                    "fromExecutionJson": args.from_execution_json.to_string_lossy(),
                    "requestedRedeemBps": 10_000,
                    "selectedRedeemBps": bps,
                    "autoSizeDown": args.auto_size_down,
                    "legs": serialize_manager_position_reads(&reads),
                    "totalPayoutRaw": total_payout_raw.to_string(),
                    "totalPayoutDisplay": asset.format_amount(total_payout_raw),
                    "eventCount": events.len(),
                    "events": events,
                    "failures": failures,
                    "warnings": warnings,
                }));
            }
            Err(err) => failures.push(serde_json::json!({
                "redeemBps": bps,
                "reason": err.to_string(),
            })),
        }
    }

    Err(io::Error::other(format!(
        "all redeem preview attempts failed: {}",
        serde_json::to_string(&failures)?
    ))
    .into())
}

fn validate_sui_address_arg(name: &str, value: &str) -> Result<(), Box<dyn std::error::Error>> {
    let value = value.trim();
    if value.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("`--{name}` is empty. Make sure the variable is set."),
        )
        .into());
    }
    Address::from_str(value).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("`--{name}` is not a valid Sui address `{value}`: {err}"),
        )
    })?;
    Ok(())
}

fn build_client(
    server_url: String,
    predict_id: String,
) -> Result<DeepBookClient, Box<dyn std::error::Error>> {
    Ok(DeepBookClient::new(DeepBookConfig {
        server_url,
        predict_id,
        request_timeout: StdDuration::from_secs(15),
    })?)
}

async fn devinspect_mint_for_selected_market_json(
    args: &DevinspectMintBreakoutJsonArgs,
    selected: &structx_core::SelectedMarket<'_>,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let strikes = selected.grid.centered_strikes_by_display_step(
        selected.spot_raw,
        DisplayPrice(250.0),
        4,
    )?;
    let center = selected
        .grid
        .snap_nearest(selected.spot_raw)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "spot cannot be snapped"))?;
    let center_idx = strikes
        .iter()
        .position(|strike| strike.raw == center.raw)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "center strike missing"))?;
    if center_idx < 2 || center_idx + 2 >= strikes.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not enough strikes around spot for default breakout preview",
        )
        .into());
    }

    let k1 = strikes[center_idx - 2];
    let k2 = strikes[center_idx - 1];
    let k3 = strikes[center_idx + 1];
    let k4 = strikes[center_idx + 2];
    let compiled = compile_breakout(k1, k2, k3, k4, 1_000, 400)?;
    let plan = build_quote_plan(selected, &compiled)?;

    let rpc = SuiRpcClient::new(args.rpc_url.clone(), StdDuration::from_secs(20))?;
    let predict = position_service::resolve_sui_object(&rpc, PREDICT_OBJECT_ID).await?;
    let manager = position_service::resolve_sui_object(&rpc, &args.manager_id).await?;
    let oracle = position_service::resolve_sui_object(&rpc, selected.oracle_id).await?;
    let clock = position_service::resolve_sui_object(&rpc, SUI_CLOCK_OBJECT_ID).await?;

    position_service::validate_predict_manager_object(&manager)?;
    validate_quote_object_refs_quiet(&predict, &oracle, &clock)?;

    let quote_tx_kind = build_quote_tx_kind(
        &plan,
        QuoteObjectRefs { predict: &predict, oracle: &oracle, clock: &clock },
        &args.sender,
    )?;
    let quote_response =
        rpc.dev_inspect_transaction_kind(&quote_tx_kind.sender, &quote_tx_kind.tx_kind_b64).await?;
    let preview = quote_preview_from_response(selected, &plan, &quote_tx_kind, &quote_response)?;
    let guarded = guard_quote_preview(
        &preview,
        QuoteCostGuard {
            max_total_mint_cost_raw: args.max_total_mint_cost_raw,
            slippage_bps: args.slippage_bps,
        },
    )?;

    let manager_balance_tx = build_manager_balance_tx_kind(&manager, &args.sender)?;
    let manager_balance_response = rpc
        .dev_inspect_transaction_kind(&manager_balance_tx.sender, &manager_balance_tx.tx_kind_b64)
        .await?;
    let manager_balance_raw = read_manager_balance_from_response(&manager_balance_response)?;
    if manager_balance_raw < preview.total_mint_cost_raw {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "manager balance {} is below required mint cost {}",
                manager_balance_raw, preview.total_mint_cost_raw
            ),
        )
        .into());
    }

    let mint_tx_kind = build_mint_tx_kind(
        &plan,
        MintObjectRefs { predict: &predict, manager: &manager, oracle: &oracle, clock: &clock },
        &args.sender,
    )?;
    let mint_response =
        rpc.dev_inspect_transaction_kind(&mint_tx_kind.sender, &mint_tx_kind.tx_kind_b64).await?;
    let mint_status = mint_response
        .get("effects")
        .and_then(|effects| effects.get("status"))
        .and_then(|status| status.get("status"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    if mint_status != "success" {
        return Err(io::Error::other(devinspect_failure_summary(&mint_response)).into());
    }

    let mut warnings = Vec::new();
    if args.write_execute_script {
        warnings.push(
            "write_execute_script was requested, but direct API preview mode does not emit helper scripts.".to_string(),
        );
    }

    Ok(serde_json::json!({
        "oracleId": selected.oracle_id,
        "expiry": selected.expiry.to_rfc3339(),
        "spotRaw": selected.spot_raw.to_string(),
        "spotDisplay": position_service::format_raw_price_e9(selected.spot_raw),
        "strikes": {
            "k1": position_service::format_raw_price_e9(k1.raw),
            "k2": position_service::format_raw_price_e9(k2.raw),
            "k3": position_service::format_raw_price_e9(k3.raw),
            "k4": position_service::format_raw_price_e9(k4.raw),
            "k1Raw": k1.raw.to_string(),
            "k2Raw": k2.raw.to_string(),
            "k3Raw": k3.raw.to_string(),
            "k4Raw": k4.raw.to_string(),
        },
        "quotePreview": serialize_quote_preview(&preview),
        "quoteGuard": {
            "maxTotalMintCostRaw": guarded.max_total_mint_cost_raw.to_string(),
            "maxAllowedAfterSlippageRaw": guarded.max_allowed_after_slippage_raw.to_string(),
            "totalMintCostRaw": guarded.total_mint_cost_raw.to_string(),
            "slippageBps": guarded.slippage_bps,
        },
        "managerBalanceRaw": manager_balance_raw.to_string(),
        "managerBalanceDisplay": preview.asset.format_amount(manager_balance_raw),
        "mintPreview": {
            "status": mint_status,
            "eventCount": mint_response
                .get("events")
                .and_then(serde_json::Value::as_array)
                .map(|events| events.len())
                .unwrap_or(0),
            "events": mint_response.get("events").cloned().unwrap_or(serde_json::Value::Array(Vec::new())),
        },
        "warnings": warnings,
    }))
}

fn validate_quote_object_refs_quiet(
    predict: &SuiObjectInfo,
    oracle: &SuiObjectInfo,
    clock: &SuiObjectInfo,
) -> Result<(), Box<dyn std::error::Error>> {
    for (role, object) in [("predict", predict), ("oracle", oracle), ("clock", clock)] {
        if object.owner_kind != ObjectOwnerKind::Shared {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{role} object is not shared: owner={}", object.owner_kind),
            )
            .into());
        }
        if object.initial_shared_version.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{role} object is missing initial_shared_version"),
            )
            .into());
        }
    }
    Ok(())
}

fn quote_preview_from_response(
    selected: &structx_core::SelectedMarket<'_>,
    plan: &structx_core::QuotePlan,
    tx_kind: &QuoteTxKind,
    response: &serde_json::Value,
) -> Result<QuotePreview, Box<dyn std::error::Error>> {
    let status = response
        .get("effects")
        .and_then(|effects| effects.get("status"))
        .and_then(|status| status.get("status"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    if status != "success" {
        return Err(io::Error::other(devinspect_failure_summary(response)).into());
    }

    let results = response
        .get("results")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing devInspect results"))?;
    let asset = QuoteAssetDisplay { symbol: "dUSDC".to_string(), decimals: DUSDC_DECIMALS };
    let mut preview_legs = Vec::new();

    for (quote_idx, call) in plan.calls.iter().enumerate() {
        let command_idx = tx_kind.quote_result_command_indices.get(quote_idx).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "missing quote command index")
        })?;
        let result = results.get(*command_idx).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("missing devInspect result for command {command_idx}"),
            )
        })?;
        let return_values = result
            .get("returnValues")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing returnValues"))?;
        if return_values.len() != 2 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "expected 2 quote return values",
            )
            .into());
        }
        let mint_cost_raw = position_service::decode_devinspect_u64(&return_values[0])?;
        let redeem_payout_raw = position_service::decode_devinspect_u64(&return_values[1])?;
        match call {
            QuoteCall::Binary { function, direction, strike, quantity, .. } => {
                preview_legs.push(QuotePreviewLeg {
                    index: quote_idx,
                    function: function.to_string(),
                    leg: format!("{direction}_binary"),
                    strike_or_lower: selected.grid.display(*strike).to_string(),
                    upper: None,
                    quantity: *quantity,
                    mint_cost_raw,
                    redeem_payout_raw,
                })
            }
            QuoteCall::Range { function, lower, upper, quantity, .. } => {
                preview_legs.push(QuotePreviewLeg {
                    index: quote_idx,
                    function: function.to_string(),
                    leg: "range".to_string(),
                    strike_or_lower: selected.grid.display(*lower).to_string(),
                    upper: Some(selected.grid.display(*upper).to_string()),
                    quantity: *quantity,
                    mint_cost_raw,
                    redeem_payout_raw,
                })
            }
        }
    }
    Ok(QuotePreview::new(asset, preview_legs))
}

fn serialize_quote_preview(preview: &QuotePreview) -> serde_json::Value {
    serde_json::json!({
        "legs": preview.legs.iter().map(|leg| serde_json::json!({
            "index": leg.index,
            "function": leg.function,
            "leg": leg.leg,
            "strikeOrLower": leg.strike_or_lower,
            "upper": leg.upper,
            "quantity": leg.quantity.to_string(),
            "mintCostRaw": leg.mint_cost_raw.to_string(),
            "mintCostDisplay": preview.asset.format_amount(leg.mint_cost_raw),
            "redeemPayoutRaw": leg.redeem_payout_raw.to_string(),
            "redeemPayoutDisplay": preview.asset.format_amount(leg.redeem_payout_raw),
        })).collect::<Vec<_>>(),
        "totalMintCostRaw": preview.total_mint_cost_raw.to_string(),
        "totalMintCostDisplay": preview.total_mint_cost_display(),
        "totalRedeemPayoutRaw": preview.total_redeem_payout_raw.to_string(),
        "totalRedeemPayoutDisplay": preview.total_redeem_payout_display(),
    })
}

fn read_manager_balance_from_response(
    response: &serde_json::Value,
) -> Result<u64, Box<dyn std::error::Error>> {
    let status = response
        .get("effects")
        .and_then(|effects| effects.get("status"))
        .and_then(|status| status.get("status"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    if status != "success" {
        return Err(io::Error::other(devinspect_failure_summary(response)).into());
    }
    let results =
        response.get("results").and_then(serde_json::Value::as_array).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "missing manager balance results")
        })?;
    let return_values = results
        .first()
        .and_then(|result| result.get("returnValues"))
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "missing manager balance returnValues")
        })?;
    if return_values.len() != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "expected 1 manager balance return",
        )
        .into());
    }
    position_service::decode_devinspect_u64(&return_values[0])
}

fn redeem_bps_candidates(requested: u16, auto_size_down: bool) -> Vec<u16> {
    if !auto_size_down {
        return vec![requested];
    }
    let ladder = [10_000u16, 7_500, 5_000, 2_500, 1_000, 500, 250, 100, 50, 10, 1];
    let mut candidates = vec![requested];
    for bps in ladder {
        if bps <= requested && !candidates.contains(&bps) {
            candidates.push(bps);
        }
    }
    candidates
}

fn scale_position_reads(
    reads: &[ManagerPositionRead],
    bps: u16,
) -> Result<Vec<ManagerPositionRead>, Box<dyn std::error::Error>> {
    if bps == 0 || bps > 10_000 {
        return Err(
            io::Error::new(io::ErrorKind::InvalidInput, "redeem bps must be in 1..=10000").into()
        );
    }
    reads
        .iter()
        .map(|read| match read {
            ManagerPositionRead::Binary {
                oracle_id,
                expiry_ms,
                strike_raw,
                is_up,
                expected_quantity,
            } => Ok(ManagerPositionRead::Binary {
                oracle_id: oracle_id.clone(),
                expiry_ms: *expiry_ms,
                strike_raw: *strike_raw,
                is_up: *is_up,
                expected_quantity: scale_quantity_bps(*expected_quantity, bps),
            }),
            ManagerPositionRead::Range {
                oracle_id,
                expiry_ms,
                lower_raw,
                upper_raw,
                expected_quantity,
            } => Ok(ManagerPositionRead::Range {
                oracle_id: oracle_id.clone(),
                expiry_ms: *expiry_ms,
                lower_raw: *lower_raw,
                upper_raw: *upper_raw,
                expected_quantity: scale_quantity_bps(*expected_quantity, bps),
            }),
        })
        .collect()
}

fn scale_quantity_bps(quantity: u64, bps: u16) -> u64 {
    let scaled = quantity.saturating_mul(bps as u64) / 10_000;
    scaled.max(1).min(quantity)
}

fn serialize_manager_position_reads(reads: &[ManagerPositionRead]) -> Vec<serde_json::Value> {
    reads
        .iter()
        .map(|read| match read {
            ManagerPositionRead::Binary {
                oracle_id,
                expiry_ms,
                strike_raw,
                is_up,
                expected_quantity,
            } => serde_json::json!({
                "kind": "binary",
                "oracleId": oracle_id,
                "expiryMs": expiry_ms,
                "strikeRaw": strike_raw.to_string(),
                "strike": position_service::format_raw_price_e9(*strike_raw),
                "direction": if *is_up { "up" } else { "down" },
                "quantity": expected_quantity.to_string(),
            }),
            ManagerPositionRead::Range {
                oracle_id,
                expiry_ms,
                lower_raw,
                upper_raw,
                expected_quantity,
            } => serde_json::json!({
                "kind": "range",
                "oracleId": oracle_id,
                "expiryMs": expiry_ms,
                "lowerRaw": lower_raw.to_string(),
                "upperRaw": upper_raw.to_string(),
                "lower": position_service::format_raw_price_e9(*lower_raw),
                "upper": position_service::format_raw_price_e9(*upper_raw),
                "quantity": expected_quantity.to_string(),
            }),
        })
        .collect()
}

fn first_oracle_id(reads: &[ManagerPositionRead]) -> Result<String, Box<dyn std::error::Error>> {
    let first = reads
        .first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "empty position reads"))?;
    let oracle_id = match first {
        ManagerPositionRead::Binary { oracle_id, .. }
        | ManagerPositionRead::Range { oracle_id, .. } => oracle_id,
    };
    for read in reads {
        let current = match read {
            ManagerPositionRead::Binary { oracle_id, .. }
            | ManagerPositionRead::Range { oracle_id, .. } => oracle_id,
        };
        if current != oracle_id {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "execution JSON contains multiple oracle IDs; split redemption per oracle",
            )
            .into());
        }
    }
    Ok(oracle_id.clone())
}

fn redeem_preview_from_response(
    response: &serde_json::Value,
) -> Result<(u64, Vec<serde_json::Value>), Box<dyn std::error::Error>> {
    let status = response
        .get("effects")
        .and_then(|effects| effects.get("status"))
        .and_then(|status| status.get("status"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    if status != "success" {
        return Err(io::Error::other(devinspect_failure_summary(response)).into());
    }
    let events =
        response.get("events").and_then(serde_json::Value::as_array).cloned().unwrap_or_default();
    let mut total_payout_raw = 0u64;
    let mut items = Vec::new();

    for event in events {
        let event_type = event.get("type").and_then(serde_json::Value::as_str).unwrap_or("");
        let parsed = event.get("parsedJson").unwrap_or(&serde_json::Value::Null);
        if event_type.ends_with("::predict::PositionRedeemed") {
            let payout = position_service::json_required_u64(parsed, "payout")?;
            total_payout_raw = total_payout_raw
                .checked_add(payout)
                .ok_or_else(|| io::Error::other("total payout overflow"))?;
            items.push(serde_json::json!({
                "event": "PositionRedeemed",
                "direction": if position_service::json_required_bool(parsed, "is_up")? { "up" } else { "down" },
                "strike": position_service::format_raw_price_e9(position_service::json_required_u64(parsed, "strike")?),
                "upper": serde_json::Value::Null,
                "quantity": position_service::json_required_u64(parsed, "quantity")?.to_string(),
                "payoutRaw": payout.to_string(),
                "bidPrice": position_service::json_required_string(parsed, "bid_price")?,
                "isSettled": position_service::json_required_bool(parsed, "is_settled")?,
            }));
        } else if event_type.ends_with("::predict::RangeRedeemed") {
            let payout = position_service::json_required_u64(parsed, "payout")?;
            total_payout_raw = total_payout_raw
                .checked_add(payout)
                .ok_or_else(|| io::Error::other("total payout overflow"))?;
            items.push(serde_json::json!({
                "event": "RangeRedeemed",
                "direction": serde_json::Value::Null,
                "strike": position_service::format_raw_price_e9(position_service::json_required_u64(parsed, "lower_strike")?),
                "upper": position_service::format_raw_price_e9(position_service::json_required_u64(parsed, "higher_strike")?),
                "quantity": position_service::json_required_u64(parsed, "quantity")?.to_string(),
                "payoutRaw": payout.to_string(),
                "bidPrice": position_service::json_required_string(parsed, "bid_price")?,
                "isSettled": position_service::json_required_bool(parsed, "is_settled")?,
            }));
        }
    }
    Ok((total_payout_raw, items))
}

fn devinspect_failure_summary(response: &serde_json::Value) -> String {
    let status_error = response
        .get("effects")
        .and_then(|effects| effects.get("status"))
        .and_then(|status| status.get("error"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown error");
    format!("devInspect failed: {status_error}")
}

#[derive(Debug, Clone)]
struct SmartCompiledCandidate {
    strategy: String,
    output: serde_json::Value,
    metrics: SmartCandidateMetrics,
    score: SmartCandidateScore,
}

#[derive(Debug, Clone, Copy)]
struct StrategyStrikeSet {
    center: Strike,
    k1: Strike,
    k2: Strike,
    k3: Strike,
    k4: Strike,
}

#[allow(clippy::too_many_arguments)]
async fn compile_smart_budget_selector_from_market(
    args: &CompileStrategyJsonArgs,
    selected: &SelectedMarket<'_>,
    predict: &SuiObjectInfo,
    oracle: &SuiObjectInfo,
    clock: &SuiObjectInfo,
    rpc: &SuiRpcClient,
    asset: &QuoteAssetDisplay,
    warnings: Vec<String>,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let style = SmartBudgetStyle::from_api_value(&args.style);
    let candidate_strategies = [
        "BREAKOUT_PROTECTION",
        "PORTFOLIO_CRASH_SHIELD",
        "CONVEX_TAIL_LADDER",
        "EXPIRY_MOVE_NOTE",
        "MOONSHOT_UPSIDE",
        "DOWNSIDE_CONVEXITY",
        "UPSIDE_STEP_LADDER",
        "DOWNSIDE_STEP_LADDER",
        "CENTER_BAND_CONDOR",
    ];

    let mut candidates = Vec::<SmartCompiledCandidate>::new();
    let mut selector_warnings = warnings;

    for strategy in candidate_strategies {
        let candidate_output = match strategy {
            "BREAKOUT_PROTECTION" => {
                compile_breakout_strategy_json_from_market(
                    args,
                    selected,
                    predict,
                    oracle,
                    clock,
                    rpc,
                    asset,
                    vec![],
                )
                .await
            }
            "PORTFOLIO_CRASH_SHIELD" => {
                compile_advanced_strategy_json_from_market(
                    args,
                    AdvancedStrategyKind::PortfolioCrashShield,
                    selected,
                    predict,
                    oracle,
                    clock,
                    rpc,
                    asset,
                    vec![],
                )
                .await
            }
            "CONVEX_TAIL_LADDER" => {
                compile_advanced_strategy_json_from_market(
                    args,
                    AdvancedStrategyKind::ConvexTailLadder,
                    selected,
                    predict,
                    oracle,
                    clock,
                    rpc,
                    asset,
                    vec![],
                )
                .await
            }
            "EXPIRY_MOVE_NOTE" => {
                compile_advanced_strategy_json_from_market(
                    args,
                    AdvancedStrategyKind::ExpiryMoveNote,
                    selected,
                    predict,
                    oracle,
                    clock,
                    rpc,
                    asset,
                    vec![],
                )
                .await
            }
            "MOONSHOT_UPSIDE" => {
                compile_advanced_strategy_json_from_market(
                    args,
                    AdvancedStrategyKind::MoonshotUpside,
                    selected,
                    predict,
                    oracle,
                    clock,
                    rpc,
                    asset,
                    vec![],
                )
                .await
            }
            "DOWNSIDE_CONVEXITY" => {
                compile_advanced_strategy_json_from_market(
                    args,
                    AdvancedStrategyKind::DownsideConvexity,
                    selected,
                    predict,
                    oracle,
                    clock,
                    rpc,
                    asset,
                    vec![],
                )
                .await
            }
            "UPSIDE_STEP_LADDER" => {
                compile_advanced_strategy_json_from_market(
                    args,
                    AdvancedStrategyKind::UpsideStepLadder,
                    selected,
                    predict,
                    oracle,
                    clock,
                    rpc,
                    asset,
                    vec![],
                )
                .await
            }
            "DOWNSIDE_STEP_LADDER" => {
                compile_advanced_strategy_json_from_market(
                    args,
                    AdvancedStrategyKind::DownsideStepLadder,
                    selected,
                    predict,
                    oracle,
                    clock,
                    rpc,
                    asset,
                    vec![],
                )
                .await
            }
            "CENTER_BAND_CONDOR" => {
                compile_advanced_strategy_json_from_market(
                    args,
                    AdvancedStrategyKind::CenterBandCondor,
                    selected,
                    predict,
                    oracle,
                    clock,
                    rpc,
                    asset,
                    vec![],
                )
                .await
            }
            _ => unreachable!(),
        };

        let output = match candidate_output {
            Ok(output) => output,
            Err(err) => {
                selector_warnings.push(format!("Candidate {strategy} skipped: {err}"));
                continue;
            }
        };

        let metrics = smart_metrics_from_output(strategy, &output)?;
        let score = score_smart_candidate(metrics, style)?;
        candidates.push(SmartCompiledCandidate {
            strategy: strategy.to_string(),
            output,
            metrics,
            score,
        });
    }

    if candidates.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Smart Budget Selector produced no valid candidates",
        )
        .into());
    }

    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.score.score_e6));
    let winner = candidates
        .first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing smart winner"))?
        .clone();
    let mut output = winner.output;

    let alternatives = candidates
        .iter()
        .map(|candidate| {
            serde_json::json!({
                "strategy": candidate.strategy,
                "scoreE6": candidate.score.score_e6.to_string(),
                "premiumRaw": candidate.metrics.premium_raw.to_string(),
                "maxPayoutRaw": candidate.metrics.max_payout_raw.to_string(),
                "expectedPayoutRaw": candidate.metrics.expected_payout_raw.to_string(),
                "hitProbabilityBps": candidate.metrics.hit_probability_bps,
                "worstCaseImprovementRaw": candidate.metrics.worst_case_improvement_raw.to_string(),
                "complexityPenaltyBps": candidate.metrics.complexity_penalty_bps,
                "scoreBreakdown": {
                    "maxPayoutScoreE6": candidate.score.max_payout_score_e6.to_string(),
                    "expectedPayoutScoreE6": candidate.score.expected_payout_score_e6.to_string(),
                    "hitProbabilityScoreE6": candidate.score.hit_probability_score_e6.to_string(),
                    "worstCaseScoreE6": candidate.score.worst_case_score_e6.to_string(),
                    "complexityPenaltyE6": candidate.score.complexity_penalty_e6.to_string()
                }
            })
        })
        .collect::<Vec<_>>();

    if let Some(obj) = output.as_object_mut() {
        obj.insert(
            "strategy".to_string(),
            serde_json::Value::String("SMART_BUDGET_SELECTOR".to_string()),
        );
        obj.insert(
            "selectedStrategy".to_string(),
            serde_json::Value::String(winner.strategy.clone()),
        );
        obj.insert(
            "smartSelector".to_string(),
            serde_json::json!({
                "style": args.style,
                "winner": winner.strategy,
                "winnerScoreE6": winner.score.score_e6.to_string(),
                "candidateCount": candidates.len(),
                "alternatives": alternatives
            }),
        );
        let warnings_value = obj.entry("warnings").or_insert_with(|| serde_json::json!([]));
        if let Some(warnings_array) = warnings_value.as_array_mut() {
            warnings_array.push(serde_json::Value::String(format!(
                "Smart Budget Selector chose {} from {} valid candidates.",
                winner.strategy,
                candidates.len()
            )));
            for warning in selector_warnings {
                warnings_array.push(serde_json::Value::String(warning));
            }
        }
    }

    Ok(output)
}

fn smart_metrics_from_output(
    strategy: &str,
    output: &serde_json::Value,
) -> Result<SmartCandidateMetrics, Box<dyn std::error::Error>> {
    let premium_raw = json_string_u64(output, "premiumRequiredRaw")?;
    let max_payout_raw = json_string_u64(output, "maxGrossPayoutRaw")?;
    let expected_payout_raw = estimate_expected_payout_from_payoff_table(output, strategy)?;
    let hit_probability_bps = estimate_hit_probability_bps(output, strategy);
    let worst_case_improvement_raw = estimate_worst_case_improvement(output, strategy)?;
    let complexity_penalty_bps = estimate_complexity_penalty_bps(output);
    Ok(SmartCandidateMetrics {
        premium_raw,
        max_payout_raw,
        expected_payout_raw,
        hit_probability_bps,
        worst_case_improvement_raw,
        complexity_penalty_bps,
    })
}

fn json_string_u64(
    value: &serde_json::Value,
    key: &str,
) -> Result<u64, Box<dyn std::error::Error>> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, format!("missing {key}")))?
        .parse::<u64>()
        .map_err(|err| {
            io::Error::new(io::ErrorKind::InvalidData, format!("bad {key}: {err}")).into()
        })
}

fn estimate_expected_payout_from_payoff_table(
    output: &serde_json::Value,
    strategy: &str,
) -> Result<u64, Box<dyn std::error::Error>> {
    let rows = output
        .get("payoffTable")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing payoffTable"))?;
    if rows.is_empty() {
        return Ok(0);
    }

    let weights_bps = payoff_weights_bps(strategy, rows.len());
    let mut total = 0u128;
    let mut weight_total = 0u128;
    for (row, weight_bps) in rows.iter().zip(weights_bps.iter()) {
        let gross = row
            .get("grossPayoutRaw")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("0")
            .parse::<u128>()
            .unwrap_or(0);
        total = total
            .checked_add(gross.saturating_mul(*weight_bps as u128))
            .ok_or_else(|| io::Error::other("expected payout overflow"))?;
        weight_total = weight_total.saturating_add(*weight_bps as u128);
    }

    if weight_total == 0 {
        return Ok(0);
    }

    Ok((total / weight_total).min(u64::MAX as u128) as u64)
}

fn payoff_weights_bps(strategy: &str, len: usize) -> Vec<u16> {
    match strategy {
        "PORTFOLIO_CRASH_SHIELD" => match len {
            0 => vec![],
            1 => vec![10_000],
            2 => vec![7_000, 3_000],
            3 => vec![5_000, 3_000, 2_000],
            _ => {
                let mut weights = vec![0u16; len];
                weights[0] = 5_000;
                weights[1] = 3_000;
                weights[2] = 2_000;
                weights
            }
        },
        "CONVEX_TAIL_LADDER" | "EXPIRY_MOVE_NOTE" => match len {
            0 => vec![],
            1 => vec![10_000],
            2 => vec![5_000, 5_000],
            3 => vec![4_000, 2_000, 4_000],
            _ => {
                let mut weights = vec![0u16; len];
                weights[0] = 3_000;
                weights[1] = 2_000;
                weights[len - 2] = 2_000;
                weights[len - 1] = 3_000;
                weights
            }
        },
        "MOONSHOT_UPSIDE" => match len {
            0 => vec![],
            1 => vec![10_000],
            2 => vec![4_000, 6_000],
            _ => {
                let mut weights = vec![0u16; len];
                weights[len - 2] = 4_000;
                weights[len - 1] = 6_000;
                weights
            }
        },
        "UPSIDE_STEP_LADDER" => match len {
            0 => vec![],
            1 => vec![10_000],
            2 => vec![4_500, 5_500],
            3 => vec![3_000, 3_500, 3_500],
            _ => {
                let mut weights = vec![0u16; len];
                weights[len - 3] = 3_000;
                weights[len - 2] = 3_500;
                weights[len - 1] = 3_500;
                weights
            }
        },
        "DOWNSIDE_STEP_LADDER" => match len {
            0 => vec![],
            1 => vec![10_000],
            2 => vec![4_500, 5_500],
            3 => vec![3_000, 3_500, 3_500],
            _ => {
                let mut weights = vec![0u16; len];
                weights[0] = 3_500;
                weights[1] = 3_500;
                weights[2] = 3_000;
                weights
            }
        },
        "CENTER_BAND_CONDOR" => match len {
            0 => vec![],
            1 => vec![10_000],
            2 => vec![5_000, 5_000],
            3 => vec![2_000, 6_000, 2_000],
            4 => vec![1_000, 4_000, 4_000, 1_000],
            _ => {
                let mut weights = vec![0u16; len];
                weights[0] = 1_000;
                weights[1] = 4_000;
                weights[2] = 4_000;
                weights[3] = 1_000;
                weights
            }
        },
        _ => match len {
            0 => vec![],
            1 => vec![10_000],
            2 => vec![5_000, 5_000],
            3 => vec![3_333, 3_334, 3_333],
            _ => {
                let base = 10_000 / len as u16;
                let mut weights = vec![base; len];
                let used: u16 = weights.iter().sum();
                if let Some(last) = weights.last_mut() {
                    *last += 10_000u16.saturating_sub(used);
                }
                weights
            }
        },
    }
}

fn estimate_hit_probability_bps(output: &serde_json::Value, strategy: &str) -> u16 {
    let leg_count = output
        .get("legs")
        .and_then(serde_json::Value::as_array)
        .map(|legs| legs.len())
        .unwrap_or(0);
    match strategy {
        "PORTFOLIO_CRASH_SHIELD" => 2_500,
        "CONVEX_TAIL_LADDER" => 3_500,
        "EXPIRY_MOVE_NOTE" => 4_500,
        "MOONSHOT_UPSIDE" => 2_000,
        "UPSIDE_STEP_LADDER" => 3_200,
        "DOWNSIDE_STEP_LADDER" => 3_200,
        "CENTER_BAND_CONDOR" => 5_500,
        "BREAKOUT_PROTECTION" => 4_000,
        _ => (leg_count as u16).saturating_mul(800).min(6_000),
    }
}

fn estimate_worst_case_improvement(
    output: &serde_json::Value,
    strategy: &str,
) -> Result<u64, Box<dyn std::error::Error>> {
    let max_payout = json_string_u64(output, "maxGrossPayoutRaw")?;
    let improvement_bps = match strategy {
        "PORTFOLIO_CRASH_SHIELD" => 9_000u64,
        "CONVEX_TAIL_LADDER" => 7_000u64,
        "EXPIRY_MOVE_NOTE" => 5_000u64,
        "MOONSHOT_UPSIDE" => 6_000u64,
        "UPSIDE_STEP_LADDER" => 6_500u64,
        "DOWNSIDE_STEP_LADDER" => 6_500u64,
        "CENTER_BAND_CONDOR" => 2_000u64,
        "BREAKOUT_PROTECTION" => 7_000u64,
        _ => 5_000u64,
    };
    Ok(((max_payout as u128) * improvement_bps as u128 / 10_000).min(u64::MAX as u128) as u64)
}

fn estimate_complexity_penalty_bps(output: &serde_json::Value) -> u16 {
    let leg_count = output
        .get("legs")
        .and_then(serde_json::Value::as_array)
        .map(|legs| legs.len())
        .unwrap_or(0);
    match leg_count {
        0..=2 => 50,
        3..=4 => 100,
        5..=6 => 200,
        _ => 350,
    }
}

#[allow(clippy::too_many_arguments)]
async fn compile_breakout_strategy_json_from_market(
    args: &CompileStrategyJsonArgs,
    selected: &SelectedMarket<'_>,
    predict: &SuiObjectInfo,
    oracle: &SuiObjectInfo,
    clock: &SuiObjectInfo,
    rpc: &SuiRpcClient,
    asset: &QuoteAssetDisplay,
    warnings: Vec<String>,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let budget_raw = parse_dusdc_to_raw(&args.budget_dusdc)?;
    let style = BreakoutStyle::from_api_value(&args.style)?;
    let compile_sender = compile_strategy_sender(&args.owner);
    let probe_quantity = 1_000_000u64;
    let StrategyStrikeSet { k1, k2, k3, k4, .. } = resolve_strategy_strikes(args, selected)?;

    let probe_compiled = compile_breakout(k1, k2, k3, k4, probe_quantity, probe_quantity)?;
    let probe_plan = build_quote_plan(selected, &probe_compiled)?;
    let probe_tx_kind = build_quote_tx_kind(
        &probe_plan,
        QuoteObjectRefs { predict, oracle, clock },
        &compile_sender,
    )?;
    let probe_response =
        rpc.dev_inspect_transaction_kind(&probe_tx_kind.sender, &probe_tx_kind.tx_kind_b64).await?;
    let probe_costs = quote_costs_from_response(&probe_tx_kind, &probe_response)?;

    if probe_costs.len() != 4 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("expected 4 probe quote legs, got {}", probe_costs.len()),
        )
        .into());
    }

    let ask_inputs = BreakoutAskInputs {
        down_tail_ask_raw: infer_ask_price_raw(probe_costs[0].0, probe_quantity),
        downside_range_ask_raw: infer_ask_price_raw(probe_costs[1].0, probe_quantity),
        upside_range_ask_raw: infer_ask_price_raw(probe_costs[2].0, probe_quantity),
        up_tail_ask_raw: infer_ask_price_raw(probe_costs[3].0, probe_quantity),
    };
    let optimized = optimize_breakout_quantities(budget_raw, ask_inputs, style)?;
    let final_compiled = compile_breakout(
        k1,
        k2,
        k3,
        k4,
        optimized.down_tail_quantity,
        optimized.downside_range_quantity,
    )?;
    let final_plan = build_quote_plan(selected, &final_compiled)?;
    let final_tx_kind = build_quote_tx_kind(
        &final_plan,
        QuoteObjectRefs { predict, oracle, clock },
        &compile_sender,
    )?;
    let final_response =
        rpc.dev_inspect_transaction_kind(&final_tx_kind.sender, &final_tx_kind.tx_kind_b64).await?;
    let final_costs = quote_costs_from_response(&final_tx_kind, &final_response)?;

    if final_costs.len() != 4 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("final quote returned {} legs", final_costs.len()),
        )
        .into());
    }

    let total_cost_raw = final_costs
        .iter()
        .try_fold(0u64, |acc, (cost, _)| acc.checked_add(*cost))
        .ok_or_else(|| io::Error::other("total cost overflow"))?;
    let max_gross_payout_raw = optimized.down_tail_quantity.max(optimized.downside_range_quantity);
    let max_loss_raw = total_cost_raw;
    let max_net_payout_raw = max_gross_payout_raw.saturating_sub(total_cost_raw);
    let scenario_1 = format!("BTC settles <= {}", position_service::format_raw_price_e9(k1.raw));
    let scenario_2 = format!(
        "{} < BTC settles <= {}",
        position_service::format_raw_price_e9(k1.raw),
        position_service::format_raw_price_e9(k2.raw)
    );
    let scenario_3 = format!(
        "{} < BTC settles < {}",
        position_service::format_raw_price_e9(k2.raw),
        position_service::format_raw_price_e9(k3.raw)
    );
    let scenario_4 = format!(
        "{} <= BTC settles < {}",
        position_service::format_raw_price_e9(k3.raw),
        position_service::format_raw_price_e9(k4.raw)
    );
    let scenario_5 = format!("BTC settles >= {}", position_service::format_raw_price_e9(k4.raw));
    let compiled_strategy_id = format!(
        "breakout:{}:{}:{}:{}:{}",
        args.owner,
        selected.oracle_id,
        selected.expiry.timestamp_millis(),
        total_cost_raw,
        style.api_value()
    );

    Ok(serde_json::json!({
        "ok": true,
        "compiledStrategyId": compiled_strategy_id,
        "strategy": "BREAKOUT_PROTECTION",
        "network": "sui:testnet",
        "owner": args.owner,
        "oracleId": selected.oracle_id,
        "expiry": selected.expiry.to_rfc3339(),
        "spot": position_service::format_raw_price_e9(selected.spot_raw),
        "style": style.api_value(),
        "styleRatioBps": optimized.style_ratio_bps,
        "slippageBps": args.slippage_bps,
        "budgetRaw": budget_raw.to_string(),
        "budgetDisplay": asset.format_amount(budget_raw),
        "premiumRequiredRaw": total_cost_raw.to_string(),
        "premiumRequiredDisplay": asset.format_amount(total_cost_raw),
        "maxLossRaw": max_loss_raw.to_string(),
        "maxLossDisplay": asset.format_amount(max_loss_raw),
        "maxGrossPayoutRaw": max_gross_payout_raw.to_string(),
        "maxGrossPayoutDisplay": asset.format_amount(max_gross_payout_raw),
        "maxNetPayoutRaw": max_net_payout_raw.to_string(),
        "maxNetPayoutDisplay": asset.format_amount(max_net_payout_raw),
        "strikes": {
            "k1": position_service::format_raw_price_e9(k1.raw),
            "k2": position_service::format_raw_price_e9(k2.raw),
            "k3": position_service::format_raw_price_e9(k3.raw),
            "k4": position_service::format_raw_price_e9(k4.raw),
            "k1Raw": k1.raw.to_string(),
            "k2Raw": k2.raw.to_string(),
            "k3Raw": k3.raw.to_string(),
            "k4Raw": k4.raw.to_string()
        },
        "legs": [
            compile_json_leg_down(k1.raw, optimized.down_tail_quantity, final_costs[0].0, ask_inputs.down_tail_ask_raw, asset),
            compile_json_leg_range("moderate_downside", k1.raw, k2.raw, optimized.downside_range_quantity, final_costs[1].0, ask_inputs.downside_range_ask_raw, asset),
            compile_json_leg_range("moderate_upside", k3.raw, k4.raw, optimized.upside_range_quantity, final_costs[2].0, ask_inputs.upside_range_ask_raw, asset),
            compile_json_leg_up(k4.raw, optimized.up_tail_quantity, final_costs[3].0, ask_inputs.up_tail_ask_raw, asset)
        ],
        "payoffTable": [
            payoff_json(&scenario_1, max_gross_payout_raw, total_cost_raw, asset),
            payoff_json(&scenario_2, optimized.downside_range_quantity, total_cost_raw, asset),
            payoff_json(&scenario_3, 0, total_cost_raw, asset),
            payoff_json(&scenario_4, optimized.upside_range_quantity, total_cost_raw, asset),
            payoff_json(&scenario_5, max_gross_payout_raw, total_cost_raw, asset)
        ],
        "warnings": warnings
    }))
}

fn quote_costs_from_response(
    tx_kind: &QuoteTxKind,
    response: &serde_json::Value,
) -> Result<Vec<(u64, u64)>, Box<dyn std::error::Error>> {
    let status = response
        .get("effects")
        .and_then(|effects| effects.get("status"))
        .and_then(|status| status.get("status"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    if status != "success" {
        return Err(io::Error::other(devinspect_failure_summary(response)).into());
    }

    let results = response
        .get("results")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing devInspect results"))?;
    let mut out = Vec::with_capacity(tx_kind.quote_result_command_indices.len());

    for command_idx in &tx_kind.quote_result_command_indices {
        let result = results.get(*command_idx).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("missing result for command {command_idx}"),
            )
        })?;
        let return_values =
            result.get("returnValues").and_then(serde_json::Value::as_array).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("missing returnValues for command {command_idx}"),
                )
            })?;
        if return_values.len() != 2 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("expected 2 quote returns, got {}", return_values.len()),
            )
            .into());
        }
        let mint_cost_raw = position_service::decode_devinspect_u64(&return_values[0])?;
        let redeem_payout_raw = position_service::decode_devinspect_u64(&return_values[1])?;
        out.push((mint_cost_raw, redeem_payout_raw));
    }

    Ok(out)
}

fn infer_ask_price_raw(cost_raw: u64, quantity: u64) -> u64 {
    if quantity == 0 {
        return 0;
    }
    (((cost_raw as u128) * 1_000_000_000u128) / quantity as u128).max(1).min(u64::MAX as u128)
        as u64
}

#[allow(clippy::too_many_arguments)]
async fn quote_single_range_ask_raw(
    args: &CompileStrategyJsonArgs,
    selected: &SelectedMarket<'_>,
    predict: &SuiObjectInfo,
    oracle: &SuiObjectInfo,
    clock: &SuiObjectInfo,
    rpc: &SuiRpcClient,
    lower_raw: u64,
    upper_raw: u64,
    probe_quantity: u64,
) -> Result<u64, Box<dyn std::error::Error>> {
    let probe_compiled = compile_bucket_payoff(&[PayoffBucket::new(
        Some(Strike { raw: lower_raw }),
        Some(Strike { raw: upper_raw }),
        probe_quantity,
    )])?;
    let probe_plan = build_quote_plan(selected, &probe_compiled)?;
    let probe_tx_kind =
        build_quote_tx_kind(&probe_plan, QuoteObjectRefs { predict, oracle, clock }, &args.owner)?;
    let probe_response =
        rpc.dev_inspect_transaction_kind(&probe_tx_kind.sender, &probe_tx_kind.tx_kind_b64).await?;
    let probe_costs = quote_costs_from_response(&probe_tx_kind, &probe_response)?;
    let Some((mint_cost_raw, _)) = probe_costs.first() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "single range quote returned no costs",
        )
        .into());
    };
    Ok(infer_ask_price_raw(*mint_cost_raw, probe_quantity))
}

#[allow(clippy::too_many_arguments)]
async fn quote_single_binary_ask_raw(
    args: &CompileStrategyJsonArgs,
    selected: &SelectedMarket<'_>,
    predict: &SuiObjectInfo,
    oracle: &SuiObjectInfo,
    clock: &SuiObjectInfo,
    rpc: &SuiRpcClient,
    strike_raw: u64,
    is_up: bool,
    probe_quantity: u64,
) -> Result<u64, Box<dyn std::error::Error>> {
    let probe_compiled = if is_up {
        compile_bucket_payoff(&[PayoffBucket::new(
            Some(Strike { raw: strike_raw }),
            None,
            probe_quantity,
        )])?
    } else {
        compile_bucket_payoff(&[PayoffBucket::new(
            None,
            Some(Strike { raw: strike_raw }),
            probe_quantity,
        )])?
    };
    let probe_plan = build_quote_plan(selected, &probe_compiled)?;
    let probe_tx_kind =
        build_quote_tx_kind(&probe_plan, QuoteObjectRefs { predict, oracle, clock }, &args.owner)?;
    let probe_response =
        rpc.dev_inspect_transaction_kind(&probe_tx_kind.sender, &probe_tx_kind.tx_kind_b64).await?;
    let probe_costs = quote_costs_from_response(&probe_tx_kind, &probe_response)?;
    let Some((mint_cost_raw, _)) = probe_costs.first() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "single binary quote returned no costs",
        )
        .into());
    };
    Ok(infer_ask_price_raw(*mint_cost_raw, probe_quantity))
}

fn compile_strategy_sender(owner: &str) -> String {
    if owner.trim().is_empty() {
        "0x0000000000000000000000000000000000000000000000000000000000000000".to_string()
    } else {
        owner.to_string()
    }
}

fn resolve_strategy_strikes(
    args: &CompileStrategyJsonArgs,
    selected: &SelectedMarket<'_>,
) -> Result<StrategyStrikeSet, Box<dyn std::error::Error>> {
    let center = selected
        .grid
        .snap_nearest(selected.spot_raw)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "spot cannot be snapped"))?;
    let custom_values =
        [args.custom_k1_price, args.custom_k2_price, args.custom_k3_price, args.custom_k4_price];
    let custom_count = custom_values.iter().filter(|value| value.is_some()).count();

    if custom_count > 0 {
        if custom_count != 4 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "custom strike override requires all of K1, K2, K3, and K4",
            )
            .into());
        }

        let [Some(k1_display), Some(k2_display), Some(k3_display), Some(k4_display)] =
            custom_values
        else {
            unreachable!();
        };

        let to_strike = |display: DisplayPrice| -> Result<Strike, Box<dyn std::error::Error>> {
            let raw = selected.grid.scale.raw_from_display(display).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "invalid custom strike")
            })?;
            selected.grid.snap_nearest(raw).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "custom strike is outside the market strike grid",
                )
                .into()
            })
        };

        let k1 = to_strike(k1_display)?;
        let k2 = to_strike(k2_display)?;
        let k3 = to_strike(k3_display)?;
        let k4 = to_strike(k4_display)?;

        if !(k1.raw < k2.raw && k2.raw < center.raw && center.raw < k3.raw && k3.raw < k4.raw) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "custom strikes must satisfy K1 < K2 < center < K3 < K4",
            )
            .into());
        }

        return Ok(StrategyStrikeSet { center, k1, k2, k3, k4 });
    }

    let strikes = selected.grid.centered_strikes_by_display_step(
        selected.spot_raw,
        args.bucket_step,
        args.levels_each_side,
    )?;
    let center_idx = strikes
        .iter()
        .position(|strike| strike.raw == center.raw)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "center strike missing"))?;
    if center_idx < 2 || center_idx + 2 >= strikes.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "not enough strikes around spot for strategy",
        )
        .into());
    }
    Ok(StrategyStrikeSet {
        center,
        k1: strikes[center_idx - 2],
        k2: strikes[center_idx - 1],
        k3: strikes[center_idx + 1],
        k4: strikes[center_idx + 2],
    })
}

#[allow(clippy::too_many_arguments)]
async fn compile_advanced_strategy_json_from_market(
    args: &CompileStrategyJsonArgs,
    strategy_kind: AdvancedStrategyKind,
    selected: &SelectedMarket<'_>,
    predict: &SuiObjectInfo,
    oracle: &SuiObjectInfo,
    clock: &SuiObjectInfo,
    rpc: &SuiRpcClient,
    asset: &QuoteAssetDisplay,
    warnings: Vec<String>,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let budget_raw = parse_dusdc_to_raw(&args.budget_dusdc)?;
    let StrategyStrikeSet { center, k1, k2, k3, k4 } = resolve_strategy_strikes(args, selected)?;
    let probe_quantity = 1_000_000u64;

    let advanced_result = match strategy_kind {
        AdvancedStrategyKind::PortfolioCrashShield => {
            let exposure_raw = dusdc_f64_to_raw(args.portfolio_exposure_dusdc)?;
            let ask_down_tail = quote_single_binary_ask_raw(
                args,
                selected,
                predict,
                oracle,
                clock,
                rpc,
                k1.raw,
                false,
                probe_quantity,
            )
            .await?;
            let ask_lower_range = quote_single_range_ask_raw(
                args,
                selected,
                predict,
                oracle,
                clock,
                rpc,
                k1.raw,
                k2.raw,
                probe_quantity,
            )
            .await?;
            let ask_mild_range = quote_single_range_ask_raw(
                args,
                selected,
                predict,
                oracle,
                clock,
                rpc,
                k2.raw,
                k3.raw,
                probe_quantity,
            )
            .await?;
            compile_portfolio_crash_shield(PortfolioCrashShieldInput {
                spot_raw: selected.spot_raw,
                exposure_raw,
                budget_raw,
                over_hedge_cap_bps: args.over_hedge_cap_bps,
                gamma_bps: 10_000,
                down_tail_strike_raw: k1.raw,
                lower_range_upper_raw: k2.raw,
                mild_range_upper_raw: Some(k3.raw),
                down_tail_ask_raw: ask_down_tail,
                lower_range_ask_raw: ask_lower_range,
                mild_range_ask_raw: Some(ask_mild_range),
            })?
        }
        AdvancedStrategyKind::ExpiryMoveNote => {
            let ask_down_tail = quote_single_binary_ask_raw(
                args,
                selected,
                predict,
                oracle,
                clock,
                rpc,
                k1.raw,
                false,
                probe_quantity,
            )
            .await?;
            let ask_lower_range = quote_single_range_ask_raw(
                args,
                selected,
                predict,
                oracle,
                clock,
                rpc,
                k1.raw,
                k2.raw,
                probe_quantity,
            )
            .await?;
            let ask_upper_range = quote_single_range_ask_raw(
                args,
                selected,
                predict,
                oracle,
                clock,
                rpc,
                k3.raw,
                k4.raw,
                probe_quantity,
            )
            .await?;
            let ask_up_tail = quote_single_binary_ask_raw(
                args,
                selected,
                predict,
                oracle,
                clock,
                rpc,
                k4.raw,
                true,
                probe_quantity,
            )
            .await?;
            compile_expiry_move_note(ExpiryMoveNoteInput {
                spot_raw: selected.spot_raw,
                budget_raw,
                k1_raw: k1.raw,
                k2_raw: k2.raw,
                k3_raw: k3.raw,
                k4_raw: k4.raw,
                down_tail_ask_raw: ask_down_tail,
                lower_range_ask_raw: ask_lower_range,
                upper_range_ask_raw: ask_upper_range,
                up_tail_ask_raw: ask_up_tail,
            })?
        }
        AdvancedStrategyKind::MoonshotUpside => {
            let ask_upper_range = quote_single_range_ask_raw(
                args,
                selected,
                predict,
                oracle,
                clock,
                rpc,
                k3.raw,
                k4.raw,
                probe_quantity,
            )
            .await?;
            let ask_up_tail = quote_single_binary_ask_raw(
                args,
                selected,
                predict,
                oracle,
                clock,
                rpc,
                k4.raw,
                true,
                probe_quantity,
            )
            .await?;
            compile_moonshot_upside(MoonshotUpsideInput {
                spot_raw: selected.spot_raw,
                budget_raw,
                k3_raw: k3.raw,
                k4_raw: k4.raw,
                upper_range_ask_raw: ask_upper_range,
                up_tail_ask_raw: ask_up_tail,
                range_weight_bps: args.moonshot_range_weight_bps,
                tail_gamma_bps: args.moonshot_tail_gamma_bps,
            })?
        }
        AdvancedStrategyKind::UpsideStepLadder => {
            let ask_near_up_range = quote_single_range_ask_raw(
                args,
                selected,
                predict,
                oracle,
                clock,
                rpc,
                center.raw,
                k3.raw,
                probe_quantity,
            )
            .await?;
            let ask_upper_range = quote_single_range_ask_raw(
                args,
                selected,
                predict,
                oracle,
                clock,
                rpc,
                k3.raw,
                k4.raw,
                probe_quantity,
            )
            .await?;
            let ask_up_tail = quote_single_binary_ask_raw(
                args,
                selected,
                predict,
                oracle,
                clock,
                rpc,
                k4.raw,
                true,
                probe_quantity,
            )
            .await?;
            compile_upside_step_ladder(UpsideStepLadderInput {
                spot_raw: selected.spot_raw,
                budget_raw,
                center_raw: center.raw,
                k3_raw: k3.raw,
                k4_raw: k4.raw,
                near_up_range_ask_raw: ask_near_up_range,
                upper_range_ask_raw: ask_upper_range,
                up_tail_ask_raw: ask_up_tail,
                near_range_weight_bps: args.upside_near_range_weight_bps,
                upper_range_weight_bps: args.upside_upper_range_weight_bps,
                tail_gamma_bps: args.upside_tail_gamma_bps,
            })?
        }
        AdvancedStrategyKind::DownsideStepLadder => {
            let ask_down_tail = quote_single_binary_ask_raw(
                args,
                selected,
                predict,
                oracle,
                clock,
                rpc,
                k1.raw,
                false,
                probe_quantity,
            )
            .await?;
            let ask_lower_range = quote_single_range_ask_raw(
                args,
                selected,
                predict,
                oracle,
                clock,
                rpc,
                k1.raw,
                k2.raw,
                probe_quantity,
            )
            .await?;
            let ask_near_down_range = quote_single_range_ask_raw(
                args,
                selected,
                predict,
                oracle,
                clock,
                rpc,
                k2.raw,
                center.raw,
                probe_quantity,
            )
            .await?;
            compile_downside_step_ladder(DownsideStepLadderInput {
                spot_raw: selected.spot_raw,
                budget_raw,
                k1_raw: k1.raw,
                k2_raw: k2.raw,
                center_raw: center.raw,
                down_tail_ask_raw: ask_down_tail,
                lower_range_ask_raw: ask_lower_range,
                near_down_range_ask_raw: ask_near_down_range,
                near_range_weight_bps: args.downside_near_range_weight_bps,
                lower_range_weight_bps: args.downside_lower_range_weight_bps,
                tail_gamma_bps: args.downside_step_tail_gamma_bps,
            })?
        }
        AdvancedStrategyKind::CenterBandCondor => {
            let ask_lower_wing = quote_single_range_ask_raw(
                args,
                selected,
                predict,
                oracle,
                clock,
                rpc,
                k1.raw,
                k2.raw,
                probe_quantity,
            )
            .await?;
            let ask_lower_center_range = quote_single_range_ask_raw(
                args,
                selected,
                predict,
                oracle,
                clock,
                rpc,
                k2.raw,
                center.raw,
                probe_quantity,
            )
            .await?;
            let ask_upper_center_range = quote_single_range_ask_raw(
                args,
                selected,
                predict,
                oracle,
                clock,
                rpc,
                center.raw,
                k3.raw,
                probe_quantity,
            )
            .await?;
            let ask_upper_wing = quote_single_range_ask_raw(
                args,
                selected,
                predict,
                oracle,
                clock,
                rpc,
                k3.raw,
                k4.raw,
                probe_quantity,
            )
            .await?;
            compile_center_band_condor(CenterBandCondorInput {
                budget_raw,
                k1_raw: k1.raw,
                k2_raw: k2.raw,
                center_raw: center.raw,
                k3_raw: k3.raw,
                k4_raw: k4.raw,
                lower_wing_ask_raw: ask_lower_wing,
                lower_center_ask_raw: ask_lower_center_range,
                upper_center_ask_raw: ask_upper_center_range,
                upper_wing_ask_raw: ask_upper_wing,
                center_weight_bps: args.condor_center_weight_bps,
            })?
        }
        AdvancedStrategyKind::NearBarrierProxy => {
            let ask_down_tail = if args.barrier_side.eq_ignore_ascii_case("down") {
                Some(
                    quote_single_binary_ask_raw(
                        args,
                        selected,
                        predict,
                        oracle,
                        clock,
                        rpc,
                        k1.raw,
                        false,
                        probe_quantity,
                    )
                    .await?,
                )
            } else {
                None
            };
            let ask_lower_range = if args.barrier_side.eq_ignore_ascii_case("down") {
                Some(
                    quote_single_range_ask_raw(
                        args,
                        selected,
                        predict,
                        oracle,
                        clock,
                        rpc,
                        k1.raw,
                        k2.raw,
                        probe_quantity,
                    )
                    .await?,
                )
            } else {
                None
            };
            let ask_upper_range = if args.barrier_side.eq_ignore_ascii_case("up") {
                Some(
                    quote_single_range_ask_raw(
                        args,
                        selected,
                        predict,
                        oracle,
                        clock,
                        rpc,
                        k3.raw,
                        k4.raw,
                        probe_quantity,
                    )
                    .await?,
                )
            } else {
                None
            };
            let ask_up_tail = if args.barrier_side.eq_ignore_ascii_case("up") {
                Some(
                    quote_single_binary_ask_raw(
                        args,
                        selected,
                        predict,
                        oracle,
                        clock,
                        rpc,
                        k4.raw,
                        true,
                        probe_quantity,
                    )
                    .await?,
                )
            } else {
                None
            };
            compile_near_barrier_proxy(NearBarrierProxyInput {
                spot_raw: selected.spot_raw,
                budget_raw,
                side: BarrierSide::from_api_value(&args.barrier_side)?,
                k1_raw: k1.raw,
                k2_raw: k2.raw,
                k3_raw: k3.raw,
                k4_raw: k4.raw,
                down_tail_ask_raw: ask_down_tail.unwrap_or(0),
                lower_range_ask_raw: ask_lower_range.unwrap_or(0),
                upper_range_ask_raw: ask_upper_range.unwrap_or(0),
                up_tail_ask_raw: ask_up_tail.unwrap_or(0),
                near_range_weight_bps: args.barrier_near_range_weight_bps,
                tail_gamma_bps: args.barrier_tail_gamma_bps,
            })?
        }
        AdvancedStrategyKind::DownsideConvexity => {
            let ask_down_tail = quote_single_binary_ask_raw(
                args,
                selected,
                predict,
                oracle,
                clock,
                rpc,
                k1.raw,
                false,
                probe_quantity,
            )
            .await?;
            let ask_lower_range = quote_single_range_ask_raw(
                args,
                selected,
                predict,
                oracle,
                clock,
                rpc,
                k1.raw,
                k2.raw,
                probe_quantity,
            )
            .await?;
            compile_downside_convexity(DownsideConvexityInput {
                spot_raw: selected.spot_raw,
                budget_raw,
                k1_raw: k1.raw,
                k2_raw: k2.raw,
                down_tail_ask_raw: ask_down_tail,
                lower_range_ask_raw: ask_lower_range,
                range_weight_bps: args.downside_range_weight_bps,
                tail_gamma_bps: args.downside_tail_gamma_bps,
            })?
        }
        AdvancedStrategyKind::ConvexTailLadder => {
            let ask_down_tail = quote_single_binary_ask_raw(
                args,
                selected,
                predict,
                oracle,
                clock,
                rpc,
                k1.raw,
                false,
                probe_quantity,
            )
            .await?;
            let ask_lower_range = quote_single_range_ask_raw(
                args,
                selected,
                predict,
                oracle,
                clock,
                rpc,
                k1.raw,
                k2.raw,
                probe_quantity,
            )
            .await?;
            let ask_upper_range = quote_single_range_ask_raw(
                args,
                selected,
                predict,
                oracle,
                clock,
                rpc,
                k3.raw,
                k4.raw,
                probe_quantity,
            )
            .await?;
            let ask_up_tail = quote_single_binary_ask_raw(
                args,
                selected,
                predict,
                oracle,
                clock,
                rpc,
                k4.raw,
                true,
                probe_quantity,
            )
            .await?;
            compile_convex_tail_ladder(ConvexTailLadderInput {
                spot_raw: selected.spot_raw,
                budget_raw,
                dead_zone_bps: args.dead_zone_bps,
                gamma_bps: args.convex_gamma_bps,
                k1_raw: k1.raw,
                k2_raw: k2.raw,
                k3_raw: k3.raw,
                k4_raw: k4.raw,
                down_tail_ask_raw: ask_down_tail,
                lower_range_ask_raw: ask_lower_range,
                upper_range_ask_raw: ask_upper_range,
                up_tail_ask_raw: ask_up_tail,
            })?
        }
        AdvancedStrategyKind::SmartBudgetSelector => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "SMART_BUDGET_SELECTOR must be compiled through the selector path",
            )
            .into());
        }
        AdvancedStrategyKind::RangeConviction => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "RANGE_CONVICTION is not wired into compile-strategy-json yet",
            )
            .into());
        }
    };

    let final_compiled = advanced_result_to_compiled_payoff(&advanced_result)?;
    let final_plan = build_quote_plan(selected, &final_compiled)?;
    let final_tx_kind =
        build_quote_tx_kind(&final_plan, QuoteObjectRefs { predict, oracle, clock }, &args.owner)?;
    let final_response =
        rpc.dev_inspect_transaction_kind(&final_tx_kind.sender, &final_tx_kind.tx_kind_b64).await?;
    let final_costs = quote_costs_from_response(&final_tx_kind, &final_response)?;

    if final_costs.len() != advanced_result.legs.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "final quote returned {} legs, expected {}",
                final_costs.len(),
                advanced_result.legs.len()
            ),
        )
        .into());
    }

    let total_cost_raw = final_costs
        .iter()
        .try_fold(0u64, |acc, (cost, _)| acc.checked_add(*cost))
        .ok_or_else(|| io::Error::other("total cost overflow"))?;
    let max_gross_payout_raw = final_compiled.max_payout_quantity;
    let max_loss_raw = total_cost_raw;
    let max_net_payout_raw = max_gross_payout_raw.saturating_sub(total_cost_raw);

    let mut all_warnings = warnings;
    all_warnings.extend(advanced_result.warnings.clone());
    all_warnings.push(
        "Advanced strategy quantities are generated by StructX optimizer and re-quoted live before wallet signing."
            .to_string(),
    );
    if total_cost_raw > budget_raw {
        all_warnings.push(format!(
            "Final quote exceeds budget: required {}, budget {}. Transaction build should refuse unless user increases budget.",
            total_cost_raw, budget_raw
        ));
    }

    let compiled_strategy_id = format!(
        "{}:{}:{}:{}:{}",
        strategy_kind.api_value(),
        args.owner,
        selected.oracle_id,
        selected.expiry.timestamp_millis(),
        total_cost_raw
    );

    let legs_json = advanced_result
        .legs
        .iter()
        .zip(final_costs.iter())
        .map(|(leg, (premium_raw, _))| advanced_leg_json(leg, *premium_raw, asset))
        .collect::<Vec<_>>();
    let payoff_table = advanced_payoff_table_json(&advanced_result.legs, total_cost_raw, asset);

    Ok(serde_json::json!({
        "ok": true,
        "compiledStrategyId": compiled_strategy_id,
        "strategy": strategy_kind.api_value(),
        "network": "sui:testnet",
        "owner": args.owner,
        "oracleId": selected.oracle_id,
        "expiry": selected.expiry.to_rfc3339(),
        "spot": position_service::format_raw_price_e9(selected.spot_raw),
        "style": args.style,
        "styleRatioBps": 0,
        "slippageBps": args.slippage_bps,
        "budgetRaw": budget_raw.to_string(),
        "budgetDisplay": asset.format_amount(budget_raw),
        "premiumRequiredRaw": total_cost_raw.to_string(),
        "premiumRequiredDisplay": asset.format_amount(total_cost_raw),
        "maxLossRaw": max_loss_raw.to_string(),
        "maxLossDisplay": asset.format_amount(max_loss_raw),
        "maxGrossPayoutRaw": max_gross_payout_raw.to_string(),
        "maxGrossPayoutDisplay": asset.format_amount(max_gross_payout_raw),
        "maxNetPayoutRaw": max_net_payout_raw.to_string(),
        "maxNetPayoutDisplay": asset.format_amount(max_net_payout_raw),
        "strikes": {
            "k1": position_service::format_raw_price_e9(k1.raw),
            "k2": position_service::format_raw_price_e9(k2.raw),
            "k3": position_service::format_raw_price_e9(k3.raw),
            "k4": position_service::format_raw_price_e9(k4.raw),
            "k1Raw": k1.raw.to_string(),
            "k2Raw": k2.raw.to_string(),
            "k3Raw": k3.raw.to_string(),
            "k4Raw": k4.raw.to_string()
        },
        "advanced": {
            "requestedBudgetRaw": advanced_result.requested_budget_raw.to_string(),
            "usedBudgetRaw": advanced_result.used_budget_raw.to_string(),
            "unusedBudgetRaw": advanced_result.unused_budget_raw.to_string(),
            "portfolioExposureDUSDC": args.portfolio_exposure_dusdc,
            "overHedgeCapBps": args.over_hedge_cap_bps,
            "deadZoneBps": args.dead_zone_bps,
            "convexGammaBps": args.convex_gamma_bps,
            "moonshotRangeWeightBps": args.moonshot_range_weight_bps,
            "moonshotTailGammaBps": args.moonshot_tail_gamma_bps,
            "downsideRangeWeightBps": args.downside_range_weight_bps,
            "downsideTailGammaBps": args.downside_tail_gamma_bps,
            "upsideNearRangeWeightBps": args.upside_near_range_weight_bps,
            "upsideUpperRangeWeightBps": args.upside_upper_range_weight_bps,
            "upsideTailGammaBps": args.upside_tail_gamma_bps,
            "downsideNearRangeWeightBps": args.downside_near_range_weight_bps,
            "downsideLowerRangeWeightBps": args.downside_lower_range_weight_bps,
            "downsideStepTailGammaBps": args.downside_step_tail_gamma_bps,
            "condorCenterWeightBps": args.condor_center_weight_bps
        },
        "legs": legs_json,
        "payoffTable": payoff_table,
        "warnings": all_warnings
    }))
}

fn advanced_result_to_compiled_payoff(
    result: &AdvancedCompileResult,
) -> Result<structx_core::CompiledPayoff, Box<dyn std::error::Error>> {
    let mut buckets = Vec::new();
    for leg in &result.legs {
        if leg.quantity == 0 {
            continue;
        }
        match leg.kind {
            AdvancedLegKind::Down => {
                let strike_raw = leg.strike_raw.ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "DOWN leg missing strike")
                })?;
                buckets.push(PayoffBucket::new(
                    None,
                    Some(Strike { raw: strike_raw }),
                    leg.quantity,
                ));
            }
            AdvancedLegKind::Up => {
                let strike_raw = leg.strike_raw.ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "UP leg missing strike")
                })?;
                buckets.push(PayoffBucket::new(
                    Some(Strike { raw: strike_raw }),
                    None,
                    leg.quantity,
                ));
            }
            AdvancedLegKind::Range => {
                let lower_raw = leg.lower_raw.ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "RANGE leg missing lower")
                })?;
                let upper_raw = leg.upper_raw.ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "RANGE leg missing upper")
                })?;
                buckets.push(PayoffBucket::new(
                    Some(Strike { raw: lower_raw }),
                    Some(Strike { raw: upper_raw }),
                    leg.quantity,
                ));
            }
        }
    }

    if buckets.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "advanced strategy produced no legs",
        )
        .into());
    }

    compile_bucket_payoff(&buckets).map_err(|err| err.into())
}

fn compile_json_leg_down(
    strike_raw: u64,
    quantity: u64,
    premium_raw: u64,
    ask_price_raw: u64,
    asset: &QuoteAssetDisplay,
) -> serde_json::Value {
    serde_json::json!({
        "kind": "DOWN",
        "role": "extreme_downside",
        "strike": position_service::format_raw_price_e9(strike_raw),
        "strikeRaw": strike_raw.to_string(),
        "quantityRaw": quantity.to_string(),
        "quantityDisplay": asset.format_amount(quantity),
        "askPriceRaw": ask_price_raw.to_string(),
        "premiumRaw": premium_raw.to_string(),
        "premiumDisplay": asset.format_amount(premium_raw)
    })
}

fn compile_json_leg_up(
    strike_raw: u64,
    quantity: u64,
    premium_raw: u64,
    ask_price_raw: u64,
    asset: &QuoteAssetDisplay,
) -> serde_json::Value {
    serde_json::json!({
        "kind": "UP",
        "role": "extreme_upside",
        "strike": position_service::format_raw_price_e9(strike_raw),
        "strikeRaw": strike_raw.to_string(),
        "quantityRaw": quantity.to_string(),
        "quantityDisplay": asset.format_amount(quantity),
        "askPriceRaw": ask_price_raw.to_string(),
        "premiumRaw": premium_raw.to_string(),
        "premiumDisplay": asset.format_amount(premium_raw)
    })
}

fn compile_json_leg_range(
    role: &str,
    lower_raw: u64,
    upper_raw: u64,
    quantity: u64,
    premium_raw: u64,
    ask_price_raw: u64,
    asset: &QuoteAssetDisplay,
) -> serde_json::Value {
    serde_json::json!({
        "kind": "RANGE",
        "role": role,
        "lower": position_service::format_raw_price_e9(lower_raw),
        "upper": position_service::format_raw_price_e9(upper_raw),
        "lowerRaw": lower_raw.to_string(),
        "upperRaw": upper_raw.to_string(),
        "quantityRaw": quantity.to_string(),
        "quantityDisplay": asset.format_amount(quantity),
        "askPriceRaw": ask_price_raw.to_string(),
        "premiumRaw": premium_raw.to_string(),
        "premiumDisplay": asset.format_amount(premium_raw)
    })
}

fn advanced_leg_json(
    leg: &AdvancedCompiledLeg,
    premium_raw: u64,
    asset: &QuoteAssetDisplay,
) -> serde_json::Value {
    match leg.kind {
        AdvancedLegKind::Down => serde_json::json!({
            "kind": "DOWN",
            "role": leg.role,
            "strike": position_service::format_raw_price_e9(leg.strike_raw.unwrap_or_default()),
            "strikeRaw": leg.strike_raw.unwrap_or_default().to_string(),
            "quantityRaw": leg.quantity.to_string(),
            "quantityDisplay": asset.format_amount(leg.quantity),
            "askPriceRaw": leg.ask_price_raw.to_string(),
            "premiumRaw": premium_raw.to_string(),
            "premiumDisplay": asset.format_amount(premium_raw),
            "midpoint": position_service::format_raw_price_e9(leg.midpoint_raw),
            "midpointRaw": leg.midpoint_raw.to_string(),
            "weightE6": leg.weight_e6.to_string(),
            "maxQuantity": leg.max_quantity.map(|value| value.to_string())
        }),
        AdvancedLegKind::Up => serde_json::json!({
            "kind": "UP",
            "role": leg.role,
            "strike": position_service::format_raw_price_e9(leg.strike_raw.unwrap_or_default()),
            "strikeRaw": leg.strike_raw.unwrap_or_default().to_string(),
            "quantityRaw": leg.quantity.to_string(),
            "quantityDisplay": asset.format_amount(leg.quantity),
            "askPriceRaw": leg.ask_price_raw.to_string(),
            "premiumRaw": premium_raw.to_string(),
            "premiumDisplay": asset.format_amount(premium_raw),
            "midpoint": position_service::format_raw_price_e9(leg.midpoint_raw),
            "midpointRaw": leg.midpoint_raw.to_string(),
            "weightE6": leg.weight_e6.to_string(),
            "maxQuantity": leg.max_quantity.map(|value| value.to_string())
        }),
        AdvancedLegKind::Range => serde_json::json!({
            "kind": "RANGE",
            "role": leg.role,
            "lower": position_service::format_raw_price_e9(leg.lower_raw.unwrap_or_default()),
            "upper": position_service::format_raw_price_e9(leg.upper_raw.unwrap_or_default()),
            "lowerRaw": leg.lower_raw.unwrap_or_default().to_string(),
            "upperRaw": leg.upper_raw.unwrap_or_default().to_string(),
            "quantityRaw": leg.quantity.to_string(),
            "quantityDisplay": asset.format_amount(leg.quantity),
            "askPriceRaw": leg.ask_price_raw.to_string(),
            "premiumRaw": premium_raw.to_string(),
            "premiumDisplay": asset.format_amount(premium_raw),
            "midpoint": position_service::format_raw_price_e9(leg.midpoint_raw),
            "midpointRaw": leg.midpoint_raw.to_string(),
            "weightE6": leg.weight_e6.to_string(),
            "maxQuantity": leg.max_quantity.map(|value| value.to_string())
        }),
    }
}

fn advanced_payoff_table_json(
    legs: &[AdvancedCompiledLeg],
    total_cost_raw: u64,
    asset: &QuoteAssetDisplay,
) -> Vec<serde_json::Value> {
    legs.iter()
        .map(|leg| {
            let condition = match leg.kind {
                AdvancedLegKind::Down => format!(
                    "BTC settles <= {}",
                    position_service::format_raw_price_e9(leg.strike_raw.unwrap_or_default())
                ),
                AdvancedLegKind::Up => format!(
                    "BTC settles >= {}",
                    position_service::format_raw_price_e9(leg.strike_raw.unwrap_or_default())
                ),
                AdvancedLegKind::Range => format!(
                    "{} < BTC settles <= {}",
                    position_service::format_raw_price_e9(leg.lower_raw.unwrap_or_default()),
                    position_service::format_raw_price_e9(leg.upper_raw.unwrap_or_default())
                ),
            };
            let net_pnl_raw = leg.quantity as i128 - total_cost_raw as i128;
            serde_json::json!({
                "condition": condition,
                "grossPayoutRaw": leg.quantity.to_string(),
                "grossPayoutDisplay": asset.format_amount(leg.quantity),
                "netPnlRaw": net_pnl_raw.to_string(),
                "netPnlDisplay": format_signed_asset_amount(net_pnl_raw, asset)
            })
        })
        .collect()
}

fn payoff_json(
    condition: &str,
    gross_payout_raw: u64,
    premium_raw: u64,
    asset: &QuoteAssetDisplay,
) -> serde_json::Value {
    let net_pnl_raw = gross_payout_raw as i128 - premium_raw as i128;
    serde_json::json!({
        "condition": condition,
        "grossPayoutRaw": gross_payout_raw.to_string(),
        "grossPayoutDisplay": asset.format_amount(gross_payout_raw),
        "netPnlRaw": net_pnl_raw.to_string(),
        "netPnlDisplay": format_signed_asset_amount(net_pnl_raw, asset)
    })
}

fn format_signed_asset_amount(value: i128, asset: &QuoteAssetDisplay) -> String {
    if value < 0 {
        format!("-{}", asset.format_amount((-value) as u64))
    } else {
        asset.format_amount(value as u64)
    }
}

fn parse_dusdc_to_raw(value: &str) -> Result<u64, Box<dyn std::error::Error>> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "empty budget").into());
    }

    let mut parts = trimmed.split('.');
    let whole = parts.next().unwrap_or("0");
    let frac = parts.next().unwrap_or("");
    if parts.next().is_some() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid decimal budget").into());
    }

    let whole_raw = whole
        .parse::<u64>()?
        .checked_mul(1_000_000)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "budget overflow"))?;
    let mut frac_string = frac.to_string();
    if frac_string.len() > 6 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "dUSDC budget supports up to 6 decimal places",
        )
        .into());
    }
    while frac_string.len() < 6 {
        frac_string.push('0');
    }
    let frac_raw = if frac_string.is_empty() { 0 } else { frac_string.parse::<u64>()? };

    whole_raw
        .checked_add(frac_raw)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "budget overflow").into())
}

#[cfg(test)]
mod amount_parsing_tests {
    use super::parse_dusdc_to_raw;

    #[test]
    fn accepts_six_decimal_dusdc_amounts() {
        assert_eq!(parse_dusdc_to_raw("1.234567").unwrap(), 1_234_567);
    }

    #[test]
    fn rejects_precision_that_would_be_silently_truncated() {
        let error = parse_dusdc_to_raw("1.2345678").unwrap_err();
        assert!(error.to_string().contains("up to 6 decimal places"));
    }
}

fn dusdc_f64_to_raw(value: f64) -> Result<u64, Box<dyn std::error::Error>> {
    if !value.is_finite() || value <= 0.0 {
        return Err(
            io::Error::new(io::ErrorKind::InvalidInput, "dUSDC value must be positive").into()
        );
    }
    let raw = (value * 1_000_000.0).round();
    if raw > u64::MAX as f64 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "dUSDC overflow").into());
    }
    Ok(raw as u64)
}

pub mod market_service {
    pub use super::{build_freshness, list_markets_json_value};
}

pub mod strategy_compile_service {
    pub use super::{compile_strategy_json_value, CompileStrategyJsonArgs};
}

pub mod account_service {
    pub use super::manager_balance_json_value;
}

pub mod devinspect_service {
    pub use super::{
        devinspect_mint_breakout_json_value, devinspect_redeem_breakout_json_value,
        DevinspectMintBreakoutJsonArgs, DevinspectRedeemBreakoutJsonArgs,
    };
}

pub mod position_service {
    use std::fs;
    use std::io;
    use std::path::Path;
    use std::time::Duration as StdDuration;

    use deepbook_client::{
        ObjectOwnerKind, SuiObjectInfo, SuiRpcClient, DEFAULT_SUI_TESTNET_RPC_URL,
        PREDICT_MANAGER_TYPE,
    };
    use structx_core::{build_manager_positions_tx_kind, ManagerPositionRead, QuoteTxKind};

    pub async fn manager_positions_json_value(
        rpc_url: Option<String>,
        manager_id: &str,
        from_execution_json: &Path,
        sender: &str,
        expect_exact: bool,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let reads = load_position_reads_from_execution_json(from_execution_json)?;

        if reads.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "no PositionMinted or RangeMinted events found",
            )
            .into());
        }

        let rpc = SuiRpcClient::new(
            rpc_url.unwrap_or_else(|| DEFAULT_SUI_TESTNET_RPC_URL.to_string()),
            StdDuration::from_secs(20),
        )?;

        let manager = resolve_sui_object(&rpc, manager_id).await?;
        validate_predict_manager_object(&manager)?;

        let tx_kind = build_manager_positions_tx_kind(&reads, &manager, sender)?;
        let response =
            rpc.dev_inspect_transaction_kind(&tx_kind.sender, &tx_kind.tx_kind_b64).await?;

        summarize_manager_positions_response(&reads, &tx_kind, &response, expect_exact)
    }

    pub async fn demo_status_json_value(
        rpc_url: Option<String>,
        manager_id: &str,
        sender: &str,
        from_execution_json: &Path,
        expect_exact: bool,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let execution_json: serde_json::Value =
            serde_json::from_slice(&fs::read(from_execution_json)?)?;

        let digest =
            execution_json.get("digest").and_then(serde_json::Value::as_str).unwrap_or("unknown");

        let execution_status = execution_json
            .get("effects")
            .and_then(serde_json::Value::as_object)
            .and_then(|effects| effects.get("status"))
            .and_then(serde_json::Value::as_object)
            .and_then(|status| status.get("status"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown");

        if execution_status != "success" {
            return Err(io::Error::other("execution JSON status is not success").into());
        }

        let manager_positions = manager_positions_json_value(
            rpc_url.clone(),
            manager_id,
            from_execution_json,
            sender,
            expect_exact,
        )
        .await?;

        let manager_balance = super::manager_balance_json_value(
            rpc_url.unwrap_or_else(|| DEFAULT_SUI_TESTNET_RPC_URL.to_string()),
            manager_id.to_string(),
            sender.to_string(),
        )
        .await?;

        let verification_status = manager_positions
            .get("status")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown");

        let ok = verification_status == "ok" || verification_status == "partial";

        Ok(serde_json::json!({
            "ok": ok,
            "digest": digest,
            "executionStatus": execution_status,
            "managerBalanceRaw": manager_balance.get("balanceRaw").cloned().unwrap_or(serde_json::Value::Null),
            "managerBalanceDisplay": manager_balance.get("balanceDisplay").cloned().unwrap_or(serde_json::Value::Null),
            "positionVerification": manager_positions,
            "warnings": if verification_status == "partial" {
                vec!["Position verification is partial. Range legs verified. Binary manager-key verification is a known issue under investigation."]
            } else {
                Vec::<&str>::new()
            },
        }))
    }

    fn summarize_manager_positions_response(
        reads: &[ManagerPositionRead],
        tx_kind: &QuoteTxKind,
        response: &serde_json::Value,
        expect_exact: bool,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let status = response
            .get("effects")
            .and_then(serde_json::Value::as_object)
            .and_then(|effects| effects.get("status"))
            .and_then(serde_json::Value::as_object)
            .and_then(|status| status.get("status"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown");

        if status != "success" {
            return Err(io::Error::other(devinspect_failure_summary(response)).into());
        }

        let results =
            response.get("results").and_then(serde_json::Value::as_array).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "missing devInspect results")
            })?;

        let mut items = Vec::new();
        let mut ok_count = 0usize;
        let mut bad_count = 0usize;

        for (idx, read) in reads.iter().enumerate() {
            let command_idx = tx_kind.quote_result_command_indices.get(idx).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "missing command index")
            })?;

            let result = results.get(*command_idx).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("missing result for command {command_idx}"),
                )
            })?;

            let return_values = result
                .get("returnValues")
                .and_then(serde_json::Value::as_array)
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("missing returnValues for command {command_idx}"),
                    )
                })?;

            if return_values.len() != 1 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("expected 1 position return, got {}", return_values.len()),
                )
                .into());
            }

            let actual_quantity = decode_devinspect_u64(&return_values[0])?;
            let expected_quantity = position_expected_quantity(read);
            let accepted = if expect_exact {
                actual_quantity == expected_quantity
            } else {
                actual_quantity >= expected_quantity
            };

            if accepted {
                ok_count += 1;
            } else {
                bad_count += 1;
            }

            match read {
                ManagerPositionRead::Binary { strike_raw, is_up, .. } => {
                    items.push(serde_json::json!({
                        "index": idx,
                        "kind": "binary",
                        "direction": if *is_up { "up" } else { "down" },
                        "strike": format_raw_price_e9(*strike_raw),
                        "upper": serde_json::Value::Null,
                        "mintedQuantity": expected_quantity.to_string(),
                        "managerQuantity": actual_quantity.to_string(),
                        "check": if accepted { "ok" } else { "mismatch" }
                    }))
                }
                ManagerPositionRead::Range { lower_raw, upper_raw, .. } => {
                    items.push(serde_json::json!({
                        "index": idx,
                        "kind": "range",
                        "direction": serde_json::Value::Null,
                        "strike": format_raw_price_e9(*lower_raw),
                        "upper": format_raw_price_e9(*upper_raw),
                        "mintedQuantity": expected_quantity.to_string(),
                        "managerQuantity": actual_quantity.to_string(),
                        "check": if accepted { "ok" } else { "mismatch" }
                    }))
                }
            }
        }

        let summary_status = if bad_count == 0 { "ok" } else { "partial" };

        Ok(serde_json::json!({
            "status": summary_status,
            "verifiedCount": ok_count,
            "mismatchCount": bad_count,
            "mode": if expect_exact { "exact" } else { "actual >= minted" },
            "items": items,
        }))
    }

    pub fn load_position_reads_from_execution_json(
        path: &Path,
    ) -> Result<Vec<ManagerPositionRead>, Box<dyn std::error::Error>> {
        let value: serde_json::Value = serde_json::from_slice(&fs::read(path)?)?;

        let events = value
            .get("events")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing events array"))?;

        let mut reads = Vec::new();

        for event in events {
            let event_type = event.get("type").and_then(serde_json::Value::as_str).unwrap_or("");

            let parsed = event.get("parsedJson").unwrap_or(&serde_json::Value::Null);

            if event_type.ends_with("::predict::PositionMinted") {
                reads.push(ManagerPositionRead::Binary {
                    oracle_id: json_required_string(parsed, "oracle_id")?,
                    expiry_ms: json_required_u64(parsed, "expiry")?,
                    strike_raw: json_required_u64(parsed, "strike")?,
                    is_up: json_required_bool(parsed, "is_up")?,
                    expected_quantity: json_required_u64(parsed, "quantity")?,
                });
            } else if event_type.ends_with("::predict::RangeMinted") {
                reads.push(ManagerPositionRead::Range {
                    oracle_id: json_required_string(parsed, "oracle_id")?,
                    expiry_ms: json_required_u64(parsed, "expiry")?,
                    lower_raw: json_required_u64(parsed, "lower_strike")?,
                    upper_raw: json_required_u64(parsed, "higher_strike")?,
                    expected_quantity: json_required_u64(parsed, "quantity")?,
                });
            }
        }

        Ok(reads)
    }

    pub(crate) async fn resolve_sui_object(
        rpc: &SuiRpcClient,
        object_id: &str,
    ) -> Result<SuiObjectInfo, Box<dyn std::error::Error>> {
        let value = rpc.get_object(object_id).await?;
        Ok(SuiObjectInfo::from_get_object_result(object_id, value)?)
    }

    pub(crate) fn validate_predict_manager_object(
        manager: &SuiObjectInfo,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match manager.owner_kind {
            ObjectOwnerKind::AddressOwner | ObjectOwnerKind::Shared => {}
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "manager object has unsupported ownership kind: owner={}",
                        manager.owner_kind
                    ),
                )
                .into())
            }
        }

        let object_type = manager
            .object_type
            .as_deref()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "manager missing type"))?;

        if !object_type.contains(PREDICT_MANAGER_TYPE) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "manager type mismatch: expected substring `{PREDICT_MANAGER_TYPE}`, got `{object_type}`"
                ),
            )
            .into());
        }

        Ok(())
    }

    fn position_expected_quantity(read: &ManagerPositionRead) -> u64 {
        match read {
            ManagerPositionRead::Binary { expected_quantity, .. }
            | ManagerPositionRead::Range { expected_quantity, .. } => *expected_quantity,
        }
    }

    pub(crate) fn decode_devinspect_u64(
        value: &serde_json::Value,
    ) -> Result<u64, Box<dyn std::error::Error>> {
        let arr = value.as_array().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("return value is not array: {value}"),
            )
        })?;

        let bytes_value = arr.first().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "return value missing bytes")
        })?;

        let bytes_array = bytes_value.as_array().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("return bytes are not array: {bytes_value}"),
            )
        })?;

        if bytes_array.len() < 8 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("u64 return needs at least 8 bytes, got {}", bytes_array.len()),
            )
            .into());
        }

        let mut raw = 0u128;

        for (idx, byte_value) in bytes_array.iter().take(8).enumerate() {
            let byte = byte_value.as_u64().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid byte value: {byte_value}"),
                )
            })?;

            let byte = u8::try_from(byte)?;
            raw |= (byte as u128) << (idx * 8);
        }

        Ok(raw as u64)
    }

    fn devinspect_failure_summary(response: &serde_json::Value) -> String {
        let status_error = response
            .get("effects")
            .and_then(|effects| effects.get("status"))
            .and_then(|status| status.get("error"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown error");

        format!("devInspect failed: {status_error}")
    }

    pub(crate) fn json_required_string(
        value: &serde_json::Value,
        key: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        value.get(key).and_then(serde_json::Value::as_str).map(ToString::to_string).ok_or_else(
            || {
                io::Error::new(io::ErrorKind::InvalidData, format!("missing string field `{key}`"))
                    .into()
            },
        )
    }

    pub(crate) fn json_required_u64(
        value: &serde_json::Value,
        key: &str,
    ) -> Result<u64, Box<dyn std::error::Error>> {
        let item = value.get(key).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, format!("missing u64 field `{key}`"))
        })?;

        match item {
            serde_json::Value::String(s) => Ok(s.parse::<u64>()?),
            serde_json::Value::Number(n) => n.as_u64().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, format!("invalid u64 field `{key}`"))
                    .into()
            }),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid u64 field `{key}`"),
            )
            .into()),
        }
    }

    pub(crate) fn json_required_bool(
        value: &serde_json::Value,
        key: &str,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let item = value.get(key).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, format!("missing bool field `{key}`"))
        })?;

        match item {
            serde_json::Value::Bool(value) => Ok(*value),
            serde_json::Value::String(s) if s == "true" => Ok(true),
            serde_json::Value::String(s) if s == "false" => Ok(false),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid bool field `{key}`"),
            )
            .into()),
        }
    }

    pub(crate) fn format_raw_price_e9(raw: u64) -> String {
        let whole = raw / 1_000_000_000;
        let frac = raw % 1_000_000_000;

        if frac == 0 {
            return whole.to_string();
        }

        let mut frac_string = format!("{frac:09}");
        while frac_string.ends_with('0') {
            frac_string.pop();
        }

        format!("{whole}.{frac_string}")
    }
}

pub mod audit_service {
    use std::fs;
    use std::io;
    use std::path::Path;

    use deepbook_client::DUSDC_DECIMALS;
    use structx_core::QuoteAssetDisplay;

    pub fn audit_execution_json_value(
        from_execution_json: &Path,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let value: serde_json::Value = serde_json::from_slice(&fs::read(from_execution_json)?)?;

        let status = value
            .get("effects")
            .and_then(serde_json::Value::as_object)
            .and_then(|effects| effects.get("status"))
            .and_then(serde_json::Value::as_object)
            .and_then(|status| status.get("status"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown");

        let digest = value
            .get("digest")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown / recovered artifact");

        if status != "success" {
            return Err(io::Error::other("execution was not successful").into());
        }

        let events = value
            .get("events")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing events array"))?;

        let asset = QuoteAssetDisplay { symbol: "dUSDC".to_string(), decimals: DUSDC_DECIMALS };

        let mut total_cost_raw = 0u64;
        let mut minted_legs = Vec::new();

        for event in events {
            let event_type = event.get("type").and_then(serde_json::Value::as_str).unwrap_or("");

            let parsed = event.get("parsedJson").unwrap_or(&serde_json::Value::Null);

            if event_type.ends_with("::predict::PositionMinted") {
                let cost = json_required_u64(parsed, "cost")?;
                total_cost_raw = total_cost_raw
                    .checked_add(cost)
                    .ok_or_else(|| io::Error::other("total cost overflow"))?;

                minted_legs.push(serde_json::json!({
                    "kind": "binary",
                    "direction": if json_required_bool(parsed, "is_up")? { "up" } else { "down" },
                    "strike": format_raw_price_e9(json_required_u64(parsed, "strike")?),
                    "upper": serde_json::Value::Null,
                    "quantity": json_required_u64(parsed, "quantity")?,
                    "costRaw": cost.to_string(),
                    "costDisplay": asset.format_amount(cost),
                    "askPrice": json_required_string(parsed, "ask_price")?,
                }));
            } else if event_type.ends_with("::predict::RangeMinted") {
                let cost = json_required_u64(parsed, "cost")?;
                total_cost_raw = total_cost_raw
                    .checked_add(cost)
                    .ok_or_else(|| io::Error::other("total cost overflow"))?;

                minted_legs.push(serde_json::json!({
                    "kind": "range",
                    "direction": serde_json::Value::Null,
                    "strike": format_raw_price_e9(json_required_u64(parsed, "lower_strike")?),
                    "upper": format_raw_price_e9(json_required_u64(parsed, "higher_strike")?),
                    "quantity": json_required_u64(parsed, "quantity")?,
                    "costRaw": cost.to_string(),
                    "costDisplay": asset.format_amount(cost),
                    "askPrice": json_required_string(parsed, "ask_price")?,
                }));
            }
        }

        if minted_legs.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "no PositionMinted or RangeMinted events found",
            )
            .into());
        }

        Ok(serde_json::json!({
            "ok": true,
            "source": from_execution_json.to_string_lossy(),
            "digest": digest,
            "status": status,
            "mintedCount": minted_legs.len(),
            "totalCostRaw": total_cost_raw.to_string(),
            "totalCostDisplay": asset.format_amount(total_cost_raw),
            "mintedLegs": minted_legs,
        }))
    }

    fn json_required_string(
        value: &serde_json::Value,
        key: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        value.get(key).and_then(serde_json::Value::as_str).map(ToString::to_string).ok_or_else(
            || {
                io::Error::new(io::ErrorKind::InvalidData, format!("missing string field `{key}`"))
                    .into()
            },
        )
    }

    fn json_required_u64(
        value: &serde_json::Value,
        key: &str,
    ) -> Result<u64, Box<dyn std::error::Error>> {
        let item = value.get(key).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, format!("missing u64 field `{key}`"))
        })?;

        match item {
            serde_json::Value::String(s) => Ok(s.parse::<u64>()?),
            serde_json::Value::Number(n) => n.as_u64().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, format!("invalid u64 field `{key}`"))
                    .into()
            }),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid u64 field `{key}`"),
            )
            .into()),
        }
    }

    fn json_required_bool(
        value: &serde_json::Value,
        key: &str,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let item = value.get(key).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, format!("missing bool field `{key}`"))
        })?;

        match item {
            serde_json::Value::Bool(value) => Ok(*value),
            serde_json::Value::String(s) if s == "true" => Ok(true),
            serde_json::Value::String(s) if s == "false" => Ok(false),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid bool field `{key}`"),
            )
            .into()),
        }
    }

    fn format_raw_price_e9(raw: u64) -> String {
        let whole = raw / 1_000_000_000;
        let frac = raw % 1_000_000_000;

        if frac == 0 {
            return whole.to_string();
        }

        let mut frac_string = format!("{frac:09}");
        while frac_string.ends_with('0') {
            frac_string.pop();
        }

        format!("{whole}.{frac_string}")
    }
}
