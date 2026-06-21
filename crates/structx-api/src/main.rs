mod intent_audit;
mod intent_positions;
mod open_execution_audit;
mod position_ledger;
mod proposal_store;
mod storage;

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderValue, Method, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};

use deepbook_client::{
    DeepBookClient, DeepBookConfig, FreshnessConfig, MarketSnapshot, ObjectOwnerKind,
    SuiObjectInfo, SuiRpcClient, DEFAULT_SUI_TESTNET_RPC_URL, PREDICT_MANAGER_TYPE,
    PREDICT_SERVER_URL,
};
use intent_audit::{
    infer_execution_status, infer_manager_id_from_execution, make_audit_id,
    now_ms as intent_audit_now_ms, DiskIntentAuditStore, IntentExecutionAudit,
};
use intent_positions::list_intent_positions;
use open_execution_audit::{
    audit_open_execution, minted_leg_from_audit_json, OpenExecutionAuditInput,
    OpenExecutionAuditSource,
};
use position_ledger::{premium_basis_for_slice, LegKind, MintedLeg, PositionLedger, RedeemedLeg};
use proposal_store::DiskProposalStore;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    env,
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use structx_core::{
    build_mint_tx_kind, build_redeem_tx_kind, BinaryDirection, DisplayPrice, ManagerPositionRead,
    MintObjectRefs, QuoteCall, QuoteFunction, QuotePlan, QuoteTarget, Strike,
};
use structx_service as service;
use structx_service::{
    load_catalog_status, plan_from_intent, quote_intent_plan,
    refresh_catalog_from_existing_markets_json, DiskMarketStore, ExpiryPreference, MarketCategory,
    MarketKind, MarketSearchQuery, MarketStatus, MarketStore, QuoteIntentPlanRequest, RiskStyle,
    UserIntentRequest,
};
use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;

const PREDICT_PACKAGE_ID: &str =
    "0xf5ea2b3749c65d6e56507cc35388719aadb28f9cab873696a2f8687f5c785138";
const PREDICT_OBJECT_ID: &str =
    "0xc8736204d12f0a7277c86388a68bf8a194b0a14c5538ad13f22cbd8e2a38028a";
const CLOCK_OBJECT_ID: &str = "0x6";
const DUSDC_COIN_TYPE: &str =
    "0xe95040085976bfd54a1a07225cd46c8a2b4e8e2b6732f140a0fc49850ba73e1a::dusdc::DUSDC";

#[derive(Debug, Clone)]
struct AppState {
    compiled: Arc<Mutex<HashMap<String, serde_json::Value>>>,
    /// Persistent map from lower-cased wallet address -> PredictManager object id.
    /// Backed by a JSON file at `managers_path` so the same wallet sees its
    /// previously created manager across browsers, devices, and localStorage
    /// resets.
    managers: Arc<Mutex<HashMap<String, String>>>,
    managers_path: PathBuf,
    markets_refresh: Arc<Mutex<MarketsRefreshState>>,
}

#[derive(Debug, Default)]
struct MarketsRefreshState {
    in_flight: bool,
    last_started_at: Option<Instant>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct MarketsCacheRecord {
    #[serde(rename = "schemaVersion")]
    schema_version: u8,
    #[serde(rename = "refreshedAtUnix")]
    refreshed_at_unix: i64,
    envelope: serde_json::Value,
}

const MARKETS_REFRESH_THROTTLE: Duration = Duration::from_secs(20);
const MARKETS_CACHE_FRESH_FOR: Duration = Duration::from_secs(75);
const MARKETS_CACHE_WARM_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Debug, Deserialize)]
struct SaveManagerRequest {
    #[serde(rename = "managerId")]
    manager_id: String,
}

#[derive(Debug, Serialize)]
struct ManagerLookupResponse {
    ok: bool,
    address: String,
    #[serde(rename = "managerId", skip_serializing_if = "Option::is_none")]
    manager_id: Option<String>,
}

fn normalize_address(address: &str) -> String {
    let trimmed = address.trim();
    let lower = trimmed.to_lowercase();
    if lower.starts_with("0x") {
        lower
    } else {
        format!("0x{lower}")
    }
}

fn is_sui_hex_id(value: &str) -> bool {
    let value = value.trim();
    let Some(hex) = value.strip_prefix("0x").or_else(|| value.strip_prefix("0X")) else {
        return false;
    };
    !hex.is_empty() && hex.len() <= 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn cors_layer() -> CorsLayer {
    let configured = env::var("STRUCTX_ALLOWED_ORIGINS")
        .unwrap_or_else(|_| "http://localhost:3000,http://127.0.0.1:3000".to_string());
    let origins = configured
        .split(',')
        .filter_map(|origin| origin.trim().parse::<HeaderValue>().ok())
        .collect::<Vec<_>>();

    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::CONTENT_TYPE])
}

fn load_managers(path: &std::path::Path) -> HashMap<String, String> {
    match std::fs::read_to_string(path) {
        Ok(contents) => {
            serde_json::from_str::<HashMap<String, String>>(&contents).unwrap_or_else(|err| {
                eprintln!(
                    "warning: managers store {} is not valid JSON ({err}); starting empty",
                    path.display()
                );
                HashMap::new()
            })
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => HashMap::new(),
        Err(err) => {
            eprintln!(
                "warning: failed to read managers store {} ({err}); starting empty",
                path.display()
            );
            HashMap::new()
        }
    }
}

fn save_managers(path: &std::path::Path, map: &HashMap<String, String>) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    // Atomic write via a sibling tmp file + rename so a crash mid-write can
    // never leave a half-written managers.json behind.
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(map)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[derive(Debug, Deserialize)]
struct BuildOpenStrategyRequest {
    owner: String,
    #[serde(rename = "managerId")]
    manager_id: String,
    #[serde(rename = "compiledStrategyId")]
    compiled_strategy_id: String,
    #[serde(rename = "maxPremiumRaw")]
    max_premium_raw: String,
    #[serde(rename = "slippageBps")]
    slippage_bps: u16,
}

#[derive(Debug, Deserialize)]
struct AuditOpenStrategyRequest {
    owner: String,
    #[serde(rename = "managerId")]
    manager_id: String,
    #[serde(rename = "compiledStrategyId")]
    compiled_strategy_id: String,
    digest: String,
    #[serde(rename = "effects")]
    _effects: serde_json::Value,
    #[serde(rename = "events")]
    _events: Vec<serde_json::Value>,
    #[serde(rename = "objectChanges")]
    _object_changes: Vec<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    ok: bool,
    service: &'static str,
}

#[derive(Debug, Serialize)]
struct CliResponse {
    ok: bool,
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

#[derive(Debug, Deserialize)]
struct CompileStrategyRequest {
    owner: String,
    strategy: String,
    #[serde(rename = "budgetDUSDC")]
    budget_dusdc: String,
    style: String,
    #[serde(rename = "expiryPreference")]
    expiry_preference: String,
    #[serde(rename = "slippageBps")]
    slippage_bps: u16,
    #[serde(rename = "bucketStepUsd")]
    bucket_step_usd: Option<f64>,
    #[serde(rename = "customK1Price")]
    custom_k1_price: Option<f64>,
    #[serde(rename = "customK2Price")]
    custom_k2_price: Option<f64>,
    #[serde(rename = "customK3Price")]
    custom_k3_price: Option<f64>,
    #[serde(rename = "customK4Price")]
    custom_k4_price: Option<f64>,
    #[serde(rename = "portfolioExposureDUSDC")]
    portfolio_exposure_dusdc: Option<f64>,
    #[serde(rename = "overHedgeCapBps")]
    over_hedge_cap_bps: Option<u16>,
    #[serde(rename = "deadZoneBps")]
    dead_zone_bps: Option<u16>,
    #[serde(rename = "convexGammaBps")]
    convex_gamma_bps: Option<u16>,
    #[serde(rename = "moonshotRangeWeightBps")]
    moonshot_range_weight_bps: Option<u16>,
    #[serde(rename = "moonshotTailGammaBps")]
    moonshot_tail_gamma_bps: Option<u16>,
    #[serde(rename = "downsideRangeWeightBps")]
    downside_range_weight_bps: Option<u16>,
    #[serde(rename = "downsideTailGammaBps")]
    downside_tail_gamma_bps: Option<u16>,
    #[serde(rename = "upsideNearRangeWeightBps")]
    upside_near_range_weight_bps: Option<u16>,
    #[serde(rename = "upsideUpperRangeWeightBps")]
    upside_upper_range_weight_bps: Option<u16>,
    #[serde(rename = "upsideTailGammaBps")]
    upside_tail_gamma_bps: Option<u16>,
    #[serde(rename = "downsideNearRangeWeightBps")]
    downside_near_range_weight_bps: Option<u16>,
    #[serde(rename = "downsideLowerRangeWeightBps")]
    downside_lower_range_weight_bps: Option<u16>,
    #[serde(rename = "downsideStepTailGammaBps")]
    downside_step_tail_gamma_bps: Option<u16>,
    #[serde(rename = "condorCenterWeightBps")]
    condor_center_weight_bps: Option<u16>,
    #[serde(rename = "barrierSide")]
    barrier_side: Option<String>,
    #[serde(rename = "barrierNearRangeWeightBps")]
    barrier_near_range_weight_bps: Option<u16>,
    #[serde(rename = "barrierTailGammaBps")]
    barrier_tail_gamma_bps: Option<u16>,
}

#[derive(Debug, Deserialize)]
struct ParseIntentRequest {
    owner: String,
    message: String,
    #[serde(rename = "budgetDUSDC")]
    budget_dusdc: Option<String>,
    #[serde(rename = "riskPreference")]
    risk_preference: Option<String>,
    #[serde(rename = "timePreference")]
    time_preference: Option<String>,
}

#[derive(Debug, Deserialize)]
struct IntentPlanApiRequest {
    user_address: Option<String>,
    prompt: String,
    budget: Option<u64>,
    quote_asset: Option<String>,
    risk_style: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AuditIntentExecutionRequest {
    proposal_id: String,
    tx_digest: String,
    user_address: Option<String>,
    manager_id: Option<String>,
    #[serde(rename = "execution_result")]
    _execution_result: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct IntentExecutePlanRequest {
    proposal_id: String,
    user_address: Option<String>,
}

#[derive(Debug, Serialize)]
struct IntentExecutePlanResponse {
    proposal_id: String,
    user_address: Option<String>,
    compiled_strategy_id: Option<String>,
    raw_compiled_strategy: serde_json::Value,
    proposal: structx_service::ExecutionProposal,
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
struct AuditIntentExecutionResponse {
    ok: bool,
    audit: IntentExecutionAudit,
    position_sync_status: String,
    position_ids: Vec<String>,
    warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RecentIntentAuditsQuery {
    max: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct IntentPositionsQuery {
    user_address: Option<String>,
    max: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct CompileFromIntentRequest {
    owner: String,
    intent: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ParsedIntent {
    ok: bool,
    #[serde(rename = "intentId")]
    intent_id: String,
    owner: String,
    #[serde(rename = "rawMessage")]
    raw_message: String,
    asset: String,
    goal: String,
    #[serde(rename = "budgetDUSDC")]
    budget_dusdc: String,
    #[serde(rename = "riskPreference")]
    risk_preference: String,
    #[serde(rename = "timePreference")]
    time_preference: String,
    #[serde(rename = "recommendedStrategy")]
    recommended_strategy: String,
    #[serde(rename = "recommendedStyle")]
    recommended_style: String,
    confidence: f64,
    #[serde(rename = "reasoningSummary")]
    reasoning_summary: String,
    #[serde(rename = "missingFields")]
    missing_fields: Vec<String>,
    warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct FindMintableStrategyRequest {
    owner: String,
    #[serde(rename = "managerId")]
    manager_id: String,
    #[serde(rename = "budgetDUSDC")]
    budget_dusdc: String,
    style: String,
    #[serde(rename = "expiryPreference")]
    expiry_preference: String,
    #[serde(rename = "slippageBps")]
    slippage_bps: u16,
}

#[derive(Debug, Deserialize)]
struct DemoStatusRequest {
    manager_id: String,
    sender: String,
    from_execution_json: String,
}

#[derive(Debug, Deserialize)]
struct ManagerBalanceRequest {
    manager_id: String,
}

#[derive(Debug, Deserialize)]
struct ManagerPositionsRequest {
    manager_id: String,
    sender: String,
    from_execution_json: String,
}

#[derive(Debug, Deserialize)]
struct AuditExecutionRequest {
    from_execution_json: String,
}

#[derive(Debug, Deserialize)]
struct DevinspectMintBreakoutRequest {
    manager_id: String,
    sender: String,
    max_total_mint_cost_raw: u64,

    #[serde(default = "default_slippage_bps")]
    slippage_bps: u16,

    #[serde(default = "default_max_quote_market_attempts")]
    max_quote_market_attempts: usize,

    #[serde(default)]
    write_execute_script: bool,
}

#[derive(Debug, Deserialize)]
struct DevinspectRedeemBreakoutRequest {
    manager_id: String,
    sender: String,
    from_execution_json: String,

    #[serde(default)]
    auto_size_down: bool,

    #[serde(default)]
    write_execute_script: bool,

    #[serde(default)]
    allow_zero_payout_script: bool,
}

fn default_slippage_bps() -> u16 {
    100
}

fn default_max_quote_market_attempts() -> usize {
    5
}

fn parse_market_category(input: Option<String>) -> Option<MarketCategory> {
    match input?.trim().to_ascii_lowercase().as_str() {
        "crypto" => Some(MarketCategory::Crypto),
        "finance" => Some(MarketCategory::Finance),
        "sports" => Some(MarketCategory::Sports),
        "politics" => Some(MarketCategory::Politics),
        "macro" => Some(MarketCategory::Macro),
        "weather" => Some(MarketCategory::Weather),
        "other" => Some(MarketCategory::Other),
        "unknown" => Some(MarketCategory::Unknown),
        _ => None,
    }
}

fn parse_market_kind(input: Option<String>) -> Option<MarketKind> {
    match input?.trim().to_ascii_lowercase().as_str() {
        "scalar_price" | "price" => Some(MarketKind::ScalarPrice),
        "scalar_event" | "scalar" => Some(MarketKind::ScalarEvent),
        "binary_event" | "binary" => Some(MarketKind::BinaryEvent),
        "categorical_event" | "categorical" => Some(MarketKind::CategoricalEvent),
        "unknown" => Some(MarketKind::Unknown),
        _ => None,
    }
}

fn parse_expiry_preference(input: Option<String>) -> Option<ExpiryPreference> {
    match input?.trim().to_ascii_lowercase().as_str() {
        "nearest" | "nearest_active" => Some(ExpiryPreference::NearestActive),
        "soonest" => Some(ExpiryPreference::Soonest),
        "latest" => Some(ExpiryPreference::Latest),
        "any" => Some(ExpiryPreference::Any),
        _ => None,
    }
}

fn parse_risk_style(input: Option<String>) -> Option<RiskStyle> {
    match input?.trim().to_ascii_lowercase().as_str() {
        "conservative" => Some(RiskStyle::Conservative),
        "balanced" => Some(RiskStyle::Balanced),
        "aggressive" => Some(RiskStyle::Aggressive),
        "tail_heavy" | "tail-heavy" => Some(RiskStyle::TailHeavy),
        "higher_hit_rate" | "higher-hit-rate" => Some(RiskStyle::HigherHitRate),
        _ => None,
    }
}

#[tokio::main]
async fn main() {
    if let Err(err) = std::fs::create_dir_all(storage::markets_dir()) {
        eprintln!(
            "warning: could not create markets cache dir {} ({err})",
            storage::markets_dir().display()
        );
    }

    let managers_path = env::var("STRUCTX_MANAGERS_PATH").map(PathBuf::from).unwrap_or_else(|_| {
        env::var("STRUCTX_STATE_DIR")
            .map(PathBuf::from)
            .map(|root| root.join("managers.json"))
            .unwrap_or_else(|_| PathBuf::from("data/managers.json"))
    });
    let managers_initial = load_managers(&managers_path);
    println!(
        "Loaded {} stored PredictManager(s) from {}",
        managers_initial.len(),
        managers_path.display()
    );

    let state = Arc::new(AppState {
        compiled: Arc::new(Mutex::new(HashMap::new())),
        managers: Arc::new(Mutex::new(managers_initial)),
        managers_path,
        markets_refresh: Arc::new(Mutex::new(MarketsRefreshState::default())),
    });

    spawn_markets_cache_warmer(state.clone());

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/demo-status", post(demo_status))
        .route("/api/manager-balance", post(manager_balance))
        .route("/api/manager-balance-json", post(manager_balance_json))
        .route("/api/manager-positions", post(manager_positions))
        .route("/api/audit-execution", post(audit_execution))
        .route("/api/intent/plan", post(plan_intent))
        .route("/api/intent/quote", post(quote_intent))
        .route("/api/intent/execute-plan", post(execute_intent_plan))
        .route("/api/intent/audit-execution", post(audit_intent_execution))
        .route("/api/intent/audits/recent", get(list_recent_intent_audits))
        .route("/api/intent/audits/proposal/{proposal_id}", get(get_intent_audit_by_proposal))
        .route("/api/intent/audits/digest/{digest}", get(get_intent_audit_by_digest))
        .route("/api/intent/positions", get(list_intent_position_overlays))
        .route("/api/intent/parse", post(parse_intent))
        .route("/api/strategies/compile", post(compile_strategy))
        .route("/api/strategies/compile-from-intent", post(compile_from_intent))
        .route("/api/strategies/find-mintable", post(find_mintable_strategy))
        .route("/api/tx/build-open-strategy", post(build_open_strategy))
        .route("/api/tx/audit-open-strategy", post(audit_open_strategy))
        .route("/api/markets", get(list_markets))
        .route("/api/markets/catalog/status", get(get_market_catalog_status))
        .route("/api/markets/catalog/refresh", post(refresh_market_catalog))
        .route("/api/markets/search", get(search_market_catalog))
        .route("/api/markets/catalog/{market_id}", get(get_catalog_market))
        .route("/api/positions", get(list_positions))
        .route("/api/positions/sync-from-audits", post(sync_positions_from_audits))
        .route("/api/positions/sync-from-chain", post(sync_positions_from_chain))
        .route("/api/tx/audit-redeem-position", post(audit_redeem_position))
        .route(
            "/api/managers/{address}",
            get(get_manager_for_address).post(put_manager_for_address),
        )
        .route("/api/devinspect-mint-breakout", post(devinspect_mint_breakout))
        .route("/api/devinspect-redeem-breakout", post(devinspect_redeem_breakout))
        .layer(cors_layer())
        .with_state(state);

    let addr: SocketAddr = env::var("STRUCTX_API_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:8787".to_string())
        .parse()
        .expect("STRUCTX_API_ADDR must be host:port");

    println!("StructX API listening on http://{addr}");
    axum::serve(tokio::net::TcpListener::bind(addr).await.expect("bind API listener"), app)
        .await
        .expect("serve API");
}

async fn build_open_strategy(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BuildOpenStrategyRequest>,
) -> impl IntoResponse {
    if req.slippage_bps > 10_000 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "ok": false,
                "error": "slippageBps must be between 0 and 10000"
            })),
        );
    }
    let max_premium_raw = match req.max_premium_raw.parse::<u128>() {
        Ok(value) if value > 0 => value,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "ok": false,
                    "error": "maxPremiumRaw must be a positive integer"
                })),
            );
        }
    };

    let cached_compiled = {
        let cache = state.compiled.lock().await;
        cache.get(&req.compiled_strategy_id).cloned()
    };

    let Some(cached_compiled) = cached_compiled else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "ok": false,
                "error": "compiledStrategyId not found. Compile the strategy again before building transaction."
            })),
        );
    };

    let max_premium_after_slippage = max_premium_raw
        .saturating_mul(10_000u128 + u128::from(req.slippage_bps))
        .saturating_add(9_999)
        / 10_000;
    let mut excluded_oracle_ids = Vec::new();
    let mut last_mintability_error: Option<serde_json::Value> = None;
    let mut attempted_oracle_ids = Vec::new();
    let mut best_effort_warnings: Vec<String> = Vec::new();
    let compiled = loop {
        let refreshed_compiled = match refresh_compiled_strategy(
            state.as_ref(),
            &cached_compiled,
            &req.owner,
            req.slippage_bps,
            &excluded_oracle_ids,
        )
        .await
        {
            Ok(compiled) => compiled,
            Err((status, value)) => {
                if let Some(last_error) = last_mintability_error {
                    return (StatusCode::BAD_REQUEST, Json(last_error));
                }
                return (status, Json(value));
            }
        };

        let premium_required = refreshed_compiled
            .get("premiumRequiredRaw")
            .and_then(serde_json::Value::as_str)
            .and_then(|v| v.parse::<u128>().ok())
            .unwrap_or(u128::MAX);

        if premium_required > max_premium_after_slippage {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "ok": false,
                    "code": "PREMIUM_EXCEEDS_SLIPPAGE_CAP",
                    "title": "Price moved beyond your limit",
                    "message": format!(
                        "The live premium is {premium_required} raw dUSDC, above your limit of {max_premium_after_slippage}."
                    ),
                    "action": "Preview the strategy again to review the latest price before opening it."
                })),
            );
        }

        let compiled_strategy_id = refreshed_compiled
            .get("compiledStrategyId")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();

        let Some(expiry_ms) = compiled_expiry_ms(compiled_strategy_id) else {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "ok": false,
                    "error": "compiledStrategyId missing expiry_ms"
                })),
            );
        };

        match find_best_effort_mintable_compiled(
            &req.owner,
            &req.manager_id,
            &refreshed_compiled,
            &expiry_ms,
            true,
        )
        .await
        {
            Ok((compiled, mut warnings)) => {
                best_effort_warnings.append(&mut warnings);
                break compiled;
            }
            Err((status, value)) => {
                let is_not_mintable = value
                    .get("error")
                    .and_then(serde_json::Value::as_str)
                    .map(|message| message.contains("assert_mintable_ask"))
                    .unwrap_or(false);

                let oracle_id = refreshed_compiled
                    .get("oracleId")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                if !oracle_id.is_empty() && !attempted_oracle_ids.contains(&oracle_id) {
                    attempted_oracle_ids.push(oracle_id.clone());
                }

                if is_not_mintable
                    && !oracle_id.is_empty()
                    && !excluded_oracle_ids.contains(&oracle_id)
                    && excluded_oracle_ids.len() < 4
                {
                    excluded_oracle_ids.push(oracle_id);
                    last_mintability_error =
                        Some(enrich_mintability_error(value, &attempted_oracle_ids));
                    continue;
                }

                return (status, Json(enrich_mintability_error(value, &attempted_oracle_ids)));
            }
        }
    };

    if let Some(id) = compiled.get("compiledStrategyId").and_then(serde_json::Value::as_str) {
        state.compiled.lock().await.insert(id.to_string(), compiled.clone());
    }

    let oracle_id =
        compiled.get("oracleId").and_then(serde_json::Value::as_str).unwrap_or_default();

    let raw_legs =
        compiled.get("legs").and_then(serde_json::Value::as_array).cloned().unwrap_or_default();

    let legs = legs_with_max_costs(raw_legs, req.slippage_bps);

    let mut warnings =
        compiled.get("warnings").and_then(serde_json::Value::as_array).cloned().unwrap_or_default();
    warnings.push(serde_json::Value::String(
        "StructX checks your price limit before building and checks the complete transaction again before your wallet opens it.".to_string(),
    ));
    for warning in best_effort_warnings {
        warnings.push(serde_json::Value::String(warning));
    }

    let compiled_strategy_id =
        compiled.get("compiledStrategyId").and_then(serde_json::Value::as_str).unwrap_or_default();

    let Some(expiry_ms) = compiled_expiry_ms(compiled_strategy_id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "ok": false,
                "error": "compiledStrategyId missing expiry_ms"
            })),
        );
    };

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "ok": true,
            "buildKind": "FRONTEND_TRANSACTION_BUILDER",
            "network": "sui:testnet",
            "compiledStrategyId": compiled.get("compiledStrategyId").cloned().unwrap_or(serde_json::Value::Null),
            "expiryMs": expiry_ms,
            "owner": req.owner,
            "managerId": req.manager_id,
            "predictPackageId": PREDICT_PACKAGE_ID,
            "predictObjectId": PREDICT_OBJECT_ID,
            "clockObjectId": CLOCK_OBJECT_ID,
            "dusdcCoinType": DUSDC_COIN_TYPE,
            "oracleId": oracle_id,
            "slippageBps": req.slippage_bps,
            "summary": {
                "strategy": compiled.get("strategy").cloned().unwrap_or(serde_json::Value::Null),
                "premiumRequiredRaw": compiled.get("premiumRequiredRaw").cloned().unwrap_or(serde_json::Value::Null),
                "premiumRequiredDisplay": compiled.get("premiumRequiredDisplay").cloned().unwrap_or(serde_json::Value::Null),
                "legs": legs
            },
            "warnings": warnings
        })),
    )
}

async fn audit_open_strategy(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AuditOpenStrategyRequest>,
) -> impl IntoResponse {
    let execution_result = match fetch_verified_execution(&req.digest, Some(&req.owner)).await {
        Ok(result) => result,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "ok": false,
                    "error": error
                })),
            );
        }
    };

    let raw_compiled_strategy = {
        let cache = state.compiled.lock().await;
        cache.get(&req.compiled_strategy_id).cloned()
    }
    .unwrap_or_else(|| {
        serde_json::json!({
            "compiledStrategyId": req.compiled_strategy_id,
        })
    });

    let input = OpenExecutionAuditInput {
        source: OpenExecutionAuditSource::AdvancedMode,
        proposal_id: None,
        user_address: Some(req.owner.clone()),
        manager_id: Some(req.manager_id.clone()),
        tx_digest: req.digest.clone(),
        execution_result,
        raw_compiled_strategy,
        intent_proposal: None,
    };

    match audit_open_execution(input).await {
        Ok(outcome) => {
            let status = if outcome.ok { StatusCode::OK } else { StatusCode::BAD_REQUEST };

            let body = serde_json::json!({
                "ok": outcome.ok,
                "digest": outcome.tx_digest,
                "explorerUrl": outcome.explorer_url,
                "executionStatus": outcome.execution_status,
                "compiledStrategyId": outcome.compiled_strategy_id,
                "artifactPath": outcome.artifact_path,
                "totalCostRaw": outcome.total_cost_raw,
                "totalCostDisplay": outcome.total_cost_display,
                "managerId": outcome.manager_id,
                "managerBalanceRaw": outcome.manager_balance_raw,
                "managerBalanceDisplay": outcome.manager_balance_display,
                "mintedLegs": outcome.minted_legs,
                "positionVerification": outcome.position_verification,
                "positionIds": outcome.position_ids,
                "ledgerSyncStatus": outcome.ledger_sync_status,
                "warnings": outcome.warnings,
                "debug": outcome.raw_audit_result,
            });

            (status, Json(body))
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(api_error(
                "AUDIT_FAILED",
                "Audit failed",
                &format!("Failed to audit open execution: {err}"),
                "Try opening the strategy again.",
                None,
                None,
            )),
        ),
    }
}

async fn fetch_verified_execution(
    digest: &str,
    expected_sender: Option<&str>,
) -> Result<serde_json::Value, String> {
    let rpc = SuiRpcClient::new(DEFAULT_SUI_TESTNET_RPC_URL, Duration::from_secs(20))
        .map_err(|err| format!("Unable to initialize Sui RPC verification: {err}"))?;
    let result = rpc
        .get_transaction_block(digest)
        .await
        .map_err(|err| format!("Unable to verify transaction {digest} on Sui Testnet: {err}"))?;

    if let Some(expected) = expected_sender {
        let actual = result
            .get("transaction")
            .and_then(|transaction| transaction.get("data"))
            .and_then(|data| data.get("sender"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "The verified transaction is missing its sender address.".to_string())?;
        if normalize_address(actual) != normalize_address(expected) {
            return Err(format!(
                "The transaction sender {actual} does not match the connected wallet {expected}."
            ));
        }
    }

    Ok(result)
}

fn json_value_as_u128_string(value: &serde_json::Value) -> Option<String> {
    if let Some(s) = value.as_str() {
        if s.parse::<u128>().is_ok() {
            return Some(s.to_string());
        }
    }
    if let Some(u) = value.as_u64() {
        return Some(u.to_string());
    }
    None
}

fn format_dusdc_raw_u128(raw: u128) -> String {
    let whole = raw / 1_000_000;
    let frac = raw % 1_000_000;

    if frac == 0 {
        format!("{whole}.00 dUSDC")
    } else {
        let mut frac_string = format!("{frac:06}");
        while frac_string.ends_with('0') {
            frac_string.pop();
        }
        format!("{whole}.{frac_string} dUSDC")
    }
}

fn api_error(
    code: &str,
    title: &str,
    message: &str,
    action: &str,
    stdout: Option<&str>,
    stderr: Option<&str>,
) -> serde_json::Value {
    let mut debug = serde_json::Map::new();
    if let Some(stdout) = stdout {
        debug.insert("stdout".to_string(), serde_json::Value::String(stdout.to_string()));
    }
    if let Some(stderr) = stderr {
        debug.insert("stderr".to_string(), serde_json::Value::String(stderr.to_string()));
    }
    serde_json::json!({
        "ok": false,
        "code": code,
        "title": title,
        "message": message,
        "action": action,
        "debug": serde_json::Value::Object(debug)
    })
}

async fn parse_intent(Json(req): Json<ParseIntentRequest>) -> impl IntoResponse {
    let parsed = match parse_intent_with_openai_or_fallback(&req).await {
        Ok(parsed) => parsed,
        Err(err) => {
            let mut fallback = deterministic_parse_intent(&req);
            fallback.warnings.push(format!("AI parser failed; deterministic fallback used: {err}"));
            fallback
        }
    };

    if !parsed.missing_fields.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "ok": false,
                "missingFields": parsed.missing_fields,
                "clarifyingQuestion": build_clarifying_question(&parsed),
                "fallbackIntent": parsed
            })),
        );
    }

    (
        StatusCode::OK,
        Json(serde_json::to_value(parsed).unwrap_or_else(
            |_| serde_json::json!({"ok": false, "error": "failed to serialize parsed intent"}),
        )),
    )
}

async fn plan_intent(Json(req): Json<IntentPlanApiRequest>) -> impl IntoResponse {
    if req.prompt.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "ok": false,
                "message": "prompt cannot be empty",
            })),
        );
    }

    let store = DiskMarketStore::default_state_dir();
    if let Err(err) = ensure_market_catalog_ready(&store).await {
        return (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({
                "ok": false,
                "message": format!("failed to prepare market catalog for intent planning: {err}"),
            })),
        );
    }

    let service_request = UserIntentRequest {
        user_address: req.user_address,
        prompt: req.prompt,
        budget: req.budget,
        quote_asset: req.quote_asset,
        risk_style: parse_risk_style(req.risk_style),
    };

    match plan_from_intent(&store, service_request).await {
        Ok(response) => (
            StatusCode::OK,
            Json(serde_json::to_value(response).unwrap_or_else(
                |_| serde_json::json!({"ok": false, "error": "failed to serialize intent plan"}),
            )),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "ok": false,
                "message": err.to_string(),
            })),
        ),
    }
}

async fn quote_intent(
    State(state): State<Arc<AppState>>,
    Json(req): Json<QuoteIntentPlanRequest>,
) -> impl IntoResponse {
    let store = DiskMarketStore::default_state_dir();
    if let Err(err) = ensure_market_catalog_ready(&store).await {
        return (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({
                "ok": false,
                "message": format!("failed to prepare market catalog for quoting: {err}"),
            })),
        );
    }

    match quote_intent_plan(&store, req).await {
        Ok(response) => {
            if let Some(compiled_strategy_id) = response
                .raw_compiled_strategy
                .get("compiledStrategyId")
                .and_then(serde_json::Value::as_str)
            {
                state.compiled.lock().await.insert(
                    compiled_strategy_id.to_string(),
                    response.raw_compiled_strategy.clone(),
                );
            }

            let proposal_store = DiskProposalStore::default_state_dir();
            match proposal_store.save(response).await {
                Ok(stored) => (
                    StatusCode::OK,
                    Json(serde_json::to_value(stored.proposal).unwrap_or_else(
                        |_| serde_json::json!({"ok": false, "error": "failed to serialize execution proposal"}),
                    )),
                ),
                Err(err) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "ok": false,
                        "message": format!("failed to persist quoted proposal: {err}"),
                    })),
                ),
            }
        }
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "ok": false,
                "message": err.to_string(),
            })),
        ),
    }
}

async fn execute_intent_plan(
    State(state): State<Arc<AppState>>,
    Json(req): Json<IntentExecutePlanRequest>,
) -> impl IntoResponse {
    if req.proposal_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"ok": false, "message": "proposal_id is required"})),
        );
    }

    let proposal_store = DiskProposalStore::default_state_dir();
    let stored =
        match proposal_store.require_fresh(&req.proposal_id, proposal_store::now_ms()).await {
            Ok(stored) => stored,
            Err(err) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "ok": false,
                        "message": format!("failed to load fresh proposal: {err}"),
                    })),
                );
            }
        };

    let compiled_strategy_id = stored
        .proposal
        .raw_compiled_strategy
        .get("compiledStrategyId")
        .and_then(serde_json::Value::as_str)
        .map(|value| value.to_string());

    if let Some(ref id) = compiled_strategy_id {
        state
            .compiled
            .lock()
            .await
            .insert(id.clone(), stored.proposal.raw_compiled_strategy.clone());
    }

    let mut warnings = stored.proposal.warnings.clone();
    warnings.push(
        "Frontend must build and sign the Sui transaction with the connected wallet. Backend validates and caches the compiled proposal but does not sign."
            .to_string(),
    );

    let response = IntentExecutePlanResponse {
        proposal_id: stored.proposal_id,
        user_address: req.user_address.or(stored.proposal.user_address.clone()),
        compiled_strategy_id,
        raw_compiled_strategy: stored.proposal.raw_compiled_strategy.clone(),
        proposal: stored.proposal,
        warnings,
    };

    (
        StatusCode::OK,
        Json(
            serde_json::to_value(response)
                .unwrap_or_else(|_| serde_json::json!({"ok": false, "error": "failed to serialize intent execute-plan response"})),
        ),
    )
}

async fn audit_intent_execution(Json(req): Json<AuditIntentExecutionRequest>) -> impl IntoResponse {
    if req.proposal_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"ok": false, "message": "proposal_id is required"})),
        );
    }
    if req.tx_digest.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"ok": false, "message": "tx_digest is required"})),
        );
    }

    let proposal_store = DiskProposalStore::default_state_dir();
    let stored = match proposal_store.load(&req.proposal_id).await {
        Ok(Some(stored)) => stored,
        Ok(None) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "ok": false,
                    "message": format!("proposal not found: {}", req.proposal_id),
                })),
            );
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "ok": false,
                    "message": format!("failed to load proposal: {err}"),
                })),
            );
        }
    };

    let expected_sender = req.user_address.as_deref().or(stored.proposal.user_address.as_deref());
    let raw_execution_result = match fetch_verified_execution(&req.tx_digest, expected_sender).await
    {
        Ok(result) => result,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "ok": false,
                    "message": error
                })),
            );
        }
    };

    let now = intent_audit_now_ms();
    let mut warnings = stored.proposal.warnings.clone();
    let inferred_status = infer_execution_status(&raw_execution_result);
    let inferred_manager_id =
        req.manager_id.or_else(|| infer_manager_id_from_execution(&raw_execution_result));

    if inferred_manager_id.is_none() {
        warnings.push(
            "manager_id was not provided; canonical ledger merge may be skipped until chain sync or richer execution metadata is available."
                .to_string(),
        );
    }

    let audit = IntentExecutionAudit {
        schema_version: 1,
        audit_id: make_audit_id(&req.proposal_id, &req.tx_digest),
        proposal_id: req.proposal_id.clone(),
        user_address: req.user_address.or(stored.proposal.user_address.clone()),
        manager_id: inferred_manager_id.clone(),
        tx_digest: req.tx_digest.clone(),
        status: inferred_status,
        market_id: stored.proposal.selected_market.market_id.clone(),
        oracle_id: stored.proposal.selected_market.oracle_id.clone(),
        underlying: stored.proposal.selected_market.underlying.clone(),
        strategy_template: format!("{:?}", stored.proposal.strategy_template),
        backend_strategy_id: stored.proposal.backend_strategy_id.clone(),
        total_premium: stored.proposal.total_premium,
        max_loss: stored.proposal.max_loss,
        max_payout: stored.proposal.max_payout,
        created_at_ms: now,
        updated_at_ms: now,
        warnings: warnings.clone(),
        raw_execution_result: raw_execution_result.clone(),
        proposal: stored.proposal.clone(),
    };

    let audit_store = DiskIntentAuditStore::default_state_dir();
    if let Err(err) = audit_store.save(&audit).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "ok": false,
                "message": format!("failed to persist intent audit: {err}"),
            })),
        );
    }

    let mut position_ids = Vec::new();
    let position_sync_status = match audit_open_execution(OpenExecutionAuditInput {
        source: OpenExecutionAuditSource::NormalModeIntent,
        proposal_id: Some(audit.proposal_id.clone()),
        user_address: audit.user_address.clone(),
        manager_id: audit.manager_id.clone(),
        tx_digest: audit.tx_digest.clone(),
        execution_result: audit.raw_execution_result.clone(),
        raw_compiled_strategy: audit.proposal.raw_compiled_strategy.clone(),
        intent_proposal: Some(audit.proposal.clone()),
    })
    .await
    {
        Ok(outcome) => {
            warnings.extend(outcome.warnings.clone());
            position_ids = outcome.position_ids;
            outcome.ledger_sync_status
        }
        Err(err) => {
            warnings.push(format!("intent audit saved, but canonical open audit failed: {err}"));
            "intent_audit_saved_open_audit_failed".to_string()
        }
    };

    let response = AuditIntentExecutionResponse {
        ok: true,
        audit,
        position_sync_status,
        position_ids,
        warnings,
    };

    (
        StatusCode::OK,
        Json(serde_json::to_value(response).unwrap_or_else(
            |_| serde_json::json!({"ok": false, "error": "failed to serialize intent audit response"}),
        )),
    )
}

async fn get_intent_audit_by_proposal(Path(proposal_id): Path<String>) -> impl IntoResponse {
    let store = DiskIntentAuditStore::default_state_dir();
    match store.load_by_proposal(&proposal_id).await {
        Ok(Some(audit)) => (StatusCode::OK, Json(serde_json::to_value(audit).unwrap_or_default())),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(
                serde_json::json!({"ok": false, "message": format!("intent audit not found for proposal: {proposal_id}")}),
            ),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                serde_json::json!({"ok": false, "message": format!("failed to load intent audit: {err}")}),
            ),
        ),
    }
}

async fn get_intent_audit_by_digest(Path(digest): Path<String>) -> impl IntoResponse {
    let store = DiskIntentAuditStore::default_state_dir();
    match store.load_by_digest(&digest).await {
        Ok(Some(audit)) => (StatusCode::OK, Json(serde_json::to_value(audit).unwrap_or_default())),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(
                serde_json::json!({"ok": false, "message": format!("intent audit not found for digest: {digest}")}),
            ),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                serde_json::json!({"ok": false, "message": format!("failed to load intent audit: {err}")}),
            ),
        ),
    }
}

async fn list_recent_intent_audits(
    Query(query): Query<RecentIntentAuditsQuery>,
) -> impl IntoResponse {
    let store = DiskIntentAuditStore::default_state_dir();
    match store.list_recent(query.max.unwrap_or(25).min(100)).await {
        Ok(audits) => (StatusCode::OK, Json(serde_json::to_value(audits).unwrap_or_default())),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                serde_json::json!({"ok": false, "message": format!("failed to list intent audits: {err}")}),
            ),
        ),
    }
}

async fn list_intent_position_overlays(
    Query(query): Query<IntentPositionsQuery>,
) -> impl IntoResponse {
    match list_intent_positions(query.user_address, query.max.unwrap_or(50).min(200)).await {
        Ok(positions) => {
            (StatusCode::OK, Json(serde_json::to_value(positions).unwrap_or_default()))
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                serde_json::json!({"ok": false, "message": format!("failed to list intent positions: {err}")}),
            ),
        ),
    }
}

async fn ensure_market_catalog_ready(store: &DiskMarketStore) -> Result<(), String> {
    let needs_refresh = match store.load_latest_catalog().await {
        Ok(Some(catalog)) => catalog.markets.is_empty(),
        Ok(None) => true,
        Err(err) => return Err(format!("failed to inspect existing market catalog: {err}")),
    };

    if !needs_refresh {
        return Ok(());
    }

    let raw_markets_json = load_existing_markets_json().await?;
    refresh_catalog_from_existing_markets_json(store, raw_markets_json)
        .await
        .map(|_| ())
        .map_err(|err| format!("failed to refresh market catalog from live markets: {err}"))
}

async fn compile_strategy(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CompileStrategyRequest>,
) -> impl IntoResponse {
    match compile_strategy_service_value(&req).await {
        Ok(final_value) => {
            if let Some(id) =
                final_value.get("compiledStrategyId").and_then(serde_json::Value::as_str)
            {
                state.compiled.lock().await.insert(id.to_string(), final_value.clone());
            }

            (StatusCode::OK, Json(final_value))
        }
        Err((status, value)) => (status, Json(value)),
    }
}

async fn compile_from_intent(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CompileFromIntentRequest>,
) -> impl IntoResponse {
    let Some(strategy) = req.intent.get("recommendedStrategy").and_then(serde_json::Value::as_str)
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(api_error(
                "INTENT_INVALID",
                "Intent missing strategy",
                "The parsed intent did not include a recommended strategy.",
                "Generate the strategy again.",
                None,
                None,
            )),
        );
    };

    let Some(style) = req.intent.get("recommendedStyle").and_then(serde_json::Value::as_str) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(api_error(
                "INTENT_INVALID",
                "Intent missing style",
                "The parsed intent did not include a recommended style.",
                "Generate the strategy again.",
                None,
                None,
            )),
        );
    };

    let Some(budget_dusdc) = req.intent.get("budgetDUSDC").and_then(serde_json::Value::as_str)
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(api_error(
                "INTENT_INVALID",
                "Intent missing budget",
                "The parsed intent did not include a dUSDC budget.",
                "Enter a budget and try again.",
                None,
                None,
            )),
        );
    };

    let mut compiled = match compile_strategy_service_value(&CompileStrategyRequest {
        owner: req.owner.clone(),
        strategy: strategy.to_string(),
        budget_dusdc: budget_dusdc.to_string(),
        style: style.to_string(),
        expiry_preference: "nearest_active".to_string(),
        slippage_bps: 100,
        bucket_step_usd: None,
        custom_k1_price: None,
        custom_k2_price: None,
        custom_k3_price: None,
        custom_k4_price: None,
        portfolio_exposure_dusdc: None,
        over_hedge_cap_bps: None,
        dead_zone_bps: None,
        convex_gamma_bps: None,
        moonshot_range_weight_bps: None,
        moonshot_tail_gamma_bps: None,
        downside_range_weight_bps: None,
        downside_tail_gamma_bps: None,
        upside_near_range_weight_bps: None,
        upside_upper_range_weight_bps: None,
        upside_tail_gamma_bps: None,
        downside_near_range_weight_bps: None,
        downside_lower_range_weight_bps: None,
        downside_step_tail_gamma_bps: None,
        condor_center_weight_bps: None,
        barrier_side: None,
        barrier_near_range_weight_bps: None,
        barrier_tail_gamma_bps: None,
    })
    .await
    {
        Ok(compiled) => compiled,
        Err((status, value)) => return (status, Json(value)),
    };

    if let Some(obj) = compiled.as_object_mut() {
        obj.insert(
            "recommendation".to_string(),
            serde_json::json!({
                "source": "AI_INTENT_PLUS_DETERMINISTIC_COMPILER",
                "intent": req.intent,
                "reasoningSummary": req
                    .intent
                    .get("reasoningSummary")
                    .cloned()
                    .unwrap_or_else(|| serde_json::Value::String("Strategy selected from parsed user intent.".to_string())),
                "confidence": req
                    .intent
                    .get("confidence")
                    .cloned()
                    .unwrap_or_else(|| serde_json::Value::from(0.65))
            }),
        );
    }

    if let Some(id) = compiled.get("compiledStrategyId").and_then(serde_json::Value::as_str) {
        state.compiled.lock().await.insert(id.to_string(), compiled.clone());
    }

    (StatusCode::OK, Json(compiled))
}

#[allow(clippy::too_many_arguments)]
async fn compile_strategy_json_value(
    owner: &str,
    strategy: &str,
    budget_dusdc: &str,
    style: &str,
    expiry_preference: &str,
    slippage_bps: u16,
    bucket_step_usd: Option<f64>,
    custom_k1_price: Option<f64>,
    custom_k2_price: Option<f64>,
    custom_k3_price: Option<f64>,
    custom_k4_price: Option<f64>,
    portfolio_exposure_dusdc: Option<f64>,
    over_hedge_cap_bps: Option<u16>,
    dead_zone_bps: Option<u16>,
    convex_gamma_bps: Option<u16>,
    moonshot_range_weight_bps: Option<u16>,
    moonshot_tail_gamma_bps: Option<u16>,
    downside_range_weight_bps: Option<u16>,
    downside_tail_gamma_bps: Option<u16>,
    upside_near_range_weight_bps: Option<u16>,
    upside_upper_range_weight_bps: Option<u16>,
    upside_tail_gamma_bps: Option<u16>,
    downside_near_range_weight_bps: Option<u16>,
    downside_lower_range_weight_bps: Option<u16>,
    downside_step_tail_gamma_bps: Option<u16>,
    condor_center_weight_bps: Option<u16>,
    barrier_side: Option<String>,
    barrier_near_range_weight_bps: Option<u16>,
    barrier_tail_gamma_bps: Option<u16>,
) -> Result<serde_json::Value, (StatusCode, serde_json::Value)> {
    if slippage_bps > 10_000 {
        return Err((
            StatusCode::BAD_REQUEST,
            serde_json::json!({
                "ok": false,
                "error": "slippageBps must be between 0 and 10000"
            }),
        ));
    }

    let requested = normalize_strategy_id(strategy);
    let needs_template = false;
    let args = service::CompileStrategyJsonArgs {
        server_url: env::var("STRUCTX_PREDICT_SERVER_URL")
            .unwrap_or_else(|_| PREDICT_SERVER_URL.to_string()),
        predict_id: env::var("STRUCTX_PREDICT_ID")
            .unwrap_or_else(|_| PREDICT_OBJECT_ID.to_string()),
        rpc_url: env::var("STRUCTX_RPC_URL")
            .unwrap_or_else(|_| DEFAULT_SUI_TESTNET_RPC_URL.to_string()),
        owner: owner.to_string(),
        strategy: requested.clone(),
        budget_dusdc: budget_dusdc.to_string(),
        style: style.to_string(),
        expiry_preference: expiry_preference.to_string(),
        slippage_bps,
        bucket_step: DisplayPrice(bucket_step_usd.unwrap_or(250.0)),
        custom_k1_price: custom_k1_price.map(DisplayPrice),
        custom_k2_price: custom_k2_price.map(DisplayPrice),
        custom_k3_price: custom_k3_price.map(DisplayPrice),
        custom_k4_price: custom_k4_price.map(DisplayPrice),
        levels_each_side: 4,
        max_quote_market_attempts: 5,
        portfolio_exposure_dusdc: portfolio_exposure_dusdc.unwrap_or(5_000.0),
        over_hedge_cap_bps: over_hedge_cap_bps.unwrap_or(12_000),
        convex_gamma_bps: convex_gamma_bps.unwrap_or(15_000),
        dead_zone_bps: dead_zone_bps.unwrap_or(200),
        moonshot_range_weight_bps: moonshot_range_weight_bps.unwrap_or(6_000),
        moonshot_tail_gamma_bps: moonshot_tail_gamma_bps.unwrap_or(15_000),
        downside_range_weight_bps: downside_range_weight_bps.unwrap_or(6_000),
        downside_tail_gamma_bps: downside_tail_gamma_bps.unwrap_or(15_000),
        upside_near_range_weight_bps: upside_near_range_weight_bps.unwrap_or(4_000),
        upside_upper_range_weight_bps: upside_upper_range_weight_bps.unwrap_or(3_500),
        upside_tail_gamma_bps: upside_tail_gamma_bps.unwrap_or(15_000),
        downside_near_range_weight_bps: downside_near_range_weight_bps.unwrap_or(4_000),
        downside_lower_range_weight_bps: downside_lower_range_weight_bps.unwrap_or(3_500),
        downside_step_tail_gamma_bps: downside_step_tail_gamma_bps.unwrap_or(15_000),
        condor_center_weight_bps: condor_center_weight_bps.unwrap_or(6_000),
        barrier_side: barrier_side.unwrap_or_else(|| "up".to_string()),
        barrier_near_range_weight_bps: barrier_near_range_weight_bps.unwrap_or(7_000),
        barrier_tail_gamma_bps: barrier_tail_gamma_bps.unwrap_or(15_000),
        exclude_oracle_ids: Vec::new(),
    };

    let value = service::compile_strategy_json_value(args).await.map_err(|err| {
        (
            StatusCode::BAD_REQUEST,
            serde_json::json!({
                "ok": false,
                "error": err.to_string(),
            }),
        )
    })?;

    if needs_template {
        apply_strategy_template(&value, &requested)
    } else {
        Ok(value)
    }
}

async fn compile_strategy_service_value(
    req: &CompileStrategyRequest,
) -> Result<serde_json::Value, (StatusCode, serde_json::Value)> {
    compile_strategy_json_value(
        &req.owner,
        &req.strategy,
        &req.budget_dusdc,
        &req.style,
        &req.expiry_preference,
        req.slippage_bps,
        req.bucket_step_usd,
        req.custom_k1_price,
        req.custom_k2_price,
        req.custom_k3_price,
        req.custom_k4_price,
        req.portfolio_exposure_dusdc,
        req.over_hedge_cap_bps,
        req.dead_zone_bps,
        req.convex_gamma_bps,
        req.moonshot_range_weight_bps,
        req.moonshot_tail_gamma_bps,
        req.downside_range_weight_bps,
        req.downside_tail_gamma_bps,
        req.upside_near_range_weight_bps,
        req.upside_upper_range_weight_bps,
        req.upside_tail_gamma_bps,
        req.downside_near_range_weight_bps,
        req.downside_lower_range_weight_bps,
        req.downside_step_tail_gamma_bps,
        req.condor_center_weight_bps,
        req.barrier_side.clone(),
        req.barrier_near_range_weight_bps,
        req.barrier_tail_gamma_bps,
    )
    .await
}

async fn parse_intent_with_openai_or_fallback(
    req: &ParseIntentRequest,
) -> Result<ParsedIntent, Box<dyn std::error::Error + Send + Sync>> {
    let api_key = match env::var("OPENAI_API_KEY") {
        Ok(value) if !value.trim().is_empty() => value,
        _ => return Ok(deterministic_parse_intent(req)),
    };

    let model = env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string());
    let schema = serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "required": [
            "asset",
            "goal",
            "budgetDUSDC",
            "riskPreference",
            "timePreference",
            "recommendedStrategy",
            "recommendedStyle",
            "confidence",
            "reasoningSummary",
            "missingFields",
            "warnings"
        ],
        "properties": {
            "asset": { "type": "string", "enum": ["BTC"] },
            "goal": {
                "type": "string",
                "enum": [
                    "downside_protection",
                    "upside_speculation",
                    "two_sided_breakout",
                    "range_income",
                    "unknown"
                ]
            },
            "budgetDUSDC": { "type": "string" },
            "riskPreference": {
                "type": "string",
                "enum": ["conservative", "balanced", "aggressive"]
            },
            "timePreference": {
                "type": "string",
                "enum": ["nearest_active", "today", "this_week"]
            },
            "recommendedStrategy": {
                "type": "string",
                "enum": [
                    "BREAKOUT_PROTECTION",
                    "PORTFOLIO_CRASH_SHIELD",
                    "CONVEX_TAIL_LADDER",
                    "EXPIRY_MOVE_NOTE",
                    "MOONSHOT_UPSIDE",
                    "UPSIDE_STEP_LADDER",
                    "DOWNSIDE_CONVEXITY",
                    "DOWNSIDE_STEP_LADDER",
                    "CENTER_BAND_CONDOR",
                    "NEAR_BARRIER_PROXY",
                    "SMART_BUDGET_SELECTOR"
                ]
            },
            "recommendedStyle": {
                "type": "string",
                "enum": ["tail-heavy", "balanced", "higher-hit-rate"]
            },
            "confidence": {
                "type": "number",
                "minimum": 0,
                "maximum": 1
            },
            "reasoningSummary": { "type": "string" },
            "missingFields": { "type": "array", "items": { "type": "string" } },
            "warnings": { "type": "array", "items": { "type": "string" } }
        }
    });

    let input = format!(
        r#"
You are the StructX intent parser.

StructX is a non-custodial structured payoff builder on DeepBook Predict testnet.
You do not give financial advice.
You only convert user intent into strict JSON for deterministic compiler logic.
Supported asset: BTC only.
Supported strategies for this milestone:
- BREAKOUT_PROTECTION
- PORTFOLIO_CRASH_SHIELD
- CONVEX_TAIL_LADDER
- EXPIRY_MOVE_NOTE
- MOONSHOT_UPSIDE
- UPSIDE_STEP_LADDER
- DOWNSIDE_CONVEXITY
- DOWNSIDE_STEP_LADDER
- CENTER_BAND_CONDOR
- SMART_BUDGET_SELECTOR
Supported expiry preference: nearest_active.

Rules:
- If the user asks StructX to choose, optimize, recommend, or pick the best strategy, recommendedStrategy = SMART_BUDGET_SELECTOR.
- If the user wants protection, crash hedge, dump protection, or downside coverage, goal = downside_protection and recommendedStrategy = PORTFOLIO_CRASH_SHIELD.
- If the user wants to get paid when BTC expires far away from the current price, recommendedStrategy = EXPIRY_MOVE_NOTE.
- If the user wants a condor, center band, or nearby range with smaller outside wings, goal = range_income and recommendedStrategy = CENTER_BAND_CONDOR.
- If the user wants BTC to grind higher, step up, or go progressively higher, goal = upside_speculation and recommendedStrategy = UPSIDE_STEP_LADDER.
- If the user wants BTC to grind lower, step down, or go progressively lower, goal = downside_protection and recommendedStrategy = DOWNSIDE_STEP_LADDER.
- If the user wants a big move either direction, volatility, or breakout, goal = two_sided_breakout and recommendedStrategy = CONVEX_TAIL_LADDER unless the phrasing is generic enough for BREAKOUT_PROTECTION.
- If the user wants moonshot/upside/rally exposure, goal = upside_speculation and recommendedStrategy = MOONSHOT_UPSIDE.
- conservative -> higher-hit-rate unless user explicitly asks for tail.
- aggressive/max payout -> tail-heavy.
- balanced/default -> balanced.
- If budget is missing, include missingFields [\"budgetDUSDC\"].
- If budget is provided separately, use that.
- Never invent unsupported strategies.
- Always include testnet and not-financial-advice warnings.

Owner: {owner}
User message: {message}
Provided budgetDUSDC: {budget}
Provided riskPreference: {risk}
Provided timePreference: {time}
"#,
        owner = req.owner,
        message = req.message,
        budget = req.budget_dusdc.clone().unwrap_or_default(),
        risk = req.risk_preference.clone().unwrap_or_default(),
        time = req.time_preference.clone().unwrap_or_default(),
    );

    let body = serde_json::json!({
        "model": model,
        "input": input,
        "text": {
            "format": {
                "type": "json_schema",
                "name": "structx_intent",
                "strict": true,
                "schema": schema
            }
        }
    });

    let response = reqwest::Client::new()
        .post("https://api.openai.com/v1/responses")
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;

    let text = extract_openai_text(&response)
        .ok_or_else(|| "OpenAI response missing structured output text".to_string())?;
    let value: serde_json::Value = serde_json::from_str(&text)?;
    let mut parsed = parsed_intent_from_value(req, value)?;
    validate_and_rewrite_intent(&mut parsed);
    Ok(parsed)
}

fn extract_openai_text(response: &serde_json::Value) -> Option<String> {
    let output = response.get("output")?.as_array()?;
    for item in output {
        let content = item.get("content")?.as_array()?;
        for part in content {
            if let Some(text) = part.get("text").and_then(serde_json::Value::as_str) {
                return Some(text.to_string());
            }
        }
    }
    None
}

fn parsed_intent_from_value(
    req: &ParseIntentRequest,
    value: serde_json::Value,
) -> Result<ParsedIntent, Box<dyn std::error::Error + Send + Sync>> {
    let mut parsed = ParsedIntent {
        ok: true,
        intent_id: format!("intent_{}", stable_intent_id(&req.owner, &req.message)),
        owner: req.owner.clone(),
        raw_message: req.message.clone(),
        asset: value.get("asset").and_then(serde_json::Value::as_str).unwrap_or("BTC").to_string(),
        goal: value
            .get("goal")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("two_sided_breakout")
            .to_string(),
        budget_dusdc: value
            .get("budgetDUSDC")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string(),
        risk_preference: value
            .get("riskPreference")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("balanced")
            .to_string(),
        time_preference: value
            .get("timePreference")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("nearest_active")
            .to_string(),
        recommended_strategy: value
            .get("recommendedStrategy")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("BREAKOUT_PROTECTION")
            .to_string(),
        recommended_style: value
            .get("recommendedStyle")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("balanced")
            .to_string(),
        confidence: value.get("confidence").and_then(serde_json::Value::as_f64).unwrap_or(0.65),
        reasoning_summary: value
            .get("reasoningSummary")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("Strategy selected from parsed user intent.")
            .to_string(),
        missing_fields: value
            .get("missingFields")
            .and_then(serde_json::Value::as_array)
            .map(|items| {
                items.iter().filter_map(serde_json::Value::as_str).map(ToOwned::to_owned).collect()
            })
            .unwrap_or_default(),
        warnings: value
            .get("warnings")
            .and_then(serde_json::Value::as_array)
            .map(|items| {
                items.iter().filter_map(serde_json::Value::as_str).map(ToOwned::to_owned).collect()
            })
            .unwrap_or_default(),
    };

    if parsed.budget_dusdc.is_empty() {
        if let Some(budget) = &req.budget_dusdc {
            parsed.budget_dusdc = budget.clone();
        }
    }

    Ok(parsed)
}

fn deterministic_parse_intent(req: &ParseIntentRequest) -> ParsedIntent {
    let msg = req.message.to_lowercase();
    let (goal, recommended_strategy) = if contains_any(
        &msg,
        &["choose for me", "best strategy", "optimize", "smart", "recommend", "pick for me"],
    ) {
        ("two_sided_breakout", "SMART_BUDGET_SELECTOR")
    } else if contains_any(&msg, &["near barrier", "barrier", "close to target", "near target"]) {
        ("two_sided_breakout", "NEAR_BARRIER_PROXY")
    } else if contains_any(
        &msg,
        &["expires far", "far from current", "expiry move", "terminal move"],
    ) {
        ("two_sided_breakout", "EXPIRY_MOVE_NOTE")
    } else if contains_any(
        &msg,
        &[
            "condor",
            "center band",
            "near current with wings",
            "range with wings",
            "nearby with protection",
        ],
    ) {
        ("range_income", "CENTER_BAND_CONDOR")
    } else if contains_any(
        &msg,
        &["portfolio", "hedge my", "protect my", "protection", "insurance"],
    ) {
        ("downside_protection", "PORTFOLIO_CRASH_SHIELD")
    } else if contains_any(
        &msg,
        &["grind lower", "step down", "staged downside", "progressively lower", "keeps going down"],
    ) {
        ("downside_protection", "DOWNSIDE_STEP_LADDER")
    } else if contains_any(
        &msg,
        &["bearish", "downside", "breakdown", "dump", "crash", "sell-off", "selldown"],
    ) {
        ("downside_protection", "DOWNSIDE_CONVEXITY")
    } else if contains_any(
        &msg,
        &["grind higher", "step up", "staged upside", "progressively higher", "keeps going up"],
    ) {
        ("upside_speculation", "UPSIDE_STEP_LADDER")
    } else if contains_any(&msg, &["moon", "upside", "rally", "pump", "breaks up", "breakout up"]) {
        ("upside_speculation", "MOONSHOT_UPSIDE")
    } else if contains_any(
        &msg,
        &["big move", "breakout", "volatile", "volatility", "either direction", "move a lot"],
    ) {
        ("two_sided_breakout", "CONVEX_TAIL_LADDER")
    } else {
        ("two_sided_breakout", "SMART_BUDGET_SELECTOR")
    };

    let risk =
        req.risk_preference.clone().unwrap_or_else(|| infer_risk_preference(&msg).to_string());
    let style = style_from_goal_and_risk(goal, &risk).to_string();
    let budget = req.budget_dusdc.clone().unwrap_or_default();

    let mut missing_fields = Vec::new();
    if budget.trim().is_empty() {
        missing_fields.push("budgetDUSDC".to_string());
    }

    let mut parsed = ParsedIntent {
        ok: missing_fields.is_empty(),
        intent_id: format!("intent_{}", stable_intent_id(&req.owner, &req.message)),
        owner: req.owner.clone(),
        raw_message: req.message.clone(),
        asset: "BTC".to_string(),
        goal: goal.to_string(),
        budget_dusdc: budget,
        risk_preference: normalize_risk(&risk).to_string(),
        time_preference: req
            .time_preference
            .clone()
            .unwrap_or_else(|| "nearest_active".to_string()),
        recommended_strategy: recommended_strategy.to_string(),
        recommended_style: style,
        confidence: 0.62,
        reasoning_summary: reasoning_for_recommendation(goal, recommended_strategy).to_string(),
        missing_fields,
        warnings: vec![
            "Use this strategy suggestion as a starting point and review the payoff before opening."
                .to_string(),
            "This version uses DeepBook Predict on Sui Testnet.".to_string(),
            "StructX calculates the premium and payoff from the selected market before your wallet opens the position."
                .to_string(),
        ],
    };

    validate_and_rewrite_intent(&mut parsed);
    parsed
}

fn validate_and_rewrite_intent(parsed: &mut ParsedIntent) {
    parsed.asset = "BTC".to_string();
    parsed.risk_preference = normalize_risk(&parsed.risk_preference).to_string();

    if !matches!(
        parsed.recommended_strategy.as_str(),
        "BREAKOUT_PROTECTION"
            | "PORTFOLIO_CRASH_SHIELD"
            | "CONVEX_TAIL_LADDER"
            | "EXPIRY_MOVE_NOTE"
            | "MOONSHOT_UPSIDE"
            | "UPSIDE_STEP_LADDER"
            | "DOWNSIDE_CONVEXITY"
            | "DOWNSIDE_STEP_LADDER"
            | "CENTER_BAND_CONDOR"
            | "NEAR_BARRIER_PROXY"
            | "SMART_BUDGET_SELECTOR"
    ) {
        parsed.recommended_strategy = "SMART_BUDGET_SELECTOR".to_string();
    }

    if !matches!(parsed.time_preference.as_str(), "nearest_active" | "today" | "this_week") {
        parsed.time_preference = "nearest_active".to_string();
    }

    if !matches!(parsed.recommended_style.as_str(), "tail-heavy" | "balanced" | "higher-hit-rate") {
        parsed.recommended_style =
            style_from_goal_and_risk(&parsed.goal, &parsed.risk_preference).to_string();
    }

    let has_valid_budget =
        parsed.budget_dusdc.trim().parse::<f64>().map(|value| value > 0.0).unwrap_or(false);

    if !has_valid_budget && !parsed.missing_fields.iter().any(|field| field == "budgetDUSDC") {
        parsed.missing_fields.push("budgetDUSDC".to_string());
    }

    if !parsed.warnings.iter().any(|warning| warning.to_lowercase().contains("starting point")) {
        parsed.warnings.push(
            "Use this strategy suggestion as a starting point and review the payoff before opening."
                .to_string(),
        );
    }

    if !parsed.warnings.iter().any(|warning| warning.to_lowercase().contains("testnet")) {
        parsed.warnings.push("This version uses DeepBook Predict on Sui Testnet.".to_string());
    }

    parsed.ok = parsed.missing_fields.is_empty();
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

fn infer_risk_preference(message: &str) -> &'static str {
    if contains_any(message, &["safe", "safer", "conservative", "higher probability", "likely"]) {
        "conservative"
    } else if contains_any(message, &["aggressive", "max payout", "lottery", "tail", "cheap"]) {
        "aggressive"
    } else {
        "balanced"
    }
}

fn normalize_risk(value: &str) -> &'static str {
    match value {
        "conservative" => "conservative",
        "aggressive" => "aggressive",
        _ => "balanced",
    }
}

fn style_from_goal_and_risk(goal: &str, risk: &str) -> &'static str {
    match (goal, risk) {
        (_, "conservative") => "higher-hit-rate",
        (_, "aggressive") => "tail-heavy",
        ("downside_protection", _) => "tail-heavy",
        ("upside_speculation", _) => "tail-heavy",
        ("two_sided_breakout", _) => "balanced",
        _ => "balanced",
    }
}

fn reasoning_for_recommendation(goal: &str, recommended_strategy: &str) -> &'static str {
    match recommended_strategy {
        "PORTFOLIO_CRASH_SHIELD" => {
            "Crash Insurance fits your view by sizing downside protection around a deeper BTC sell-off. Your maximum loss is the premium."
        }
        "CONVEX_TAIL_LADDER" => {
            "Convex Tail Ladder covers moves in either direction and puts more of the payout in the largest expiry moves."
        }
        "EXPIRY_MOVE_NOTE" => {
            "Expiry Move Note fits a view that BTC will finish well away from its current price."
        }
        "MOONSHOT_UPSIDE" => {
            "Moonshot Upside focuses the payout above the upper band while keeping your maximum loss to the premium."
        }
        "UPSIDE_STEP_LADDER" => {
            "Upside Step Ladder builds the payout across a steady rise, a breakout, and a larger continuation move."
        }
        "DOWNSIDE_STEP_LADDER" => {
            "Downside Step Ladder builds the payout across a steady decline, a breakdown, and a larger continuation move."
        }
        "CENTER_BAND_CONDOR" => {
            "Center Band Condor puts most of the payout near the current price and keeps smaller ranges on either side."
        }
        "DOWNSIDE_CONVEXITY" => {
            "Downside Convexity focuses the payout below the lower band while keeping your maximum loss to the premium."
        }
        "SMART_BUDGET_SELECTOR" => {
            "StructX compared the available strategies and chose the one that fits your budget and preferred payoff style."
        }
        _ => match goal {
            "downside_protection" => {
                "Breakout Protection can put more of your budget into the downside tail while keeping your maximum loss to the premium."
            }
            "upside_speculation" => {
                "Breakout Protection gives you upside participation with a maximum loss set by the premium."
            }
            "two_sided_breakout" => {
                "Breakout Protection fits a large-move view that covers either direction."
            }
            _ => {
                "Breakout Protection gives you a clear, defined-risk payoff across both sides of the market."
            }
        },
    }
}

fn build_clarifying_question(parsed: &ParsedIntent) -> String {
    if parsed.missing_fields.iter().any(|field| field == "budgetDUSDC") {
        "How much dUSDC do you want to allocate?".to_string()
    } else {
        "Can you clarify your budget, goal, and preferred time horizon?".to_string()
    }
}

fn stable_intent_id(owner: &str, message: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in owner.bytes().chain(message.bytes()) {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:x}")
}

fn normalize_strategy_id(strategy: &str) -> String {
    let upper = strategy.trim().to_uppercase();
    match upper.as_str() {
        "BREAKOUT_PROTECTION"
        | "PORTFOLIO_CRASH_SHIELD"
        | "CONVEX_TAIL_LADDER"
        | "MOONSHOT_UPSIDE"
        | "UPSIDE_STEP_LADDER"
        | "DOWNSIDE_CONVEXITY"
        | "DOWNSIDE_STEP_LADDER"
        | "CENTER_BAND_CONDOR"
        | "EXPIRY_MOVE_NOTE"
        | "SMART_BUDGET_SELECTOR" => upper,
        "CRASH_INSURANCE" => "PORTFOLIO_CRASH_SHIELD".to_string(),
        _ => "BREAKOUT_PROTECTION".to_string(),
    }
}

fn strategy_allowed_roles(strategy: &str) -> &'static [&'static str] {
    match strategy {
        // Downside-only structures.
        "CRASH_INSURANCE" | "PORTFOLIO_CRASH_SHIELD" | "DOWNSIDE_CONVEXITY" => {
            &["extreme_downside", "moderate_downside"]
        }
        // Upside-only structure.
        "MOONSHOT_UPSIDE" => &["moderate_upside", "extreme_upside"],
        // Range / expiry-move structure keeps only the two RANGE legs.
        "EXPIRY_MOVE_NOTE" => &["moderate_downside", "moderate_upside"],
        // Four-leg presets reuse the full Breakout payoff (DOWN + 2 RANGE + UP).
        // BREAKOUT_PROTECTION, CONVEX_TAIL_LADDER, SMART_BUDGET_SELECTOR and any
        // unknown strategy fall through here so the underlying compile is
        // returned unchanged.
        _ => &["extreme_downside", "moderate_downside", "moderate_upside", "extreme_upside"],
    }
}

fn role_for_payoff_bucket(idx: usize) -> Option<&'static str> {
    match idx {
        0 => Some("extreme_downside"),
        1 => Some("moderate_downside"),
        2 => None,
        3 => Some("moderate_upside"),
        4 => Some("extreme_upside"),
        _ => None,
    }
}

fn format_signed_dusdc(raw: i128) -> String {
    if raw >= 0 {
        format_dusdc_raw_u128(raw as u128)
    } else {
        format!("-{}", format_dusdc_raw_u128((-raw) as u128))
    }
}

fn apply_strategy_template(
    compiled: &serde_json::Value,
    strategy: &str,
) -> Result<serde_json::Value, (StatusCode, serde_json::Value)> {
    let allowed = strategy_allowed_roles(strategy);

    let mut out = compiled.clone();
    let obj = out.as_object_mut().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            api_error(
                "COMPILE_FAILED",
                "Compile failed",
                "Compile response was not a JSON object.",
                "Retry.",
                None,
                None,
            ),
        )
    })?;

    // Filter legs by role.
    let original_legs =
        obj.get("legs").and_then(serde_json::Value::as_array).cloned().unwrap_or_default();

    let kept_legs: Vec<serde_json::Value> = original_legs
        .into_iter()
        .filter(|leg| {
            let role = leg.get("role").and_then(serde_json::Value::as_str).unwrap_or("");
            allowed.contains(&role)
        })
        .collect();

    if kept_legs.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            api_error(
                "COMPILE_FAILED",
                "No legs available",
                "The compiled strategy returned no legs that match this preset.",
                "Try a different budget or style.",
                None,
                None,
            ),
        ));
    }

    let new_premium: u128 = kept_legs
        .iter()
        .filter_map(|leg| {
            leg.get("premiumRaw")
                .and_then(serde_json::Value::as_str)
                .and_then(|v| v.parse::<u128>().ok())
        })
        .sum();
    let new_max_gross: u128 = kept_legs
        .iter()
        .filter_map(|leg| {
            leg.get("quantityRaw")
                .and_then(serde_json::Value::as_str)
                .and_then(|v| v.parse::<u128>().ok())
        })
        .max()
        .unwrap_or(0);
    let new_max_loss = new_premium;
    let new_max_net = new_max_gross.saturating_sub(new_premium);

    // Recompute payoff table — keep the same 5 buckets so the UI is consistent.
    let payoff_table =
        obj.get("payoffTable").and_then(serde_json::Value::as_array).cloned().unwrap_or_default();

    let kept_role_set: std::collections::HashSet<String> = kept_legs
        .iter()
        .filter_map(|leg| {
            leg.get("role").and_then(serde_json::Value::as_str).map(|s| s.to_string())
        })
        .collect();

    let new_payoff_table: Vec<serde_json::Value> = payoff_table
        .into_iter()
        .enumerate()
        .map(|(idx, mut row)| {
            let bucket_role = role_for_payoff_bucket(idx);
            let leg_pays = match bucket_role {
                Some(role) => kept_role_set.contains(role),
                None => false,
            };
            let original_gross = row
                .get("grossPayoutRaw")
                .and_then(serde_json::Value::as_str)
                .and_then(|v| v.parse::<u128>().ok())
                .unwrap_or(0);
            let new_gross: u128 = if leg_pays { original_gross } else { 0 };
            let new_net: i128 = new_gross as i128 - new_premium as i128;
            if let Some(map) = row.as_object_mut() {
                map.insert(
                    "grossPayoutRaw".to_string(),
                    serde_json::Value::String(new_gross.to_string()),
                );
                map.insert(
                    "grossPayoutDisplay".to_string(),
                    serde_json::Value::String(format!(
                        "{} dUSDC",
                        format_dusdc_raw_u128(new_gross).trim_end_matches(" dUSDC")
                    )),
                );
                map.insert("netPnlRaw".to_string(), serde_json::Value::String(new_net.to_string()));
                map.insert(
                    "netPnlDisplay".to_string(),
                    serde_json::Value::String(format!(
                        "{} dUSDC",
                        format_signed_dusdc(new_net).trim_end_matches(" dUSDC")
                    )),
                );
            }
            row
        })
        .collect();

    // Update strategy + financial summary fields.
    obj.insert("strategy".to_string(), serde_json::Value::String(strategy.to_string()));
    obj.insert(
        "selectionMode".to_string(),
        serde_json::Value::String("preset_filtered".to_string()),
    );
    obj.insert("presetTemplate".to_string(), serde_json::Value::String(strategy.to_string()));
    obj.insert("legs".to_string(), serde_json::Value::Array(kept_legs));
    obj.insert("payoffTable".to_string(), serde_json::Value::Array(new_payoff_table));
    obj.insert(
        "premiumRequiredRaw".to_string(),
        serde_json::Value::String(new_premium.to_string()),
    );
    obj.insert(
        "premiumRequiredDisplay".to_string(),
        serde_json::Value::String(format_dusdc_raw_u128(new_premium)),
    );
    obj.insert("maxLossRaw".to_string(), serde_json::Value::String(new_max_loss.to_string()));
    obj.insert(
        "maxLossDisplay".to_string(),
        serde_json::Value::String(format_dusdc_raw_u128(new_max_loss)),
    );
    obj.insert(
        "maxGrossPayoutRaw".to_string(),
        serde_json::Value::String(new_max_gross.to_string()),
    );
    obj.insert(
        "maxGrossPayoutDisplay".to_string(),
        serde_json::Value::String(format_dusdc_raw_u128(new_max_gross)),
    );
    obj.insert("maxNetPayoutRaw".to_string(), serde_json::Value::String(new_max_net.to_string()));
    obj.insert(
        "maxNetPayoutDisplay".to_string(),
        serde_json::Value::String(format_dusdc_raw_u128(new_max_net)),
    );

    // Replace the first segment of the id with the strategy slug while keeping
    // the rest of the identifier intact so compiled_expiry_ms still parses.
    if let Some(original_id) = obj.get("compiledStrategyId").and_then(serde_json::Value::as_str) {
        let suffix = original_id.split_once(':').map(|(_, rest)| rest).unwrap_or("");
        let strategy_slug = strategy.to_lowercase();
        let new_id =
            if suffix.is_empty() { strategy_slug } else { format!("{}:{}", strategy_slug, suffix) };
        obj.insert("compiledStrategyId".to_string(), serde_json::Value::String(new_id));
    }

    Ok(out)
}

async fn find_mintable_strategy(
    State(state): State<Arc<AppState>>,
    Json(req): Json<FindMintableStrategyRequest>,
) -> impl IntoResponse {
    let compile_request = CompileStrategyRequest {
        owner: req.owner.clone(),
        strategy: "BREAKOUT_PROTECTION".to_string(),
        budget_dusdc: req.budget_dusdc.clone(),
        style: req.style.clone(),
        expiry_preference: req.expiry_preference.clone(),
        slippage_bps: req.slippage_bps,
        bucket_step_usd: None,
        custom_k1_price: None,
        custom_k2_price: None,
        custom_k3_price: None,
        custom_k4_price: None,
        portfolio_exposure_dusdc: None,
        over_hedge_cap_bps: None,
        convex_gamma_bps: None,
        dead_zone_bps: None,
        moonshot_range_weight_bps: None,
        moonshot_tail_gamma_bps: None,
        downside_range_weight_bps: None,
        downside_tail_gamma_bps: None,
        upside_near_range_weight_bps: None,
        upside_upper_range_weight_bps: None,
        upside_tail_gamma_bps: None,
        downside_near_range_weight_bps: None,
        downside_lower_range_weight_bps: None,
        downside_step_tail_gamma_bps: None,
        condor_center_weight_bps: None,
        barrier_side: None,
        barrier_near_range_weight_bps: None,
        barrier_tail_gamma_bps: None,
    };

    let compiled = match compile_strategy_service_value(&compile_request).await {
        Ok(compiled) => compiled,
        Err((status, value)) => return (status, Json(value)),
    };

    let Some(compiled_strategy_id) =
        compiled.get("compiledStrategyId").and_then(serde_json::Value::as_str)
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "ok": false,
                "error": "compiled response missing compiledStrategyId"
            })),
        );
    };

    let Some(expiry_ms) = compiled_expiry_ms(compiled_strategy_id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "ok": false,
                "error": "compiled strategy id is missing expiry information"
            })),
        );
    };

    match find_best_effort_mintable_compiled(
        &req.owner,
        &req.manager_id,
        &compiled,
        &expiry_ms,
        false,
    )
    .await
    {
        Ok((mintable_compiled, extra_warnings)) => {
            if let Some(id) =
                mintable_compiled.get("compiledStrategyId").and_then(serde_json::Value::as_str)
            {
                state.compiled.lock().await.insert(id.to_string(), mintable_compiled.clone());
            }

            let mut warnings = vec![
                "Executable means mint checks passed at the time of checking; wallet signing can still fail if market state changes."
                    .to_string(),
                "Quote success alone is not treated as executable.".to_string(),
            ];
            warnings.extend(extra_warnings);

            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "ok": true,
                    "executable": true,
                    "network": "sui:testnet",
                    "owner": req.owner,
                    "managerId": req.manager_id,
                    "budgetDisplay": req.budget_dusdc,
                    "compiled": mintable_compiled,
                    "mintDevInspect": {
                        "status": "success",
                        "stdout": "Direct backend mintability checks passed for the compiled strategy."
                    },
                    "failures": [],
                    "warnings": warnings,
                })),
            )
        }
        Err((_status, value)) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "ok": true,
                "executable": false,
                "network": "sui:testnet",
                "owner": req.owner,
                "managerId": req.manager_id,
                "message": value
                    .get("error")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("No currently mintable breakout candidate found."),
                "details": value,
                "failures": [],
                "warnings": [
                    "Strategy unavailable right now under current Predict market conditions.",
                    "Quote success does not imply mintability."
                ]
            })),
        ),
    }
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { ok: true, service: "structx-api" })
}

/// GET /api/managers/:address — returns the PredictManager id previously
/// stored for this wallet, or null if none has been persisted yet.
///
/// Cached for 30s in the browser via `Cache-Control: private, max-age=30`.
/// The manager-id mapping changes only when the user creates a new manager,
/// and on that path we explicitly POST + invalidate the frontend cache, so
/// the 30s window is safe and removes redundant round-trips when the user
/// navigates between strategy pages within a session.
#[derive(Debug, Deserialize)]
struct ListPositionsQuery {
    owner: String,
    #[serde(rename = "managerId")]
    manager_id: String,
}

#[derive(Debug, Deserialize)]
struct SyncPositionsRequest {
    owner: String,
    #[serde(rename = "managerId")]
    manager_id: String,
}

#[derive(Debug, Deserialize)]
struct MarketSearchParams {
    q: Option<String>,
    quote_asset: Option<String>,
    require_active: Option<bool>,
    category: Option<String>,
    kind: Option<String>,
    expiry: Option<String>,
}

#[derive(Debug, Serialize)]
struct MarketCatalogRefreshResponse {
    ok: bool,
    market_count: usize,
    active_market_count: usize,
    report: structx_service::CatalogBuildReport,
}

/// GET /api/markets — fetch the active DeepBook Predict market directory.
///
/// This intentionally does not shell out to the CLI. The API persists the last
/// good snapshot to disk and serves it immediately on subsequent requests,
/// while revalidating against DeepBook Predict in the background when stale.
async fn list_markets(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let cached = read_cached_markets();

    if let Some(record) = &cached {
        if markets_cache_is_fresh(record) {
            return (StatusCode::OK, Json(markets_cache_response(record, "disk", false)));
        }

        if markets_refresh_in_flight(&state).await {
            return (StatusCode::OK, Json(markets_cache_response(record, "disk_refreshing", true)));
        }

        match refresh_markets_snapshot(state.clone()).await {
            Ok(envelope) => return (StatusCode::OK, Json(envelope)),
            Err(err) => {
                eprintln!("warning: synchronous markets refresh failed: {err}");
                maybe_spawn_markets_refresh(state.clone()).await;
                return (StatusCode::OK, Json(markets_cache_response(record, "disk_stale", true)));
            }
        }
    }

    match refresh_markets_snapshot(state).await {
        Ok(envelope) => (StatusCode::OK, Json(envelope)),
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({
                "ok": false,
                "error": err,
            })),
        ),
    }
}

fn build_markets_envelope(markets: &[MarketSnapshot]) -> serde_json::Value {
    let usable = markets.iter().filter(|market| market.structx_status.is_usable()).count();
    let deepbook_only = markets
        .iter()
        .filter(|market| match &market.structx_status {
            deepbook_client::StructxMarketStatus::Rejected { reasons, .. } => {
                !reasons.is_empty()
                    && reasons.iter().all(|reason| {
                        matches!(reason, deepbook_client::MarketRejectionReason::NonBtc)
                    })
            }
            _ => false,
        })
        .count();
    let warnings = markets
        .iter()
        .filter(|market| {
            matches!(
                market.structx_status,
                deepbook_client::StructxMarketStatus::UsableWithWarnings(_)
            )
        })
        .count();
    let asset_count = markets
        .iter()
        .filter_map(|market| market.underlying())
        .map(|asset| asset.to_ascii_uppercase())
        .collect::<std::collections::BTreeSet<_>>()
        .len();

    serde_json::json!({
        "ok": true,
        "asset": "ALL",
        "network": "sui:testnet",
        "totalCount": markets.len(),
        "usableCount": usable,
        "deepbookOnlyCount": deepbook_only,
        "warningsCount": warnings,
        "assetCount": asset_count,
        "structxSupportedAsset": "BTC",
        "markets": markets,
    })
}

fn markets_cache_age(record: &MarketsCacheRecord) -> Duration {
    let now = storage::unix_now();
    if record.refreshed_at_unix >= now {
        Duration::from_secs(0)
    } else {
        Duration::from_secs((now - record.refreshed_at_unix) as u64)
    }
}

fn markets_cache_is_fresh(record: &MarketsCacheRecord) -> bool {
    markets_cache_age(record) <= MARKETS_CACHE_FRESH_FOR
}

fn markets_cache_response(
    record: &MarketsCacheRecord,
    source: &str,
    stale: bool,
) -> serde_json::Value {
    let mut envelope = record.envelope.clone();
    if let Some(obj) = envelope.as_object_mut() {
        obj.insert("cachedAtUnix".to_string(), serde_json::Value::from(record.refreshed_at_unix));
        obj.insert(
            "cacheAgeSeconds".to_string(),
            serde_json::Value::from(markets_cache_age(record).as_secs()),
        );
        obj.insert("cacheSource".to_string(), serde_json::Value::from(source));
        obj.insert("stale".to_string(), serde_json::Value::from(stale));
    }
    envelope
}

fn read_cached_markets() -> Option<MarketsCacheRecord> {
    match storage::read_json::<MarketsCacheRecord>(&storage::all_markets_cache_path()) {
        Ok(record) => record,
        Err(err) => {
            eprintln!(
                "warning: could not read markets cache {} ({err})",
                storage::all_markets_cache_path().display()
            );
            None
        }
    }
    .or_else(|| {
        match storage::read_json::<MarketsCacheRecord>(&storage::btc_markets_cache_path()) {
            Ok(record) => record,
            Err(err) => {
                eprintln!(
                    "warning: could not read legacy BTC markets cache {} ({err})",
                    storage::btc_markets_cache_path().display()
                );
                None
            }
        }
    })
}

async fn markets_refresh_in_flight(state: &Arc<AppState>) -> bool {
    state.markets_refresh.lock().await.in_flight
}

async fn refresh_markets_snapshot(state: Arc<AppState>) -> Result<serde_json::Value, String> {
    {
        let mut refresh = state.markets_refresh.lock().await;
        refresh.in_flight = true;
        refresh.last_started_at = Some(Instant::now());
    }

    let client = match DeepBookClient::new(DeepBookConfig::default()) {
        Ok(client) => client,
        Err(err) => return Err(format!("could not initialize DeepBook client: {err}")),
    };

    match client.load_market_directory(FreshnessConfig::default()).await {
        Ok(markets) => {
            let mut envelope = build_markets_envelope(&markets);
            let now = storage::unix_now();
            if let Some(obj) = envelope.as_object_mut() {
                obj.insert("cachedAtUnix".to_string(), serde_json::Value::from(now));
                obj.insert("cacheSource".to_string(), serde_json::Value::from("deepbook_refresh"));
                obj.insert("stale".to_string(), serde_json::Value::from(false));
            }

            let record = MarketsCacheRecord {
                schema_version: 1,
                refreshed_at_unix: now,
                envelope: envelope.clone(),
            };
            if let Err(err) =
                storage::atomic_write_json(&storage::all_markets_cache_path(), &record)
            {
                eprintln!(
                    "warning: could not persist markets cache {} ({err})",
                    storage::all_markets_cache_path().display()
                );
            }

            let mut refresh = state.markets_refresh.lock().await;
            refresh.in_flight = false;
            Ok(envelope)
        }
        Err(err) => {
            let mut refresh = state.markets_refresh.lock().await;
            refresh.in_flight = false;
            Err(format!("could not load markets from DeepBook Predict: {err}"))
        }
    }
}

async fn maybe_spawn_markets_refresh(state: Arc<AppState>) {
    let mut refresh = state.markets_refresh.lock().await;
    if refresh.in_flight {
        return;
    }
    if let Some(last_started_at) = refresh.last_started_at {
        if last_started_at.elapsed() < MARKETS_REFRESH_THROTTLE {
            return;
        }
    }
    refresh.in_flight = true;
    refresh.last_started_at = Some(Instant::now());
    drop(refresh);

    tokio::spawn(async move {
        let _ = refresh_markets_snapshot(state).await;
    });
}

fn spawn_markets_cache_warmer(state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(MARKETS_CACHE_WARM_INTERVAL);
        loop {
            interval.tick().await;
            maybe_spawn_markets_refresh(state.clone()).await;
        }
    });
}

async fn get_market_catalog_status() -> impl IntoResponse {
    let store = DiskMarketStore::default_state_dir();

    match load_catalog_status(&store).await {
        Ok(status) => (StatusCode::OK, Json(serde_json::to_value(status).unwrap_or_default())),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "ok": false,
                "error": format!("failed to load market catalog status: {err}")
            })),
        ),
    }
}

async fn search_market_catalog(Query(params): Query<MarketSearchParams>) -> impl IntoResponse {
    let store = DiskMarketStore::default_state_dir();
    let query = MarketSearchQuery {
        text: params.q.unwrap_or_default(),
        category_hint: parse_market_category(params.category),
        market_kind_hint: parse_market_kind(params.kind),
        require_active: params.require_active.unwrap_or(true),
        quote_asset: params.quote_asset.or_else(|| Some("DUSDC".to_string())),
        expiry_preference: parse_expiry_preference(params.expiry)
            .or(Some(ExpiryPreference::NearestActive)),
    };

    match store.search_markets(query).await {
        Ok(markets) => (
            StatusCode::OK,
            Json(serde_json::to_value(markets).unwrap_or_else(|_| serde_json::json!([]))),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "ok": false,
                "error": format!("failed to search market catalog: {err}")
            })),
        ),
    }
}

async fn get_catalog_market(Path(market_id): Path<String>) -> impl IntoResponse {
    let store = DiskMarketStore::default_state_dir();

    match store.get_market(&market_id).await {
        Ok(Some(market)) => {
            (StatusCode::OK, Json(serde_json::to_value(market).unwrap_or_default()))
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "ok": false,
                "error": format!("market not found: {market_id}")
            })),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "ok": false,
                "error": format!("failed to load catalog market: {err}")
            })),
        ),
    }
}

async fn refresh_market_catalog() -> impl IntoResponse {
    let store = DiskMarketStore::default_state_dir();

    let raw_markets_json = match load_existing_markets_json().await {
        Ok(value) => value,
        Err(err) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "ok": false,
                    "error": format!("failed to load live markets for catalog refresh: {err}")
                })),
            );
        }
    };

    match refresh_catalog_from_existing_markets_json(&store, raw_markets_json).await {
        Ok((catalog, report)) => {
            let active_market_count =
                catalog.markets.iter().filter(|m| m.status == MarketStatus::Active).count();

            (
                StatusCode::OK,
                Json(
                    serde_json::to_value(MarketCatalogRefreshResponse {
                        ok: true,
                        market_count: catalog.markets.len(),
                        active_market_count,
                        report,
                    })
                    .unwrap_or_default(),
                ),
            )
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "ok": false,
                "error": format!("failed to refresh market catalog: {err}")
            })),
        ),
    }
}

async fn load_existing_markets_json() -> Result<serde_json::Value, String> {
    let client = DeepBookClient::new(DeepBookConfig::default())
        .map_err(|err| format!("could not initialize DeepBook client: {err}"))?;

    let markets = client
        .load_market_directory(FreshnessConfig::default())
        .await
        .map_err(|err| format!("could not load market directory: {err}"))?;

    Ok(build_markets_envelope(&markets))
}

/// GET /api/positions?owner=&managerId= — read the disk-backed ledger and
/// return all known positions plus an aggregate summary. This endpoint never
/// hits Sui RPC; valuation refresh is a separate explicit action (later
/// slice) so loading the page is always fast.
async fn list_positions(Query(q): Query<ListPositionsQuery>) -> impl IntoResponse {
    let mut ledger = match PositionLedger::load(&q.owner, &q.manager_id) {
        Ok(l) => l,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "ok": false,
                    "error": format!("could not load position ledger: {err}"),
                })),
            );
        }
    };
    let mut warnings: Vec<String> = Vec::new();

    if let Err(errs) = refresh_position_previews(&q.owner, &q.manager_id, &mut ledger).await {
        warnings.extend(errs);
    } else if let Err(err) = ledger.save() {
        warnings.push(format!("Could not persist refreshed position previews: {err}"));
    }

    let summary = ledger.summary();
    let body = serde_json::json!({
        "ok": true,
        "owner": ledger.owner,
        "managerId": ledger.manager_id,
        "positions": ledger.positions,
        "summary": summary,
        "auditDigests": ledger.audit_digests,
        "redeemDigests": ledger.redeem_digests,
        "updatedAtUnix": ledger.updated_at_unix,
        "warnings": warnings,
    });
    (StatusCode::OK, Json(body))
}

async fn refresh_position_previews(
    owner: &str,
    manager_id: &str,
    ledger: &mut PositionLedger,
) -> Result<(), Vec<String>> {
    if !ledger.positions.iter().any(|p| matches!(p.status, position_ledger::PositionStatus::Open)) {
        return Ok(());
    }

    let rpc = match SuiRpcClient::new(DEFAULT_SUI_TESTNET_RPC_URL, Duration::from_secs(20)) {
        Ok(rpc) => rpc,
        Err(err) => {
            return Err(vec![format!(
                "Could not initialize Sui RPC client for position previews: {err}"
            )]);
        }
    };

    let predict = match resolve_sui_object(&rpc, PREDICT_OBJECT_ID).await {
        Ok(obj) => obj,
        Err((_, value)) => {
            return Err(vec![format!(
                "Could not fetch predict object for previews: {}",
                value.get("error").and_then(serde_json::Value::as_str).unwrap_or("unknown error")
            )]);
        }
    };
    let manager = match resolve_sui_object(&rpc, manager_id).await {
        Ok(obj) => obj,
        Err((_, value)) => {
            return Err(vec![format!(
                "Could not fetch manager object for previews: {}",
                value.get("error").and_then(serde_json::Value::as_str).unwrap_or("unknown error")
            )]);
        }
    };
    let clock = match resolve_sui_object(&rpc, CLOCK_OBJECT_ID).await {
        Ok(obj) => obj,
        Err((_, value)) => {
            return Err(vec![format!(
                "Could not fetch clock object for previews: {}",
                value.get("error").and_then(serde_json::Value::as_str).unwrap_or("unknown error")
            )]);
        }
    };

    if let Err((_, value)) = validate_predict_manager_object(&manager) {
        return Err(vec![format!(
            "Manager object is invalid for previews: {}",
            value.get("error").and_then(serde_json::Value::as_str).unwrap_or("unknown error")
        )]);
    }

    let mut warnings = Vec::new();
    let open_positions = ledger
        .positions
        .iter()
        .filter(|p| matches!(p.status, position_ledger::PositionStatus::Open))
        .cloned()
        .collect::<Vec<_>>();

    for position in open_positions {
        let Some(read) = position_to_redeem_read(&position) else {
            warnings.push(format!(
                "Skipping preview refresh for {} because its strike/range key is incomplete.",
                position.position_id
            ));
            continue;
        };

        let oracle = match resolve_sui_object(&rpc, &position.oracle_id).await {
            Ok(obj) => obj,
            Err((_, value)) => {
                warnings.push(format!(
                    "Could not fetch oracle {} for {}: {}",
                    position.oracle_id,
                    position.position_id,
                    value
                        .get("error")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("unknown error")
                ));
                continue;
            }
        };

        if let Err((_, value)) = validate_quote_object_refs(&predict, &oracle, &clock) {
            warnings.push(format!(
                "Object validation failed for {}: {}",
                position.position_id,
                value.get("error").and_then(serde_json::Value::as_str).unwrap_or("unknown error")
            ));
            continue;
        }

        let tx_kind = match build_redeem_tx_kind(
            &[read],
            MintObjectRefs { predict: &predict, manager: &manager, oracle: &oracle, clock: &clock },
            owner,
        ) {
            Ok(tx_kind) => tx_kind,
            Err(err) => {
                warnings.push(format!(
                    "Could not build preview redeem tx for {}: {err}",
                    position.position_id
                ));
                continue;
            }
        };

        let response =
            match rpc.dev_inspect_transaction_kind(&tx_kind.sender, &tx_kind.tx_kind_b64).await {
                Ok(response) => response,
                Err(err) => {
                    warnings.push(format!("devInspect failed for {}: {err}", position.position_id));
                    continue;
                }
            };

        let execution_status = response
            .get("effects")
            .and_then(|effects| effects.get("status"))
            .and_then(|status| status.get("status"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown");

        if execution_status != "success" {
            warnings.push(format!(
                "Preview devInspect did not succeed for {}: {}",
                position.position_id,
                response
                    .get("effects")
                    .and_then(|effects| effects.get("status"))
                    .and_then(|status| status.get("error"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("unknown error")
            ));
            continue;
        }

        let redeemed = parse_redeemed_legs_from_events(
            response
                .get("events")
                .and_then(serde_json::Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or(&[]),
        );
        let payout_raw = redeemed.first().map(|leg| leg.payout_raw).unwrap_or(0);
        let premium_paid_raw = position.premium_paid_raw.parse::<u128>().unwrap_or(0);
        let original_quantity_raw = position.original_quantity_raw.parse::<u128>().unwrap_or(0);
        let redeem_quantity_raw = position.remaining_quantity_raw.parse::<u128>().unwrap_or(0);
        let basis =
            premium_basis_for_slice(premium_paid_raw, original_quantity_raw, redeem_quantity_raw);
        let pnl_raw = (payout_raw as i128).saturating_sub(basis as i128);

        ledger.apply_preview(&position.position_id, payout_raw, pnl_raw, storage::unix_now());
    }

    if warnings.is_empty() {
        Ok(())
    } else {
        Err(warnings)
    }
}

fn position_to_redeem_read(
    position: &position_ledger::PositionRecord,
) -> Option<ManagerPositionRead> {
    let expected_quantity = position.remaining_quantity_raw.parse::<u64>().ok()?;
    let expiry_ms = position.expiry_ms.parse::<u64>().ok()?;

    match position.kind {
        LegKind::Down | LegKind::Up => {
            let strike_raw = position.strike_raw.as_ref()?.parse::<u64>().ok()?;
            Some(ManagerPositionRead::Binary {
                oracle_id: position.oracle_id.clone(),
                expiry_ms,
                strike_raw,
                is_up: matches!(position.kind, LegKind::Up),
                expected_quantity,
            })
        }
        LegKind::Range => {
            let lower_raw = position.lower_raw.as_ref()?.parse::<u64>().ok()?;
            let upper_raw = position.upper_raw.as_ref()?.parse::<u64>().ok()?;
            Some(ManagerPositionRead::Range {
                oracle_id: position.oracle_id.clone(),
                expiry_ms,
                lower_raw,
                upper_raw,
                expected_quantity,
            })
        }
    }
}

/// POST /api/positions/sync-from-audits — rebuild the ledger by re-reading
/// every persisted audit record for this (owner, managerId). Useful when
/// disk state is lost or when an audit was missed (e.g. backend crash
/// between mint and the audit-open call). Idempotent — re-applying the
/// same audit digest is a no-op because apply_mint merges same-key legs
/// only when the source_digest is new, otherwise the merge is the
/// commutative no-op.
async fn sync_positions_from_audits(Json(req): Json<SyncPositionsRequest>) -> impl IntoResponse {
    let mut ledger = PositionLedger::empty(&req.owner, &req.manager_id);
    let mut warnings: Vec<String> = Vec::new();
    let mut applied = 0usize;

    let entries = match storage::list_dir(&storage::audits_dir()) {
        Ok(e) => e,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "ok": false,
                    "error": format!("could not list audits dir: {err}"),
                })),
            );
        }
    };

    for entry in entries {
        if !entry
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.ends_with(".record.json"))
            .unwrap_or(false)
        {
            continue;
        }
        let record: serde_json::Value = match storage::read_json::<serde_json::Value>(&entry) {
            Ok(Some(v)) => v,
            Ok(None) => continue,
            Err(err) => {
                warnings.push(format!(
                    "Skipping malformed audit record {}: {err}",
                    entry.file_name().and_then(|n| n.to_str()).unwrap_or("?")
                ));
                continue;
            }
        };
        let owner_ok =
            record.get("owner").and_then(serde_json::Value::as_str).map(|s| s.to_lowercase())
                == Some(req.owner.to_lowercase());
        let manager_ok =
            record.get("managerId").and_then(serde_json::Value::as_str).map(|s| s.to_lowercase())
                == Some(req.manager_id.to_lowercase());
        if !owner_ok || !manager_ok {
            continue;
        }
        let digest = record.get("digest").and_then(serde_json::Value::as_str).unwrap_or("");
        let oracle_id =
            record.get("oracleId").and_then(serde_json::Value::as_str).unwrap_or("0x0").to_string();
        let expiry_ms =
            record.get("expiryMs").and_then(serde_json::Value::as_str).unwrap_or("0").to_string();
        let strategy =
            record.get("strategy").and_then(serde_json::Value::as_str).map(|s| s.to_string());
        let opened_at =
            record.get("createdAtUnix").and_then(serde_json::Value::as_i64).unwrap_or(0);
        let empty_legs: Vec<serde_json::Value> = Vec::new();
        let legs =
            record.get("mintedLegs").and_then(serde_json::Value::as_array).unwrap_or(&empty_legs);
        for leg in legs {
            if let Some(leg) = minted_leg_from_audit_json(leg, &oracle_id, &expiry_ms, &strategy) {
                ledger.apply_mint(&leg, digest, opened_at);
                applied += 1;
            }
        }
    }

    if let Err(errs) = refresh_position_previews(&req.owner, &req.manager_id, &mut ledger).await {
        warnings.extend(errs);
    }

    if let Err(err) = ledger.save() {
        warnings.push(format!("Could not persist rebuilt ledger: {err}"));
    }

    let summary = ledger.summary();
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "ok": true,
            "owner": ledger.owner,
            "managerId": ledger.manager_id,
            "appliedLegs": applied,
            "positions": ledger.positions,
            "summary": summary,
            "warnings": warnings,
        })),
    )
}

#[derive(Debug, Deserialize)]
struct SyncFromChainRequest {
    owner: String,
    #[serde(rename = "managerId")]
    manager_id: String,
    #[serde(default, rename = "mintedLegs")]
    minted_legs: Vec<SyncMintedLeg>,
    #[serde(default, rename = "redeemedLegs")]
    redeemed_legs: Vec<SyncRedeemedLeg>,
}

#[derive(Debug, Deserialize)]
struct SyncMintedLeg {
    kind: String,
    #[serde(default)]
    direction: Option<String>,
    #[serde(rename = "oracleId")]
    oracle_id: String,
    #[serde(rename = "expiryMs")]
    expiry_ms: String,
    #[serde(default, rename = "strikeRaw")]
    strike_raw: Option<String>,
    #[serde(default, rename = "lowerRaw")]
    lower_raw: Option<String>,
    #[serde(default, rename = "upperRaw")]
    upper_raw: Option<String>,
    #[serde(rename = "quantityRaw")]
    quantity_raw: String,
    #[serde(rename = "costRaw")]
    cost_raw: String,
    #[serde(rename = "sourceDigest")]
    source_digest: String,
    #[serde(default, rename = "openedAtUnix")]
    opened_at_unix: Option<i64>,
    #[serde(default)]
    strategy: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SyncRedeemedLeg {
    kind: String,
    #[serde(rename = "oracleId")]
    oracle_id: String,
    #[serde(rename = "expiryMs")]
    expiry_ms: String,
    #[serde(default, rename = "strikeRaw")]
    strike_raw: Option<String>,
    #[serde(default, rename = "lowerRaw")]
    lower_raw: Option<String>,
    #[serde(default, rename = "upperRaw")]
    upper_raw: Option<String>,
    #[serde(rename = "quantityRaw")]
    quantity_raw: String,
    #[serde(rename = "payoutRaw")]
    payout_raw: String,
    #[serde(rename = "sourceDigest")]
    source_digest: String,
}

fn parse_leg_kind(s: &str) -> Option<LegKind> {
    match s {
        "UP" => Some(LegKind::Up),
        "DOWN" => Some(LegKind::Down),
        "RANGE" => Some(LegKind::Range),
        _ => None,
    }
}

/// POST /api/positions/sync-from-chain
///
/// Accept already-extracted mint/redeem events from the frontend's chain
/// walk and apply them to the ledger. Idempotent: legs whose `sourceDigest`
/// already appears in `audit_digests` / `redeem_digests` are skipped, so a
/// user can re-run the sync without double-counting.
async fn sync_positions_from_chain(Json(req): Json<SyncFromChainRequest>) -> impl IntoResponse {
    let mut ledger = match PositionLedger::load(&req.owner, &req.manager_id) {
        Ok(l) => l,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "ok": false,
                    "error": format!("could not load ledger: {err}"),
                })),
            );
        }
    };

    let mut applied_mints = 0usize;
    let mut applied_redeems = 0usize;
    let mut warnings: Vec<String> = Vec::new();

    // Snapshot the set of digests already applied to this ledger BEFORE the
    // loop. A previously-applied tx-digest is skipped wholesale. We don't
    // mutate this set inside the loop, so when one tx mints multiple legs
    // (different keys, same digest), every leg in the batch still applies.
    //
    // We own the strings (not borrow into `ledger.audit_digests`) so the
    // immutable borrow ends here and `apply_mint` can take `&mut ledger`.
    let known_mint_digests: std::collections::HashSet<String> =
        ledger.audit_digests.iter().cloned().collect();
    let known_redeem_digests: std::collections::HashSet<String> =
        ledger.redeem_digests.iter().cloned().collect();

    for leg in &req.minted_legs {
        if known_mint_digests.contains(&leg.source_digest) {
            continue;
        }
        let Some(kind) = parse_leg_kind(&leg.kind) else {
            warnings.push(format!("Skipping mint leg with unknown kind {}", leg.kind));
            continue;
        };
        let Ok(quantity_raw) = leg.quantity_raw.parse::<u128>() else {
            warnings.push(format!(
                "Skipping mint leg with non-numeric quantityRaw {}",
                leg.quantity_raw
            ));
            continue;
        };
        let cost_raw = leg.cost_raw.parse::<u128>().unwrap_or(0);
        let minted = MintedLeg {
            kind,
            direction: leg.direction.clone(),
            oracle_id: leg.oracle_id.clone(),
            expiry_ms: leg.expiry_ms.clone(),
            strike_raw: leg.strike_raw.clone(),
            lower_raw: leg.lower_raw.clone(),
            upper_raw: leg.upper_raw.clone(),
            quantity_raw,
            cost_raw,
            role: None,
            strategy: leg.strategy.clone(),
        };
        ledger.apply_mint(
            &minted,
            &leg.source_digest,
            leg.opened_at_unix.unwrap_or_else(storage::unix_now),
        );
        applied_mints += 1;
    }

    for leg in &req.redeemed_legs {
        if known_redeem_digests.contains(&leg.source_digest) {
            continue;
        }
        let Some(kind) = parse_leg_kind(&leg.kind) else {
            warnings.push(format!("Skipping redeem leg with unknown kind {}", leg.kind));
            continue;
        };
        let Ok(quantity_raw) = leg.quantity_raw.parse::<u128>() else {
            warnings.push(format!(
                "Skipping redeem leg with non-numeric quantityRaw {}",
                leg.quantity_raw
            ));
            continue;
        };
        let payout_raw = leg.payout_raw.parse::<u128>().unwrap_or(0);
        let redeemed = RedeemedLeg {
            kind,
            oracle_id: leg.oracle_id.clone(),
            expiry_ms: leg.expiry_ms.clone(),
            strike_raw: leg.strike_raw.clone(),
            lower_raw: leg.lower_raw.clone(),
            upper_raw: leg.upper_raw.clone(),
            quantity_raw,
            payout_raw,
        };
        ledger.apply_redeem(&redeemed, &leg.source_digest);
        applied_redeems += 1;
    }

    if let Err(errs) = refresh_position_previews(&req.owner, &req.manager_id, &mut ledger).await {
        warnings.extend(errs);
    }

    if let Err(err) = ledger.save() {
        warnings.push(format!("Could not persist ledger: {err}"));
    }

    let summary = ledger.summary();
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "ok": true,
            "owner": ledger.owner,
            "managerId": ledger.manager_id,
            "appliedMints": applied_mints,
            "appliedRedeems": applied_redeems,
            "positions": ledger.positions,
            "summary": summary,
            "warnings": warnings,
        })),
    )
}

#[derive(Debug, Deserialize)]
struct AuditRedeemPositionRequest {
    owner: String,
    #[serde(rename = "managerId")]
    manager_id: String,
    #[serde(rename = "positionId")]
    position_id: String,
    digest: String,
    #[serde(default)]
    effects: serde_json::Value,
    #[serde(default)]
    events: Vec<serde_json::Value>,
    #[serde(default, rename = "objectChanges")]
    object_changes: Vec<serde_json::Value>,
}

/// POST /api/tx/audit-redeem-position
///
/// Called by the frontend after a wallet-signed close transaction succeeds.
/// Walks the parsed PositionRedeemed / RangeRedeemed events, applies the
/// realized quantity + payout to the position ledger, persists a redeem
/// record for disaster recovery, and returns the updated ledger.
async fn audit_redeem_position(Json(req): Json<AuditRedeemPositionRequest>) -> impl IntoResponse {
    let execution_status = req
        .effects
        .get("status")
        .and_then(|s| s.get("status"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let success = execution_status == "success";

    let mut warnings: Vec<String> = Vec::new();
    let redeemed = parse_redeemed_legs_from_events(&req.events);

    if success {
        let mut ledger = match PositionLedger::load(&req.owner, &req.manager_id) {
            Ok(l) => l,
            Err(err) => {
                warnings.push(format!(
                    "Could not load position ledger before redeem audit: {err}. Starting fresh."
                ));
                PositionLedger::empty(&req.owner, &req.manager_id)
            }
        };
        for leg in &redeemed {
            ledger.apply_redeem(leg, &req.digest);
        }
        if let Err(errs) = refresh_position_previews(&req.owner, &req.manager_id, &mut ledger).await
        {
            warnings.extend(errs);
        }
        if let Err(err) = ledger.save() {
            warnings.push(format!("Could not persist ledger after redeem: {err}"));
        }

        let record = serde_json::json!({
            "schemaVersion": 1,
            "digest": req.digest,
            "owner": req.owner,
            "managerId": req.manager_id,
            "positionId": req.position_id,
            "events": req.events,
            "effects": req.effects,
            "objectChanges": req.object_changes,
            "createdAtUnix": storage::unix_now(),
        });
        if let Err(err) =
            storage::atomic_write_json(&storage::redeem_record_path(&req.digest), &record)
        {
            warnings.push(format!("Could not persist redeem record: {err}"));
        }

        let summary = ledger.summary();
        let updated_position =
            ledger.positions.iter().find(|p| p.position_id == req.position_id).cloned();

        let explorer_url =
            format!("https://suiexplorer.com/txblock/{}?network=testnet", req.digest);

        return (
            StatusCode::OK,
            Json(serde_json::json!({
                "ok": true,
                "digest": req.digest,
                "explorerUrl": explorer_url,
                "executionStatus": execution_status,
                "managerId": req.manager_id,
                "positionId": req.position_id,
                "updatedPosition": updated_position,
                "summary": summary,
                "redeemedLegs": redeemed_legs_to_json(&redeemed),
                "warnings": warnings,
            })),
        );
    }

    let body = serde_json::json!({
        "ok": false,
        "digest": req.digest,
        "executionStatus": execution_status,
        "managerId": req.manager_id,
        "positionId": req.position_id,
        "redeemedLegs": redeemed_legs_to_json(&redeemed),
        "error": req.effects
            .get("status")
            .and_then(|s| s.get("error"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("Transaction did not succeed."),
        "warnings": warnings,
    });
    (StatusCode::BAD_REQUEST, Json(body))
}

/// Walk parsed Sui events and lift PositionRedeemed / RangeRedeemed entries
/// into typed RedeemedLeg structs the ledger understands. Tolerates partial
/// data; missing fields default to None/0 so a malformed event doesn't break
/// the rest of the audit.
fn parse_redeemed_legs_from_events(events: &[serde_json::Value]) -> Vec<RedeemedLeg> {
    let mut out = Vec::new();
    for event in events {
        let event_type = event.get("type").and_then(serde_json::Value::as_str).unwrap_or("");
        let parsed = event.get("parsedJson").cloned().unwrap_or(serde_json::Value::Null);

        let oracle_id = parsed
            .get("oracle_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("0x0")
            .to_string();
        let expiry_ms = parsed
            .get("expiry")
            .and_then(json_value_as_u128_string)
            .unwrap_or_else(|| "0".to_string());
        let quantity_raw = parsed
            .get("quantity")
            .and_then(json_value_as_u128_string)
            .and_then(|s| s.parse::<u128>().ok())
            .unwrap_or(0);
        let payout_raw = parsed
            .get("payout")
            .and_then(json_value_as_u128_string)
            .and_then(|s| s.parse::<u128>().ok())
            .unwrap_or(0);

        if event_type.ends_with("::predict::PositionRedeemed") {
            let is_up = parsed.get("is_up").and_then(serde_json::Value::as_bool).unwrap_or(false);
            let strike_raw = parsed.get("strike").and_then(json_value_as_u128_string);
            out.push(RedeemedLeg {
                kind: if is_up { LegKind::Up } else { LegKind::Down },
                oracle_id,
                expiry_ms,
                strike_raw,
                lower_raw: None,
                upper_raw: None,
                quantity_raw,
                payout_raw,
            });
        } else if event_type.ends_with("::predict::RangeRedeemed") {
            let lower_raw = parsed.get("lower_strike").and_then(json_value_as_u128_string);
            let upper_raw = parsed.get("higher_strike").and_then(json_value_as_u128_string);
            out.push(RedeemedLeg {
                kind: LegKind::Range,
                oracle_id,
                expiry_ms,
                strike_raw: None,
                lower_raw,
                upper_raw,
                quantity_raw,
                payout_raw,
            });
        }
    }
    out
}

fn redeemed_legs_to_json(legs: &[RedeemedLeg]) -> Vec<serde_json::Value> {
    legs.iter()
        .map(|leg| {
            serde_json::json!({
                "kind": match leg.kind {
                    LegKind::Down => "DOWN",
                    LegKind::Up => "UP",
                    LegKind::Range => "RANGE",
                },
                "oracleId": leg.oracle_id,
                "expiryMs": leg.expiry_ms,
                "strikeRaw": leg.strike_raw,
                "lowerRaw": leg.lower_raw,
                "upperRaw": leg.upper_raw,
                "quantityRaw": leg.quantity_raw.to_string(),
                "payoutRaw": leg.payout_raw.to_string(),
            })
        })
        .collect()
}

async fn get_manager_for_address(
    State(state): State<Arc<AppState>>,
    Path(address): Path<String>,
) -> impl IntoResponse {
    let started = Instant::now();
    let key = normalize_address(&address);
    let manager_id = {
        let map = state.managers.lock().await;
        map.get(&key).cloned()
    };
    let elapsed_us = started.elapsed().as_micros();
    println!("managers GET address={} hit={} elapsed_us={}", key, manager_id.is_some(), elapsed_us);

    let body = Json(ManagerLookupResponse { ok: true, address: key, manager_id });
    let headers = [(header::CACHE_CONTROL, "private, max-age=30")];
    (headers, body)
}

/// POST /api/managers/:address — persists the given PredictManager id for
/// this wallet. Body: `{ "managerId": "0x..." }`. Idempotent — re-posting the
/// same id is a no-op. Re-posting a different id overwrites (we trust the
/// caller to send the freshly created/discovered manager).
async fn put_manager_for_address(
    State(state): State<Arc<AppState>>,
    Path(address): Path<String>,
    Json(body): Json<SaveManagerRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let started = Instant::now();
    let key = normalize_address(&address);
    let manager_id = body.manager_id.trim().to_string();
    if !is_sui_hex_id(&address) || !is_sui_hex_id(&manager_id) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "ok": false,
                "error": "address and managerId must be valid 0x-prefixed Sui identifiers"
            })),
        ));
    }

    let prior;
    {
        let mut map = state.managers.lock().await;
        prior = map.insert(key.clone(), manager_id.clone());
        if let Err(err) = save_managers(&state.managers_path, &map) {
            // Roll back the in-memory insert so the in-memory map stays in
            // sync with what's on disk (otherwise a restart would lose the
            // entry and the client would think it was stored).
            match prior {
                Some(ref old) => {
                    map.insert(key.clone(), old.clone());
                }
                None => {
                    map.remove(&key);
                }
            }
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "ok": false,
                    "error": format!("failed to persist managers store: {err}")
                })),
            ));
        }
    }

    let elapsed_us = started.elapsed().as_micros();
    println!(
        "managers POST address={} manager_id={} replaced_prior={} elapsed_us={}",
        key,
        manager_id,
        prior.is_some(),
        elapsed_us
    );

    // Explicitly tell the browser not to cache the POST response. The GET
    // cache is invalidated client-side by the seed() call in lib/api.ts.
    let body = Json(ManagerLookupResponse { ok: true, address: key, manager_id: Some(manager_id) });
    let headers = [(header::CACHE_CONTROL, "no-store")];
    Ok((headers, body))
}

async fn refresh_compiled_strategy(
    _state: &AppState,
    cached_compiled: &serde_json::Value,
    owner: &str,
    slippage_bps: u16,
    exclude_oracle_ids: &[String],
) -> Result<serde_json::Value, (StatusCode, serde_json::Value)> {
    if matches!(
        cached_compiled.get("selectionMode").and_then(serde_json::Value::as_str),
        Some("mintable_probe" | "best_effort_scaled" | "preset_filtered")
    ) {
        return Ok(cached_compiled.clone());
    }

    let strategy = cached_compiled
        .get("strategy")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("BREAKOUT_PROTECTION");
    let style =
        cached_compiled.get("style").and_then(serde_json::Value::as_str).unwrap_or("balanced");
    let budget_raw = cached_compiled
        .get("budgetRaw")
        .and_then(serde_json::Value::as_str)
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                serde_json::json!({
                    "ok": false,
                    "error": "cached compiled strategy is missing budgetRaw"
                }),
            )
        })?;

    let mut args = service::CompileStrategyJsonArgs {
        server_url: env::var("STRUCTX_PREDICT_SERVER_URL")
            .unwrap_or_else(|_| PREDICT_SERVER_URL.to_string()),
        predict_id: env::var("STRUCTX_PREDICT_ID")
            .unwrap_or_else(|_| PREDICT_OBJECT_ID.to_string()),
        rpc_url: env::var("STRUCTX_RPC_URL")
            .unwrap_or_else(|_| DEFAULT_SUI_TESTNET_RPC_URL.to_string()),
        owner: owner.to_string(),
        strategy: strategy.to_string(),
        budget_dusdc: dusdc_raw_to_decimal_string(budget_raw),
        style: style.to_string(),
        expiry_preference: "nearest_active".to_string(),
        slippage_bps,
        bucket_step: DisplayPrice(250.0),
        custom_k1_price: None,
        custom_k2_price: None,
        custom_k3_price: None,
        custom_k4_price: None,
        levels_each_side: 4,
        max_quote_market_attempts: 5,
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
        exclude_oracle_ids: exclude_oracle_ids.to_vec(),
    };

    if let Some(advanced) = cached_compiled.get("advanced") {
        if let Some(value) =
            advanced.get("portfolioExposureDUSDC").and_then(serde_json::Value::as_f64)
        {
            args.portfolio_exposure_dusdc = value;
        }

        if let Some(value) = advanced.get("overHedgeCapBps").and_then(serde_json::Value::as_u64) {
            args.over_hedge_cap_bps = value as u16;
        }

        if let Some(value) = advanced.get("deadZoneBps").and_then(serde_json::Value::as_u64) {
            args.dead_zone_bps = value as u16;
        }

        if let Some(value) = advanced.get("convexGammaBps").and_then(serde_json::Value::as_u64) {
            args.convex_gamma_bps = value as u16;
        }

        if let Some(value) =
            advanced.get("moonshotRangeWeightBps").and_then(serde_json::Value::as_u64)
        {
            args.moonshot_range_weight_bps = value as u16;
        }

        if let Some(value) =
            advanced.get("moonshotTailGammaBps").and_then(serde_json::Value::as_u64)
        {
            args.moonshot_tail_gamma_bps = value as u16;
        }

        if let Some(value) =
            advanced.get("downsideRangeWeightBps").and_then(serde_json::Value::as_u64)
        {
            args.downside_range_weight_bps = value as u16;
        }

        if let Some(value) =
            advanced.get("downsideTailGammaBps").and_then(serde_json::Value::as_u64)
        {
            args.downside_tail_gamma_bps = value as u16;
        }

        if let Some(value) =
            advanced.get("upsideNearRangeWeightBps").and_then(serde_json::Value::as_u64)
        {
            args.upside_near_range_weight_bps = value as u16;
        }

        if let Some(value) =
            advanced.get("upsideUpperRangeWeightBps").and_then(serde_json::Value::as_u64)
        {
            args.upside_upper_range_weight_bps = value as u16;
        }

        if let Some(value) = advanced.get("upsideTailGammaBps").and_then(serde_json::Value::as_u64)
        {
            args.upside_tail_gamma_bps = value as u16;
        }

        if let Some(value) =
            advanced.get("downsideNearRangeWeightBps").and_then(serde_json::Value::as_u64)
        {
            args.downside_near_range_weight_bps = value as u16;
        }

        if let Some(value) =
            advanced.get("downsideLowerRangeWeightBps").and_then(serde_json::Value::as_u64)
        {
            args.downside_lower_range_weight_bps = value as u16;
        }

        if let Some(value) =
            advanced.get("downsideStepTailGammaBps").and_then(serde_json::Value::as_u64)
        {
            args.downside_step_tail_gamma_bps = value as u16;
        }

        if let Some(value) =
            advanced.get("condorCenterWeightBps").and_then(serde_json::Value::as_u64)
        {
            args.condor_center_weight_bps = value as u16;
        }
    }

    service::compile_strategy_json_value(args).await.map_err(|err| {
        (
            StatusCode::BAD_REQUEST,
            serde_json::json!({
                "ok": false,
                "error": err.to_string(),
            }),
        )
    })
}

async fn assert_compiled_plan_mintable(
    owner: &str,
    manager_id: &str,
    compiled: &serde_json::Value,
    expiry_ms: &str,
) -> Result<(), (StatusCode, serde_json::Value)> {
    let plan = compiled_quote_plan(compiled, expiry_ms)?;
    let rpc =
        SuiRpcClient::new(DEFAULT_SUI_TESTNET_RPC_URL, Duration::from_secs(20)).map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({
                    "ok": false,
                    "error": format!("failed to initialize Sui RPC client: {err}")
                }),
            )
        })?;

    let predict = resolve_sui_object(&rpc, PREDICT_OBJECT_ID).await?;
    let manager = resolve_sui_object(&rpc, manager_id).await?;
    let oracle = resolve_sui_object(
        &rpc,
        compiled.get("oracleId").and_then(serde_json::Value::as_str).unwrap_or_default(),
    )
    .await?;
    let clock = resolve_sui_object(&rpc, CLOCK_OBJECT_ID).await?;

    validate_predict_manager_object(&manager)?;
    validate_quote_object_refs(&predict, &oracle, &clock)?;

    let tx_kind = build_mint_tx_kind(
        &plan,
        MintObjectRefs { predict: &predict, manager: &manager, oracle: &oracle, clock: &clock },
        owner,
    )
    .map_err(|err| {
        (
            StatusCode::BAD_REQUEST,
            serde_json::json!({
                "ok": false,
                "error": format!("failed to build mint transaction: {err}")
            }),
        )
    })?;

    let response = rpc
        .dev_inspect_transaction_kind(&tx_kind.sender, &tx_kind.tx_kind_b64)
        .await
        .map_err(|err| {
            (
                StatusCode::BAD_REQUEST,
                serde_json::json!({
                    "ok": false,
                    "error": format!("mint preflight RPC failed: {err}")
                }),
            )
        })?;

    let status = response
        .get("effects")
        .and_then(|effects| effects.get("status"))
        .and_then(|status| status.get("status"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");

    if status != "success" {
        return Err((
            StatusCode::BAD_REQUEST,
            serde_json::json!({
                "ok": false,
                "error": devinspect_failure_summary(&response),
                "details": response
            }),
        ));
    }

    Ok(())
}

async fn find_best_effort_mintable_compiled(
    owner: &str,
    manager_id: &str,
    compiled: &serde_json::Value,
    expiry_ms: &str,
    allow_manager_funding: bool,
) -> Result<(serde_json::Value, Vec<String>), (StatusCode, serde_json::Value)> {
    match assert_compiled_plan_mintable(owner, manager_id, compiled, expiry_ms).await {
        Ok(()) => return Ok((compiled.clone(), Vec::new())),
        Err((_, value)) if allow_manager_funding && is_balance_manager_funding_failure(&value) => {
            return Ok((
                compiled.clone(),
                vec![
                    "The selected PredictManager needs funding. The wallet will add the required dUSDC in the same transaction before the positions are opened."
                        .to_string(),
                ],
            ));
        }
        Err((status, value)) if !is_mintable_ask_failure(&value) => {
            return Err((status, value));
        }
        Err(_) => {}
    }

    let scale_bps_ladder = [
        9500u16, 9000, 8500, 8000, 7500, 7000, 6500, 6000, 5500, 5000, 4500, 4000, 3500, 3000,
        2500, 2000, 1500, 1000, 750, 500, 250, 100, 50, 25, 10, 5, 1,
    ];

    let mut last_error: Option<(StatusCode, serde_json::Value)> = None;

    for scale_bps in scale_bps_ladder {
        let adjusted = scale_compiled_quantities(compiled, scale_bps)?;

        match assert_compiled_plan_mintable(owner, manager_id, &adjusted, expiry_ms).await {
            Ok(()) => {
                return Ok((
                    adjusted,
                    vec![format!(
                        "The live market could support {}% of the previewed size, so StructX reduced each position before preparing the transaction.",
                        format_bps_percent(scale_bps)
                    )],
                ));
            }
            Err((status, value)) if is_mintable_ask_failure(&value) => {
                last_error = Some((status, value));
            }
            Err(err) => return Err(err),
        }
    }

    if let Some((_, value)) = last_error {
        return Err((
            StatusCode::BAD_REQUEST,
            serde_json::json!({
                "ok": false,
                "error": format!(
                    "No mintable size was found for this compiled strategy, even after shrinking the live quantities. Last failure: {}",
                    value
                        .get("error")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("unknown mintability error")
                ),
                "details": value
            }),
        ));
    }

    Err((
        StatusCode::BAD_REQUEST,
        serde_json::json!({
            "ok": false,
            "error": "No mintable size was found for this compiled strategy."
        }),
    ))
}

fn compiled_quote_plan(
    compiled: &serde_json::Value,
    expiry_ms: &str,
) -> Result<QuotePlan, (StatusCode, serde_json::Value)> {
    let oracle_id = compiled
        .get("oracleId")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                serde_json::json!({
                    "ok": false,
                    "error": "compiled strategy is missing oracleId"
                }),
            )
        })?
        .to_string();

    let expiry_ms = expiry_ms.parse::<i64>().map_err(|err| {
        (
            StatusCode::BAD_REQUEST,
            serde_json::json!({
                "ok": false,
                "error": format!("invalid expiryMs in compiled strategy: {err}")
            }),
        )
    })?;

    let legs = compiled.get("legs").and_then(serde_json::Value::as_array).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            serde_json::json!({
                "ok": false,
                "error": "compiled strategy is missing legs"
            }),
        )
    })?;

    let mut calls = Vec::with_capacity(legs.len());
    let mut max_payout_quantity = 0u64;

    for leg in legs {
        let kind = leg.get("kind").and_then(serde_json::Value::as_str).unwrap_or("");
        let quantity = json_u64_string(leg, "quantityRaw")?;
        max_payout_quantity = max_payout_quantity.max(quantity);

        match kind {
            "DOWN" => {
                calls.push(QuoteCall::Binary {
                    function: QuoteFunction::GetTradeAmounts,
                    oracle_id: oracle_id.clone(),
                    expiry_ms,
                    direction: BinaryDirection::Down,
                    strike: Strike { raw: json_u64_string(leg, "strikeRaw")? },
                    quantity,
                });
            }
            "UP" => {
                calls.push(QuoteCall::Binary {
                    function: QuoteFunction::GetTradeAmounts,
                    oracle_id: oracle_id.clone(),
                    expiry_ms,
                    direction: BinaryDirection::Up,
                    strike: Strike { raw: json_u64_string(leg, "strikeRaw")? },
                    quantity,
                });
            }
            "RANGE" => {
                calls.push(QuoteCall::Range {
                    function: QuoteFunction::GetRangeTradeAmounts,
                    oracle_id: oracle_id.clone(),
                    expiry_ms,
                    lower: Strike { raw: json_u64_string(leg, "lowerRaw")? },
                    upper: Strike { raw: json_u64_string(leg, "upperRaw")? },
                    quantity,
                });
            }
            other => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    serde_json::json!({
                        "ok": false,
                        "error": format!("unsupported compiled leg kind: {other}")
                    }),
                ));
            }
        }
    }

    Ok(QuotePlan {
        target: QuoteTarget::default(),
        oracle_id,
        expiry_ms,
        calls,
        max_payout_quantity,
    })
}

fn json_u64_string(
    value: &serde_json::Value,
    key: &str,
) -> Result<u64, (StatusCode, serde_json::Value)> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                serde_json::json!({
                    "ok": false,
                    "error": format!("compiled strategy is missing {key}")
                }),
            )
        })?
        .parse::<u64>()
        .map_err(|err| {
            (
                StatusCode::BAD_REQUEST,
                serde_json::json!({
                    "ok": false,
                    "error": format!("invalid {key}: {err}")
                }),
            )
        })
}

fn ceil_mul_bps(raw: &str, bps: u16) -> String {
    let value = raw.parse::<u128>().unwrap_or(0);
    let multiplier = 10_000u128 + bps as u128;

    let out = value.saturating_mul(multiplier).saturating_add(9_999) / 10_000;

    out.to_string()
}

fn scale_compiled_quantities(
    compiled: &serde_json::Value,
    scale_bps: u16,
) -> Result<serde_json::Value, (StatusCode, serde_json::Value)> {
    let mut adjusted = compiled.clone();
    let original_id = compiled
        .get("compiledStrategyId")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("compiled");
    let selection_mode = adjusted.as_object_mut().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            serde_json::json!({
                "ok": false,
                "error": "compiled strategy must be a JSON object"
            }),
        )
    })?;

    if let Some(legs) = selection_mode.get_mut("legs").and_then(serde_json::Value::as_array_mut) {
        for leg in legs {
            scale_compiled_leg(leg, scale_bps)?;
        }
    }

    scale_compiled_amount_field(selection_mode, "budgetRaw", "budgetDisplay", scale_bps);
    scale_compiled_amount_field(
        selection_mode,
        "premiumRequiredRaw",
        "premiumRequiredDisplay",
        scale_bps,
    );
    scale_compiled_amount_field(selection_mode, "maxLossRaw", "maxLossDisplay", scale_bps);
    scale_compiled_amount_field(
        selection_mode,
        "maxGrossPayoutRaw",
        "maxGrossPayoutDisplay",
        scale_bps,
    );
    scale_compiled_amount_field(
        selection_mode,
        "maxNetPayoutRaw",
        "maxNetPayoutDisplay",
        scale_bps,
    );

    selection_mode.insert(
        "compiledStrategyId".to_string(),
        serde_json::Value::String(format!("{original_id}:best_effort_{scale_bps}")),
    );
    selection_mode.insert(
        "selectionMode".to_string(),
        serde_json::Value::String("best_effort_scaled".to_string()),
    );
    selection_mode.insert(
        "bestEffortScaleBps".to_string(),
        serde_json::Value::Number(serde_json::Number::from(scale_bps)),
    );

    Ok(adjusted)
}

fn scale_compiled_leg(
    leg: &mut serde_json::Value,
    scale_bps: u16,
) -> Result<(), (StatusCode, serde_json::Value)> {
    let obj = leg.as_object_mut().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            serde_json::json!({
                "ok": false,
                "error": "compiled leg must be a JSON object"
            }),
        )
    })?;

    let quantity_raw = obj
        .get("quantityRaw")
        .and_then(serde_json::Value::as_str)
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    let scaled_quantity = scale_nonzero_u64(quantity_raw, scale_bps);
    obj.insert("quantityRaw".to_string(), serde_json::Value::String(scaled_quantity.to_string()));
    obj.insert(
        "quantityDisplay".to_string(),
        serde_json::Value::String(scaled_quantity.to_string()),
    );

    if let Some(premium_raw) = obj
        .get("premiumRaw")
        .and_then(serde_json::Value::as_str)
        .and_then(|value| value.parse::<u64>().ok())
    {
        let scaled_premium = scale_nonzero_u64(premium_raw, scale_bps);
        obj.insert("premiumRaw".to_string(), serde_json::Value::String(scaled_premium.to_string()));
        obj.insert(
            "premiumDisplay".to_string(),
            serde_json::Value::String(format_dusdc_raw(scaled_premium)),
        );
    }

    Ok(())
}

fn scale_compiled_amount_field(
    compiled: &mut serde_json::Map<String, serde_json::Value>,
    raw_key: &str,
    display_key: &str,
    scale_bps: u16,
) {
    let Some(raw_value) = compiled
        .get(raw_key)
        .and_then(serde_json::Value::as_str)
        .and_then(|value| value.parse::<u64>().ok())
    else {
        return;
    };

    let scaled = scale_nonzero_u64(raw_value, scale_bps);
    compiled.insert(raw_key.to_string(), serde_json::Value::String(scaled.to_string()));
    compiled.insert(display_key.to_string(), serde_json::Value::String(format_dusdc_raw(scaled)));
}

fn scale_nonzero_u64(value: u64, scale_bps: u16) -> u64 {
    if value == 0 {
        return 0;
    }

    let scaled = ((value as u128) * (scale_bps as u128)) / 10_000u128;
    scaled.max(1).min(u64::MAX as u128) as u64
}

fn is_mintable_ask_failure(value: &serde_json::Value) -> bool {
    value
        .get("error")
        .and_then(serde_json::Value::as_str)
        .map(|message| {
            message.contains("assert_mintable_ask")
                || message.contains("EAskPriceOutOfBounds")
                || message.contains("code 7")
        })
        .unwrap_or(false)
}

fn is_balance_manager_funding_failure(value: &serde_json::Value) -> bool {
    let error_matches = value
        .get("error")
        .and_then(serde_json::Value::as_str)
        .map(|message| {
            let message = message.to_ascii_lowercase();
            (message.contains("balance_manager")
                && message.contains("withdraw_with_proof")
                && message.contains("code 3"))
                || message.contains("ebalancemanagerbalancetoolow")
        })
        .unwrap_or(false);

    let abort = value
        .get("details")
        .and_then(|details| details.get("effects"))
        .and_then(|effects| effects.get("abortError"));
    let structured_matches = abort
        .map(|abort| {
            let module = abort
                .get("module_id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_ascii_lowercase();
            let function = abort
                .get("function")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_ascii_lowercase();
            let code = abort.get("error_code").and_then(serde_json::Value::as_u64);
            module.contains("balance_manager")
                && function == "withdraw_with_proof"
                && code == Some(3)
        })
        .unwrap_or(false);

    error_matches || structured_matches
}

fn format_bps_percent(scale_bps: u16) -> String {
    let whole = scale_bps / 100;
    let frac = scale_bps % 100;

    if frac == 0 {
        whole.to_string()
    } else {
        format!("{whole}.{frac:02}").trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn compiled_expiry_ms(compiled_strategy_id: &str) -> Option<String> {
    compiled_strategy_id.split(':').nth(3).map(ToOwned::to_owned)
}

fn legs_with_max_costs(legs: Vec<serde_json::Value>, slippage_bps: u16) -> Vec<serde_json::Value> {
    legs.into_iter()
        .map(|mut leg| {
            if let Some(obj) = leg.as_object_mut() {
                let premium_raw =
                    obj.get("premiumRaw").and_then(serde_json::Value::as_str).unwrap_or("0");

                obj.insert(
                    "maxCostRaw".to_string(),
                    serde_json::Value::String(ceil_mul_bps(premium_raw, slippage_bps)),
                );
            }

            leg
        })
        .collect()
}

fn format_dusdc_raw(raw: u64) -> String {
    let whole = raw / 1_000_000;
    let frac = raw % 1_000_000;

    if frac == 0 {
        format!("{whole}.00 dUSDC")
    } else {
        let mut frac_string = format!("{frac:06}");
        while frac_string.ends_with('0') {
            frac_string.pop();
        }
        format!("{whole}.{frac_string} dUSDC")
    }
}

fn dusdc_raw_to_decimal_string(raw: u64) -> String {
    let whole = raw / 1_000_000;
    let frac = raw % 1_000_000;

    if frac == 0 {
        whole.to_string()
    } else {
        let mut frac_string = format!("{frac:06}");

        while frac_string.ends_with('0') {
            frac_string.pop();
        }

        format!("{whole}.{frac_string}")
    }
}

async fn resolve_sui_object(
    rpc: &SuiRpcClient,
    object_id: &str,
) -> Result<SuiObjectInfo, (StatusCode, serde_json::Value)> {
    let value = rpc.get_object(object_id).await.map_err(|err| {
        (
            StatusCode::BAD_REQUEST,
            serde_json::json!({
                "ok": false,
                "error": format!("failed to fetch object {object_id}: {err}")
            }),
        )
    })?;

    SuiObjectInfo::from_get_object_result(object_id, value).map_err(|err| {
        (
            StatusCode::BAD_REQUEST,
            serde_json::json!({
                "ok": false,
                "error": format!("failed to parse object {object_id}: {err}")
            }),
        )
    })
}

fn validate_quote_object_refs(
    predict: &SuiObjectInfo,
    oracle: &SuiObjectInfo,
    clock: &SuiObjectInfo,
) -> Result<(), (StatusCode, serde_json::Value)> {
    for (role, object) in [("predict", predict), ("oracle", oracle), ("clock", clock)] {
        if object.owner_kind != ObjectOwnerKind::Shared {
            return Err((
                StatusCode::BAD_REQUEST,
                serde_json::json!({
                    "ok": false,
                    "error": format!("{role} object is not shared: owner={}", object.owner_kind)
                }),
            ));
        }

        if object.initial_shared_version.is_none() {
            return Err((
                StatusCode::BAD_REQUEST,
                serde_json::json!({
                    "ok": false,
                    "error": format!("{role} object is missing initial_shared_version")
                }),
            ));
        }
    }

    Ok(())
}

fn validate_predict_manager_object(
    manager: &SuiObjectInfo,
) -> Result<(), (StatusCode, serde_json::Value)> {
    if manager.owner_kind != ObjectOwnerKind::Shared {
        return Err((
            StatusCode::BAD_REQUEST,
            serde_json::json!({
                "ok": false,
                "error": format!("PredictManager is not shared: owner={}", manager.owner_kind)
            }),
        ));
    }

    if manager.initial_shared_version.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            serde_json::json!({
                "ok": false,
                "error": "PredictManager is missing initial_shared_version"
            }),
        ));
    }

    let actual_type = manager.object_type.as_deref().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            serde_json::json!({
                "ok": false,
                "error": "PredictManager object is missing type"
            }),
        )
    })?;

    if actual_type != PREDICT_MANAGER_TYPE {
        return Err((
            StatusCode::BAD_REQUEST,
            serde_json::json!({
                "ok": false,
                "error": format!(
                    "unexpected PredictManager type: expected {}, got {}",
                    PREDICT_MANAGER_TYPE, actual_type
                )
            }),
        ));
    }

    Ok(())
}

fn devinspect_failure_summary(response: &serde_json::Value) -> String {
    let status_error = response
        .get("effects")
        .and_then(|effects| effects.get("status"))
        .and_then(|status| status.get("error"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown status error");

    let abort_module = response
        .get("effects")
        .and_then(|effects| effects.get("abortError"))
        .and_then(|abort| abort.get("module_id"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown module");

    let abort_function = response
        .get("effects")
        .and_then(|effects| effects.get("abortError"))
        .and_then(|abort| abort.get("function"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown function");

    let abort_code = response
        .get("effects")
        .and_then(|effects| effects.get("abortError"))
        .and_then(|abort| abort.get("error_code"))
        .and_then(serde_json::Value::as_u64)
        .map(|code| code.to_string())
        .unwrap_or_else(|| "unknown".to_string());

    format!(
        "devInspect failed: {status_error}; abort={abort_module}::{abort_function} code {abort_code}"
    )
}

fn enrich_mintability_error(
    mut value: serde_json::Value,
    attempted_oracle_ids: &[String],
) -> serde_json::Value {
    if let Some(obj) = value.as_object_mut() {
        if let Some(error) = obj.get("error").and_then(serde_json::Value::as_str) {
            if error.contains("assert_mintable_ask") {
                let attempted = if attempted_oracle_ids.is_empty() {
                    "none".to_string()
                } else {
                    attempted_oracle_ids.join(", ")
                };

                obj.insert(
                    "error".to_string(),
                    serde_json::Value::String(format!(
                        "No currently mintable breakout candidate was found. Tried oracle candidates: {attempted}. Last failure: {error}"
                    )),
                );
                obj.insert(
                    "attemptedOracleIds".to_string(),
                    serde_json::Value::Array(
                        attempted_oracle_ids
                            .iter()
                            .cloned()
                            .map(serde_json::Value::String)
                            .collect(),
                    ),
                );
            }
        }
    }

    value
}

async fn demo_status(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<DemoStatusRequest>,
) -> impl IntoResponse {
    match service::position_service::demo_status_json_value(
        Some(DEFAULT_SUI_TESTNET_RPC_URL.to_string()),
        &req.manager_id,
        &req.sender,
        std::path::Path::new(&req.from_execution_json),
        false,
    )
    .await
    {
        Ok(value) => (
            StatusCode::OK,
            Json(CliResponse {
                ok: true,
                code: Some(0),
                stdout: serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string()),
                stderr: String::new(),
            }),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(CliResponse {
                ok: false,
                code: Some(1),
                stdout: String::new(),
                stderr: err.to_string(),
            }),
        ),
    }
}

async fn manager_balance(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<ManagerBalanceRequest>,
) -> impl IntoResponse {
    match service::manager_balance_json_value(
        DEFAULT_SUI_TESTNET_RPC_URL.to_string(),
        req.manager_id,
        "0x0000000000000000000000000000000000000000000000000000000000000000".to_string(),
    )
    .await
    {
        Ok(value) => (
            StatusCode::OK,
            Json(CliResponse {
                ok: true,
                code: Some(0),
                stdout: serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string()),
                stderr: String::new(),
            }),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(CliResponse {
                ok: false,
                code: Some(1),
                stdout: String::new(),
                stderr: err.to_string(),
            }),
        ),
    }
}

async fn manager_balance_json(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<ManagerBalanceRequest>,
) -> impl IntoResponse {
    match service::manager_balance_json_value(
        DEFAULT_SUI_TESTNET_RPC_URL.to_string(),
        req.manager_id,
        "0x0000000000000000000000000000000000000000000000000000000000000000".to_string(),
    )
    .await
    {
        Ok(value) => (StatusCode::OK, Json(value)),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "ok": false,
                "error": err.to_string(),
            })),
        ),
    }
}

async fn manager_positions(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<ManagerPositionsRequest>,
) -> impl IntoResponse {
    match service::position_service::manager_positions_json_value(
        Some(DEFAULT_SUI_TESTNET_RPC_URL.to_string()),
        &req.manager_id,
        std::path::Path::new(&req.from_execution_json),
        &req.sender,
        false,
    )
    .await
    {
        Ok(value) => (
            StatusCode::OK,
            Json(CliResponse {
                ok: true,
                code: Some(0),
                stdout: serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string()),
                stderr: String::new(),
            }),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(CliResponse {
                ok: false,
                code: Some(1),
                stdout: String::new(),
                stderr: err.to_string(),
            }),
        ),
    }
}

async fn audit_execution(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<AuditExecutionRequest>,
) -> impl IntoResponse {
    match service::audit_service::audit_execution_json_value(std::path::Path::new(
        &req.from_execution_json,
    )) {
        Ok(value) => (
            StatusCode::OK,
            Json(CliResponse {
                ok: true,
                code: Some(0),
                stdout: serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string()),
                stderr: String::new(),
            }),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(CliResponse {
                ok: false,
                code: Some(1),
                stdout: String::new(),
                stderr: err.to_string(),
            }),
        ),
    }
}

async fn devinspect_mint_breakout(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<DevinspectMintBreakoutRequest>,
) -> impl IntoResponse {
    match service::devinspect_service::devinspect_mint_breakout_json_value(
        service::devinspect_service::DevinspectMintBreakoutJsonArgs {
            server_url: PREDICT_SERVER_URL.to_string(),
            predict_id: PREDICT_OBJECT_ID.to_string(),
            rpc_url: DEFAULT_SUI_TESTNET_RPC_URL.to_string(),
            manager_id: req.manager_id,
            sender: req.sender,
            max_total_mint_cost_raw: req.max_total_mint_cost_raw,
            slippage_bps: req.slippage_bps,
            max_quote_market_attempts: req.max_quote_market_attempts,
            write_execute_script: req.write_execute_script,
        },
    )
    .await
    {
        Ok(value) => (
            StatusCode::OK,
            Json(CliResponse {
                ok: true,
                code: Some(0),
                stdout: serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string()),
                stderr: String::new(),
            }),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(CliResponse {
                ok: false,
                code: Some(1),
                stdout: String::new(),
                stderr: err.to_string(),
            }),
        ),
    }
}

async fn devinspect_redeem_breakout(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<DevinspectRedeemBreakoutRequest>,
) -> impl IntoResponse {
    match service::devinspect_service::devinspect_redeem_breakout_json_value(
        service::devinspect_service::DevinspectRedeemBreakoutJsonArgs {
            rpc_url: DEFAULT_SUI_TESTNET_RPC_URL.to_string(),
            manager_id: req.manager_id,
            sender: req.sender,
            from_execution_json: PathBuf::from(req.from_execution_json),
            auto_size_down: req.auto_size_down,
            write_execute_script: req.write_execute_script,
            allow_zero_payout_script: req.allow_zero_payout_script,
        },
    )
    .await
    {
        Ok(value) => (
            StatusCode::OK,
            Json(CliResponse {
                ok: true,
                code: Some(0),
                stdout: serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string()),
                stderr: String::new(),
            }),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(CliResponse {
                ok: false,
                code: Some(1),
                stdout: String::new(),
                stderr: err.to_string(),
            }),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{is_balance_manager_funding_failure, is_sui_hex_id};

    #[test]
    fn recognizes_structured_manager_funding_abort() {
        let value = serde_json::json!({
            "error": "devInspect failed",
            "details": {
                "effects": {
                    "abortError": {
                        "module_id": "0x74cd::balance_manager",
                        "function": "withdraw_with_proof",
                        "error_code": 3
                    }
                }
            }
        });

        assert!(is_balance_manager_funding_failure(&value));
    }

    #[test]
    fn rejects_unrelated_code_three_abort() {
        let value = serde_json::json!({
            "error": "devInspect failed: abort=0x1::other::withdraw_with_proof code 3"
        });

        assert!(!is_balance_manager_funding_failure(&value));
    }

    #[test]
    fn validates_sui_identifiers_used_by_manager_storage() {
        assert!(is_sui_hex_id("0x55a0"));
        assert!(is_sui_hex_id(&format!("0x{}", "a".repeat(64))));
        assert!(!is_sui_hex_id("../../manager"));
        assert!(!is_sui_hex_id("0xzz"));
        assert!(!is_sui_hex_id(&format!("0x{}", "a".repeat(65))));
    }
}
