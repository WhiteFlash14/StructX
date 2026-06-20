#[allow(dead_code)]
mod intent_audit;
mod intent_positions;
#[allow(dead_code)]
mod open_execution_audit;
mod position_ledger;
#[allow(dead_code)]
mod proposal_store;
#[allow(dead_code)]
mod storage;

use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use deepbook_client::{
    DeepBookClient, DeepBookConfig, FreshnessConfig, MarketSnapshot, ObjectOwnerKind,
    SuiObjectInfo, SuiRpcClient, DEFAULT_SUI_TESTNET_RPC_URL, PREDICT_MANAGER_TYPE,
};
use intent_audit::DiskIntentAuditStore;
use intent_positions::list_intent_positions;
use open_execution_audit::minted_leg_from_audit_json;
use position_ledger::{premium_basis_for_slice, LegKind, MintedLeg, PositionLedger, RedeemedLeg};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    env, fs,
    net::SocketAddr,
    path::{Path as StdPath, PathBuf},
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use structx_core::{build_redeem_tx_kind, ManagerPositionRead, MintObjectRefs};
use structx_service::{
    load_catalog_status, plan_from_intent, refresh_catalog_from_existing_markets_json,
    DiskMarketStore, ExpiryPreference, MarketCategory, MarketKind, MarketSearchQuery, MarketStatus,
    MarketStore, RiskStyle, UserIntentRequest,
};
use tokio::{process::Command, sync::Mutex};
use tower_http::cors::CorsLayer;

const DEFAULT_PREDICT_SERVER_URL: &str = "https://predict-server.testnet.mystenlabs.com";

const PREDICT_PACKAGE_ID: &str =
    "0xf5ea2b3749c65d6e56507cc35388719aadb28f9cab873696a2f8687f5c785138";
const PREDICT_OBJECT_ID: &str =
    "0xc8736204d12f0a7277c86388a68bf8a194b0a14c5538ad13f22cbd8e2a38028a";
const CLOCK_OBJECT_ID: &str = "0x6";
const DUSDC_COIN_TYPE: &str =
    "0xe95040085976bfd54a1a07225cd46c8a2b4e8e2b6732f140a0fc49850ba73e1a::dusdc::DUSDC";

#[derive(Debug, Clone)]
struct AppState {
    cli_bin: PathBuf,
    compiled: Arc<Mutex<HashMap<String, serde_json::Value>>>,
    audits_dir: PathBuf,
    audit_lock: Arc<Mutex<()>>,
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

#[derive(Debug, Serialize)]
struct HealthResponse {
    ok: bool,
    service: &'static str,
    cli_bin: String,
}

#[derive(Debug, Serialize)]
struct CliResponse {
    ok: bool,
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

#[derive(Debug, Deserialize)]
struct LivePriceQuery {
    #[serde(default = "default_live_asset")]
    asset: String,

    #[serde(rename = "oracleId")]
    oracle_id: Option<String>,
}

fn default_live_asset() -> String {
    "BTC".to_string()
}

#[derive(Debug, Serialize)]
struct LivePriceResponse {
    ok: bool,
    asset: String,
    source: String,
    price_raw: Option<String>,
    price: Option<String>,
    updated_at_ms: Option<u64>,
    stale: bool,
    oracle_id: Option<String>,
    raw: serde_json::Value,
    warnings: Vec<String>,
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
struct RecentIntentAuditsQuery {
    max: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct IntentPositionsQuery {
    user_address: Option<String>,
    max: Option<usize>,
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
    effects: serde_json::Value,
    events: Vec<serde_json::Value>,
    #[serde(rename = "objectChanges")]
    object_changes: Vec<serde_json::Value>,
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

    let cli_bin = env::var("STRUCTX_CLI_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("target/debug/structx-cli"));

    let audits_dir = env::var("STRUCTX_AUDITS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("artifacts/audits"));

    fs::create_dir_all(&audits_dir).expect("create audits dir");

    let managers_path = env::var("STRUCTX_MANAGERS_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("data/managers.json"));
    let managers_initial = load_managers(&managers_path);
    println!(
        "Loaded {} stored PredictManager(s) from {}",
        managers_initial.len(),
        managers_path.display()
    );

    let state = Arc::new(AppState {
        cli_bin,
        compiled: Arc::new(Mutex::new(HashMap::new())),
        audits_dir,
        audit_lock: Arc::new(Mutex::new(())),
        managers: Arc::new(Mutex::new(managers_initial)),
        managers_path,
        markets_refresh: Arc::new(Mutex::new(MarketsRefreshState::default())),
    });

    spawn_markets_cache_warmer(state.clone());

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/market/live-price", get(live_price))
        .route("/api/intent/plan", post(plan_intent))
        .route("/api/intent/audits/recent", get(list_recent_intent_audits))
        .route("/api/intent/audits/proposal/{proposal_id}", get(get_intent_audit_by_proposal))
        .route("/api/intent/audits/digest/{digest}", get(get_intent_audit_by_digest))
        .route("/api/intent/positions", get(list_intent_position_overlays))
        .route("/api/intent/parse", post(parse_intent))
        .route("/api/markets/catalog/status", get(get_market_catalog_status))
        .route("/api/markets/catalog/refresh", post(refresh_market_catalog))
        .route("/api/markets/search", get(search_market_catalog))
        .route("/api/markets/catalog/{market_id}", get(get_catalog_market))
        .route("/api/markets", get(list_markets))
        .route("/api/positions", get(list_positions))
        .route("/api/positions/sync-from-audits", post(sync_positions_from_audits))
        .route("/api/positions/sync-from-chain", post(sync_positions_from_chain))
        .route("/api/tx/audit-redeem-position", post(audit_redeem_position))
        .route(
            "/api/managers/{address}",
            get(get_manager_for_address).post(put_manager_for_address),
        )
        .route("/api/strategies/compile-from-intent", post(compile_from_intent))
        .route("/api/strategies/compile", post(compile_strategy))
        .route("/api/tx/build-open-strategy", post(build_open_strategy))
        .route("/api/tx/audit-open-strategy", post(audit_open_strategy))
        .route("/api/audits", get(list_audits))
        .route("/api/audits/{digest}", get(get_audit))
        .route("/api/demo-status", post(demo_status))
        .route("/api/manager-balance", post(manager_balance))
        .route("/api/manager-balance-json", post(manager_balance_json))
        .route("/api/manager-positions", post(manager_positions))
        .route("/api/audit-execution", post(audit_execution))
        .route("/api/devinspect-mint-breakout", post(devinspect_mint_breakout))
        .route("/api/devinspect-redeem-breakout", post(devinspect_redeem_breakout))
        .layer(CorsLayer::permissive())
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

fn parse_leg_kind(s: &str) -> Option<LegKind> {
    match s {
        "UP" => Some(LegKind::Up),
        "DOWN" => Some(LegKind::Down),
        "RANGE" => Some(LegKind::Range),
        _ => None,
    }
}

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

async fn compile_from_intent(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CompileFromIntentRequest>,
) -> impl IntoResponse {
    let Some(strategy) = req.intent.get("recommendedStrategy").and_then(serde_json::Value::as_str)
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "ok": false,
                "error": "intent missing recommendedStrategy"
            })),
        );
    };

    let Some(style) = req.intent.get("recommendedStyle").and_then(serde_json::Value::as_str) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "ok": false,
                "error": "intent missing recommendedStyle"
            })),
        );
    };

    let Some(budget_dusdc) = req.intent.get("budgetDUSDC").and_then(serde_json::Value::as_str)
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "ok": false,
                "error": "intent missing budgetDUSDC"
            })),
        );
    };

    let args = vec![
        "compile-strategy-json".to_string(),
        "--owner".to_string(),
        req.owner.clone(),
        "--strategy".to_string(),
        strategy.to_string(),
        "--budget-dusdc".to_string(),
        budget_dusdc.to_string(),
        "--style".to_string(),
        style.to_string(),
        "--expiry-preference".to_string(),
        "nearest_active".to_string(),
        "--slippage-bps".to_string(),
        "100".to_string(),
    ];

    match run_cli_value(&state, args).await {
        Ok(mut compiled) => {
            if let Some(obj) = compiled.as_object_mut() {
                obj.insert(
                    "recommendation".to_string(),
                    serde_json::json!({
                        "source": "AI_INTENT_PLUS_DETERMINISTIC_COMPILER",
                        "intent": req.intent,
                        "reasoningSummary": req.intent
                            .get("reasoningSummary")
                            .cloned()
                            .unwrap_or(serde_json::Value::String(
                                "Strategy selected from parsed user intent.".to_string()
                            )),
                        "confidence": req.intent
                            .get("confidence")
                            .cloned()
                            .unwrap_or(serde_json::Value::from(0.65))
                    }),
                );
            }

            if let Some(id) = compiled.get("compiledStrategyId").and_then(serde_json::Value::as_str)
            {
                state.compiled.lock().await.insert(id.to_string(), compiled.clone());
            }

            (StatusCode::OK, Json(compiled))
        }
        Err((status, value)) => (status, Json(value)),
    }
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

    (StatusCode::OK, Json(serde_json::to_value(parsed).unwrap()))
}

async fn live_price(Query(query): Query<LivePriceQuery>) -> impl IntoResponse {
    let server_url = std::env::var("PREDICT_SERVER_URL")
        .unwrap_or_else(|_| DEFAULT_PREDICT_SERVER_URL.to_string());

    let client = reqwest::Client::new();

    let mut warnings = Vec::new();
    let mut raw_payload = serde_json::Value::Null;
    let mut source = "prices/latest".to_string();

    let latest_url = format!("{}/prices/latest", server_url.trim_end_matches('/'));

    let latest_result =
        client.get(&latest_url).send().await.and_then(|response| response.error_for_status());

    match latest_result {
        Ok(response) => match response.json::<serde_json::Value>().await {
            Ok(value) => {
                raw_payload = value;
            }
            Err(err) => {
                warnings.push(format!("failed to parse /prices/latest JSON: {err}"));
            }
        },
        Err(err) => {
            warnings.push(format!("failed to fetch /prices/latest: {err}"));
        }
    }

    if raw_payload.is_null() {
        if let Some(oracle_id) = &query.oracle_id {
            let oracle_url =
                format!("{}/oracles/{}/state", server_url.trim_end_matches('/'), oracle_id);

            match client.get(&oracle_url).send().await {
                Ok(response) => match response.error_for_status() {
                    Ok(response) => match response.json::<serde_json::Value>().await {
                        Ok(value) => {
                            source = "oracles/:oracle_id/state".to_string();
                            raw_payload = value;
                        }
                        Err(err) => {
                            warnings.push(format!("failed to parse oracle state JSON: {err}"));
                        }
                    },
                    Err(err) => warnings.push(format!("oracle state HTTP error: {err}")),
                },
                Err(err) => warnings.push(format!("failed to fetch oracle state: {err}")),
            }
        }
    }

    let asset = query.asset.to_uppercase();

    let price_raw = extract_price_raw_for_asset(&raw_payload, &asset)
        .or_else(|| extract_first_price_like_raw(&raw_payload));

    let updated_at_ms = extract_timestamp_ms(&raw_payload);

    let stale = updated_at_ms
        .map(|ts| {
            let now_ms = unix_now_secs().saturating_mul(1000);
            now_ms.saturating_sub(ts) > 60_000
        })
        .unwrap_or(true);

    if price_raw.is_none() {
        warnings.push("could not find a BTC price in DeepBook Predict payload".to_string());
    }

    let response = LivePriceResponse {
        ok: price_raw.is_some(),
        asset,
        source,
        price_raw: price_raw.map(|value| value.to_string()),
        price: price_raw.map(format_price_e9),
        updated_at_ms,
        stale,
        oracle_id: query.oracle_id,
        raw: raw_payload,
        warnings,
    };

    let status = if response.ok { StatusCode::OK } else { StatusCode::BAD_REQUEST };

    (status, Json(response))
}

async fn health(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    Json(HealthResponse {
        ok: true,
        service: "structx-api",
        cli_bin: state.cli_bin.to_string_lossy().to_string(),
    })
}

async fn compile_strategy(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CompileStrategyRequest>,
) -> impl IntoResponse {
    let mut args = vec![
        "compile-strategy-json".to_string(),
        "--owner".to_string(),
        req.owner,
        "--strategy".to_string(),
        req.strategy,
        "--budget-dusdc".to_string(),
        req.budget_dusdc,
        "--style".to_string(),
        req.style,
        "--expiry-preference".to_string(),
        req.expiry_preference,
        "--slippage-bps".to_string(),
        req.slippage_bps.to_string(),
    ];

    if let Some(value) = req.portfolio_exposure_dusdc {
        args.push("--portfolio-exposure-dusdc".to_string());
        args.push(value.to_string());
    }

    if let Some(value) = req.over_hedge_cap_bps {
        args.push("--over-hedge-cap-bps".to_string());
        args.push(value.to_string());
    }

    if let Some(value) = req.dead_zone_bps {
        args.push("--dead-zone-bps".to_string());
        args.push(value.to_string());
    }

    if let Some(value) = req.convex_gamma_bps {
        args.push("--convex-gamma-bps".to_string());
        args.push(value.to_string());
    }

    if let Some(value) = req.moonshot_range_weight_bps {
        args.push("--moonshot-range-weight-bps".to_string());
        args.push(value.to_string());
    }

    if let Some(value) = req.moonshot_tail_gamma_bps {
        args.push("--moonshot-tail-gamma-bps".to_string());
        args.push(value.to_string());
    }

    if let Some(value) = req.downside_range_weight_bps {
        args.push("--downside-range-weight-bps".to_string());
        args.push(value.to_string());
    }

    if let Some(value) = req.downside_tail_gamma_bps {
        args.push("--downside-tail-gamma-bps".to_string());
        args.push(value.to_string());
    }

    if let Some(value) = req.downside_near_range_weight_bps {
        args.push("--downside-near-range-weight-bps".to_string());
        args.push(value.to_string());
    }

    if let Some(value) = req.downside_lower_range_weight_bps {
        args.push("--downside-lower-range-weight-bps".to_string());
        args.push(value.to_string());
    }

    if let Some(value) = req.downside_step_tail_gamma_bps {
        args.push("--downside-step-tail-gamma-bps".to_string());
        args.push(value.to_string());
    }

    if let Some(value) = req.condor_center_weight_bps {
        args.push("--condor-center-weight-bps".to_string());
        args.push(value.to_string());
    }

    if let Some(value) = req.barrier_side {
        args.push("--barrier-side".to_string());
        args.push(value);
    }

    if let Some(value) = req.barrier_near_range_weight_bps {
        args.push("--barrier-near-range-weight-bps".to_string());
        args.push(value.to_string());
    }

    if let Some(value) = req.barrier_tail_gamma_bps {
        args.push("--barrier-tail-gamma-bps".to_string());
        args.push(value.to_string());
    }

    match run_cli_value(&state, args).await {
        Ok(value) => {
            if let Some(id) = value.get("compiledStrategyId").and_then(serde_json::Value::as_str) {
                state.compiled.lock().await.insert(id.to_string(), value.clone());
            }

            (StatusCode::OK, Json(value))
        }
        Err((status, value)) => (status, Json(value)),
    }
}

async fn demo_status(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DemoStatusRequest>,
) -> impl IntoResponse {
    run_cli_json(
        state,
        vec![
            "demo-status".to_string(),
            "--manager-id".to_string(),
            req.manager_id,
            "--sender".to_string(),
            req.sender,
            "--from-execution-json".to_string(),
            req.from_execution_json,
        ],
    )
    .await
}

async fn manager_balance_json(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ManagerBalanceRequest>,
) -> impl IntoResponse {
    let output = Command::new(&state.cli_bin)
        .args(["manager-balance", "--manager-id", req.manager_id.as_str()])
        .output()
        .await;

    let Ok(output) = output else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "ok": false,
                "error": "failed to run manager-balance CLI"
            })),
        );
    };

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "ok": false,
                "stdout": stdout,
                "stderr": stderr
            })),
        );
    }

    let Some(balance_raw) = extract_balance_raw_from_stdout(&stdout) else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "ok": false,
                "stdout": stdout,
                "stderr": "failed to parse balance raw"
            })),
        );
    };

    let display = format_dusdc_raw(balance_raw);

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "ok": true,
            "balanceRaw": balance_raw.to_string(),
            "balanceDisplay": display,
            "stdout": stdout
        })),
    )
}

async fn manager_balance(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ManagerBalanceRequest>,
) -> impl IntoResponse {
    run_cli_json(
        state,
        vec!["manager-balance".to_string(), "--manager-id".to_string(), req.manager_id],
    )
    .await
}

async fn manager_positions(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ManagerPositionsRequest>,
) -> impl IntoResponse {
    run_cli_json(
        state,
        vec![
            "manager-positions".to_string(),
            "--manager-id".to_string(),
            req.manager_id,
            "--sender".to_string(),
            req.sender,
            "--from-execution-json".to_string(),
            req.from_execution_json,
        ],
    )
    .await
}

async fn audit_execution(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AuditExecutionRequest>,
) -> impl IntoResponse {
    run_cli_json(
        state,
        vec![
            "audit-execution".to_string(),
            "--from-execution-json".to_string(),
            req.from_execution_json,
        ],
    )
    .await
}

async fn devinspect_mint_breakout(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DevinspectMintBreakoutRequest>,
) -> impl IntoResponse {
    let mut args = vec![
        "devinspect-mint-breakout".to_string(),
        "--manager-id".to_string(),
        req.manager_id,
        "--sender".to_string(),
        req.sender,
        "--max-total-mint-cost-raw".to_string(),
        req.max_total_mint_cost_raw.to_string(),
        "--slippage-bps".to_string(),
        req.slippage_bps.to_string(),
        "--max-quote-market-attempts".to_string(),
        req.max_quote_market_attempts.to_string(),
    ];

    if req.write_execute_script {
        args.push("--write-execute-script".to_string());
    }

    run_cli_json(state, args).await
}

async fn devinspect_redeem_breakout(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DevinspectRedeemBreakoutRequest>,
) -> impl IntoResponse {
    let mut args = vec![
        "devinspect-redeem-breakout".to_string(),
        "--manager-id".to_string(),
        req.manager_id,
        "--sender".to_string(),
        req.sender,
        "--from-execution-json".to_string(),
        req.from_execution_json,
    ];

    if req.auto_size_down {
        args.push("--auto-size-down".to_string());
    }

    if req.write_execute_script {
        args.push("--write-execute-script".to_string());
    }

    if req.allow_zero_payout_script {
        args.push("--allow-zero-payout-script".to_string());
    }

    run_cli_json(state, args).await
}

async fn build_open_strategy(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BuildOpenStrategyRequest>,
) -> impl IntoResponse {
    let compiled = {
        let cache = state.compiled.lock().await;
        cache.get(&req.compiled_strategy_id).cloned()
    };

    let Some(compiled) = compiled else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "ok": false,
                "error": "compiledStrategyId not found. Compile the strategy again before building transaction."
            })),
        );
    };

    let premium_required = compiled
        .get("premiumRequiredRaw")
        .and_then(serde_json::Value::as_str)
        .and_then(|v| v.parse::<u128>().ok())
        .unwrap_or(u128::MAX);

    let max_premium = req.max_premium_raw.parse::<u128>().unwrap_or(0);

    if premium_required > max_premium {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "ok": false,
                "error": "premium exceeds maxPremiumRaw",
                "premiumRequiredRaw": premium_required.to_string(),
                "maxPremiumRaw": max_premium.to_string()
            })),
        );
    }

    let oracle_id =
        compiled.get("oracleId").and_then(serde_json::Value::as_str).unwrap_or_default();

    let raw_legs =
        compiled.get("legs").and_then(serde_json::Value::as_array).cloned().unwrap_or_default();

    let legs = legs_with_max_costs(raw_legs, req.slippage_bps);

    let Some(expiry_ms) = compiled_expiry_ms(&req.compiled_strategy_id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "ok": false,
                "error": "compiledStrategyId missing expiry_ms"
            })),
        );
    };

    let warnings =
        compiled.get("warnings").and_then(serde_json::Value::as_array).cloned().unwrap_or_default();

    let mut response_warnings = warnings;
    if !response_warnings
        .iter()
        .any(|warning| warning.as_str() == Some("Live oracle pricing can change between preview and wallet signing; dry-run immediately before opening."))
    {
        response_warnings.push(serde_json::Value::String(
            "Live oracle pricing can change between preview and wallet signing; dry-run immediately before opening.".to_string(),
        ));
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "ok": true,
            "buildKind": "FRONTEND_TRANSACTION_BUILDER",
            "network": "sui:testnet",
            "compiledStrategyId": req.compiled_strategy_id,
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
            "warnings": response_warnings

        })),
    )
}

async fn audit_open_strategy(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AuditOpenStrategyRequest>,
) -> impl IntoResponse {
    let _guard = state.audit_lock.lock().await;

    let digest = sanitize_digest(&req.digest);

    let artifact = serde_json::json!({
        "digest": req.digest,
        "effects": req.effects,
        "events": req.events,
        "objectChanges": req.object_changes
    });

    let artifact_path = audit_artifact_path(&state.audits_dir, &digest);

    let bytes = match serde_json::to_vec_pretty(&artifact) {
        Ok(bytes) => bytes,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "ok": false,
                    "error": format!("failed to serialize audit artifact: {err}")
                })),
            );
        }
    };

    if let Err(err) = tokio::fs::write(&artifact_path, bytes).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "ok": false,
                "error": format!("failed to write audit artifact: {err}")
            })),
        );
    }

    let args = vec![
        "demo-status".to_string(),
        "--manager-id".to_string(),
        req.manager_id.clone(),
        "--sender".to_string(),
        req.owner.clone(),
        "--from-execution-json".to_string(),
        artifact_path.to_string_lossy().to_string(),
    ];

    let audit_output = match Command::new(&state.cli_bin).args(args).output().await {
        Ok(output) => output,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "ok": false,
                    "error": format!("failed to run audit CLI: {err}")
                })),
            );
        }
    };

    let ok = audit_output.status.success();
    let stdout = String::from_utf8_lossy(&audit_output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&audit_output.stderr).to_string();

    let record = serde_json::json!({
        "ok": ok,
        "digest": req.digest,
        "compiledStrategyId": req.compiled_strategy_id,
        "owner": req.owner,
        "managerId": req.manager_id,
        "createdAtUnix": unix_now_secs(),
        "artifactPath": artifact_path.to_string_lossy(),
        "stdout": stdout,
        "stderr": stderr,
        "summary": extract_audit_summary(&stdout)
    });

    let record_path = audit_record_path(&state.audits_dir, &digest);

    if let Err(err) =
        tokio::fs::write(&record_path, serde_json::to_vec_pretty(&record).unwrap_or_default()).await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "ok": false,
                "error": format!("failed to write audit record: {err}")
            })),
        );
    }

    let status = if ok { StatusCode::OK } else { StatusCode::BAD_REQUEST };

    (status, Json(record))
}

async fn list_audits(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut records = Vec::new();

    let read_dir = match fs::read_dir(&state.audits_dir) {
        Ok(read_dir) => read_dir,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "ok": false,
                    "error": format!("failed to read audits dir: {err}")
                })),
            );
        }
    };

    for entry in read_dir.flatten() {
        let path = entry.path();

        if !path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.ends_with(".record.json"))
            .unwrap_or(false)
        {
            continue;
        }

        let Ok(bytes) = fs::read(&path) else {
            continue;
        };

        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            continue;
        };

        records.push(value);
    }

    records.sort_by(|a, b| {
        let a_time = a.get("createdAtUnix").and_then(serde_json::Value::as_u64).unwrap_or(0);
        let b_time = b.get("createdAtUnix").and_then(serde_json::Value::as_u64).unwrap_or(0);

        b_time.cmp(&a_time)
    });

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "ok": true,
            "count": records.len(),
            "audits": records
        })),
    )
}

async fn get_audit(
    State(state): State<Arc<AppState>>,
    Path(digest): Path<String>,
) -> impl IntoResponse {
    let digest = sanitize_digest(&digest);
    let path = audit_record_path(&state.audits_dir, &digest);

    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "ok": false,
                    "error": "audit not found"
                })),
            );
        }
    };

    let value = match serde_json::from_slice::<serde_json::Value>(&bytes) {
        Ok(value) => value,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "ok": false,
                    "error": format!("failed to parse audit record: {err}")
                })),
            );
        }
    };

    (StatusCode::OK, Json(value))
}

fn extract_price_raw_for_asset(value: &serde_json::Value, asset: &str) -> Option<u64> {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                let key_upper = key.to_uppercase();

                if key_upper == asset
                    || key_upper == format!("{asset}/USD")
                    || key_upper == format!("{asset}-USD")
                    || key_upper.contains(asset)
                {
                    if let Some(price) = extract_first_price_like_raw(child) {
                        return Some(price);
                    }
                }
            }

            for child in map.values() {
                if let Some(price) = extract_price_raw_for_asset(child, asset) {
                    return Some(price);
                }
            }

            None
        }
        serde_json::Value::Array(items) => {
            for item in items {
                if let Some(price) = extract_price_raw_for_asset(item, asset) {
                    return Some(price);
                }
            }

            None
        }
        _ => None,
    }
}

fn extract_first_price_like_raw(value: &serde_json::Value) -> Option<u64> {
    match value {
        serde_json::Value::Object(map) => {
            for key in [
                "price_raw",
                "priceRaw",
                "raw_price",
                "rawPrice",
                "oracle_price",
                "oraclePrice",
                "latest_price",
                "latestPrice",
                "price",
                "value",
            ] {
                if let Some(raw) = map.get(key).and_then(json_number_or_string_u64) {
                    return normalize_price_to_e9(raw);
                }
            }

            for child in map.values() {
                if let Some(price) = extract_first_price_like_raw(child) {
                    return Some(price);
                }
            }

            None
        }
        serde_json::Value::Array(items) => {
            for item in items {
                if let Some(price) = extract_first_price_like_raw(item) {
                    return Some(price);
                }
            }

            None
        }
        _ => json_number_or_string_u64(value).and_then(normalize_price_to_e9),
    }
}

fn extract_timestamp_ms(value: &serde_json::Value) -> Option<u64> {
    match value {
        serde_json::Value::Object(map) => {
            for key in [
                "updated_at_ms",
                "updatedAtMs",
                "timestamp_ms",
                "timestampMs",
                "price_timestamp_ms",
                "priceTimestampMs",
                "last_updated_ms",
                "lastUpdatedMs",
            ] {
                if let Some(ts) = map.get(key).and_then(json_number_or_string_u64) {
                    return Some(ts);
                }
            }

            for key in ["updated_at", "updatedAt", "timestamp", "lastUpdated"] {
                if let Some(ts) = map.get(key).and_then(json_number_or_string_u64) {
                    if ts < 10_000_000_000 {
                        return Some(ts.saturating_mul(1000));
                    }

                    return Some(ts);
                }
            }

            for child in map.values() {
                if let Some(ts) = extract_timestamp_ms(child) {
                    return Some(ts);
                }
            }

            None
        }
        serde_json::Value::Array(items) => {
            for item in items {
                if let Some(ts) = extract_timestamp_ms(item) {
                    return Some(ts);
                }
            }

            None
        }
        _ => None,
    }
}

fn json_number_or_string_u64(value: &serde_json::Value) -> Option<u64> {
    match value {
        serde_json::Value::Number(number) => number.as_u64(),
        serde_json::Value::String(value) => {
            if let Ok(raw) = value.parse::<u64>() {
                Some(raw)
            } else if let Ok(float) = value.parse::<f64>() {
                Some((float * 1_000_000_000.0) as u64)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn normalize_price_to_e9(raw: u64) -> Option<u64> {
    if raw == 0 {
        return None;
    }

    if raw < 1_000_000 {
        return raw.checked_mul(1_000_000_000);
    }

    Some(raw)
}

fn format_price_e9(raw: u64) -> String {
    let whole = raw / 1_000_000_000;
    let frac = raw % 1_000_000_000;
    let cents = frac / 10_000_000;

    format!("${whole}.{cents:02}")
}

fn sanitize_digest(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '-')
        .collect::<String>()
}

fn audit_artifact_path(audits_dir: &StdPath, digest: &str) -> PathBuf {
    audits_dir.join(format!("structx_audit_{digest}.json"))
}

fn audit_record_path(audits_dir: &StdPath, digest: &str) -> PathBuf {
    audits_dir.join(format!("structx_audit_{digest}.record.json"))
}

fn unix_now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn extract_audit_summary(stdout: &str) -> serde_json::Value {
    serde_json::json!({
        "executionStatus": find_stdout_value(stdout, "execution status:"),
        "totalCost": find_stdout_value(stdout, "total cost:"),
        "managerBalance": find_stdout_value(stdout, "balance:"),
        "positionVerification": if stdout.contains("Position verification: ok") {
            "ok"
        } else if stdout.contains("Position verification: partial") {
            "partial"
        } else if stdout.contains("Position verification") {
            "failed"
        } else {
            "unknown"
        },
        "mintedLegCount": stdout.matches("PositionMinted").count() + stdout.matches("RangeMinted").count()
    })
}

fn find_stdout_value(stdout: &str, prefix: &str) -> Option<String> {
    stdout.lines().find_map(|line| {
        let trimmed = line.trim();

        if trimmed.to_lowercase().starts_with(prefix) {
            Some(trimmed[(trimmed.find(':')? + 1)..].trim().to_string())
        } else {
            None
        }
    })
}

fn ceil_mul_bps(raw: &str, bps: u16) -> String {
    let value = raw.parse::<u128>().unwrap_or(0);
    let multiplier = 10_000u128 + bps as u128;

    let out = value.saturating_mul(multiplier).saturating_add(9_999) / 10_000;

    out.to_string()
}

fn compiled_expiry_ms(compiled_strategy_id: &str) -> Option<String> {
    // breakout:{owner}:{oracle}:{expiry_ms}:{premium}:{style}
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

fn extract_balance_raw_from_stdout(stdout: &str) -> Option<u64> {
    for line in stdout.lines() {
        let trimmed = line.trim();

        if let Some(rest) = trimmed.strip_prefix("balance raw:") {
            return rest.trim().parse::<u64>().ok();
        }
    }

    None
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

async fn parse_intent_with_openai_or_fallback(
    req: &ParseIntentRequest,
) -> Result<ParsedIntent, Box<dyn std::error::Error + Send + Sync>> {
    let api_key = match std::env::var("OPENAI_API_KEY") {
        Ok(value) if !value.trim().is_empty() => value,
        _ => return Ok(deterministic_parse_intent(req)),
    };

    let model = std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string());

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
            "asset": {
                "type": "string",
                "enum": ["BTC"]
            },
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
            "budgetDUSDC": {
                "type": "string"
            },
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
                "enum": ["BREAKOUT_PROTECTION", "NEAR_BARRIER_PROXY"]
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
            "reasoningSummary": {
                "type": "string"
            },
            "missingFields": {
                "type": "array",
                "items": { "type": "string" }
            },
            "warnings": {
                "type": "array",
                "items": { "type": "string" }
            }
        }
    });

    let input = format!(
        r#"
You are the StructX intent parser.

StructX is a non-custodial structured payoff builder on DeepBook Predict testnet.
You do not give financial advice.
You only convert user intent into strict JSON for deterministic compiler logic.
Supported asset: BTC only.
Supported strategies for this milestone: BREAKOUT_PROTECTION and NEAR_BARRIER_PROXY.
Supported expiry preference: nearest_active.

Rules:
- If the user wants protection, crash hedge, dump protection, or downside coverage, goal = downside_protection.
- If the user wants a big move either direction, volatility, or breakout, goal = two_sided_breakout.
- If the user wants moonshot/upside/rally exposure, goal = upside_speculation.
- If the user mentions a near barrier, barrier, close target, or near target, recommendedStrategy = NEAR_BARRIER_PROXY.
- conservative -> higher-hit-rate unless user explicitly asks for tail.
- aggressive/max payout -> tail-heavy.
- balanced/default -> balanced.
- If budget is missing, include missingFields ["budgetDUSDC"].
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

    let client = reqwest::Client::new();
    let response = client
        .post("https://api.openai.com/v1/responses")
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;

    let text = extract_openai_text(&response).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "OpenAI response missing structured output text",
        )
    })?;

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

    let recommended_strategy =
        if contains_any(&msg, &["near barrier", "barrier", "close to target", "near target"]) {
            "NEAR_BARRIER_PROXY"
        } else if contains_any(
            &msg,
            &["expires far", "far from current", "expiry move", "terminal move"],
        ) {
            "EXPIRY_MOVE_NOTE"
        } else if contains_any(
            &msg,
            &[
                "protect",
                "protection",
                "hedge",
                "downside",
                "dump",
                "crash",
                "sell-off",
                "selldown",
            ],
        ) {
            "PORTFOLIO_CRASH_SHIELD"
        } else if contains_any(
            &msg,
            &["moon", "upside", "rally", "pump", "breaks up", "breakout up"],
        ) {
            "CONVEX_TAIL_LADDER"
        } else if contains_any(
            &msg,
            &["big move", "breakout", "volatile", "volatility", "either direction", "move a lot"],
        ) {
            "CONVEX_TAIL_LADDER"
        } else {
            "BREAKOUT_PROTECTION"
        };

    let goal = if contains_any(
        &msg,
        &["protect", "protection", "hedge", "downside", "dump", "crash", "sell-off", "selldown"],
    ) {
        "downside_protection"
    } else if contains_any(&msg, &["moon", "upside", "rally", "pump", "breaks up", "breakout up"]) {
        "upside_speculation"
    } else {
        "two_sided_breakout"
    };

    let risk =
        req.risk_preference.clone().unwrap_or_else(|| infer_risk_preference(&msg).to_string());

    let style = style_from_goal_and_risk(goal, &risk).to_string();

    let budget = req.budget_dusdc.clone().unwrap_or_default();

    let mut missing_fields = Vec::new();
    if budget.trim().is_empty() {
        missing_fields.push("budgetDUSDC".to_string());
    }

    ParsedIntent {
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
        reasoning_summary: reasoning_for_goal(goal).to_string(),
        missing_fields,
        warnings: vec![
            "AI-assisted strategy discovery is not financial advice.".to_string(),
            "DeepBook Predict integration is testnet-only.".to_string(),
            "Final premium, payoff, and transaction are produced by deterministic StructX compiler logic."
                .to_string(),
        ],
    }
}

fn validate_and_rewrite_intent(parsed: &mut ParsedIntent) {
    parsed.asset = "BTC".to_string();
    if !matches!(
        parsed.recommended_strategy.as_str(),
        "BREAKOUT_PROTECTION"
            | "PORTFOLIO_CRASH_SHIELD"
            | "CONVEX_TAIL_LADDER"
            | "EXPIRY_MOVE_NOTE"
            | "MOONSHOT_UPSIDE"
            | "DOWNSIDE_CONVEXITY"
            | "DOWNSIDE_STEP_LADDER"
            | "CENTER_BAND_CONDOR"
            | "NEAR_BARRIER_PROXY"
            | "RANGE_CONVICTION"
            | "SMART_BUDGET_SELECTOR"
    ) {
        parsed.recommended_strategy = "BREAKOUT_PROTECTION".to_string();
    }

    parsed.risk_preference = normalize_risk(&parsed.risk_preference).to_string();

    if !matches!(parsed.time_preference.as_str(), "nearest_active" | "today" | "this_week") {
        parsed.time_preference = "nearest_active".to_string();
    }

    if !matches!(parsed.recommended_style.as_str(), "tail-heavy" | "balanced" | "higher-hit-rate") {
        parsed.recommended_style =
            style_from_goal_and_risk(&parsed.goal, &parsed.risk_preference).to_string();
    }

    if parsed.budget_dusdc.trim().is_empty()
        && !parsed.missing_fields.contains(&"budgetDUSDC".to_string())
    {
        parsed.missing_fields.push("budgetDUSDC".to_string());
    }

    if !parsed
        .warnings
        .iter()
        .any(|warning| warning.to_lowercase().contains("not financial advice"))
    {
        parsed.warnings.push("AI-assisted strategy discovery is not financial advice.".to_string());
    }

    if !parsed.warnings.iter().any(|warning| warning.to_lowercase().contains("testnet")) {
        parsed.warnings.push("DeepBook Predict integration is testnet-only.".to_string());
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

fn reasoning_for_goal(goal: &str) -> &'static str {
    match goal {
        "downside_protection" => {
            "Breakout Protection is recommended because it can allocate more exposure to downside tail protection while keeping risk defined by the premium."
        }
        "upside_speculation" => {
            "Breakout Protection is recommended for this milestone because Moonshot Upside execution is not yet live, while the breakout structure can still express upside participation."
        }
        "two_sided_breakout" => {
            "Breakout Protection is recommended because the user is expressing a large-move view without requiring a single direction."
        }
        _ => "Breakout Protection is recommended as the currently supported defined-risk strategy.",
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
/// this wallet. Body: `{ "managerId": "0x..." }`
async fn put_manager_for_address(
    State(state): State<Arc<AppState>>,
    Path(address): Path<String>,
    Json(body): Json<SaveManagerRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let started = Instant::now();
    let key = normalize_address(&address);
    let manager_id = body.manager_id.trim().to_string();
    if !manager_id.starts_with("0x") || manager_id.len() < 4 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "ok": false,
                "error": "managerId must be a 0x-prefixed object id"
            })),
        ));
    }

    let prior;
    {
        let mut map = state.managers.lock().await;
        prior = map.insert(key.clone(), manager_id.clone());
        if let Err(err) = save_managers(&state.managers_path, &map) {
            // Roll back the in memory insert so the in memory map stays in
            // sync with what's on disk
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

async fn run_cli_value(
    state: &AppState,
    args: Vec<String>,
) -> Result<serde_json::Value, (StatusCode, serde_json::Value)> {
    match Command::new(&state.cli_bin).args(&args).output().await {
        Ok(output) => {
            let code = output.status.code();
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            if output.status.success() {
                serde_json::from_str::<serde_json::Value>(&stdout).map_err(|err| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        serde_json::json!({
                            "ok": false,
                            "code": code,
                            "stdout": stdout,
                            "stderr": format!("CLI returned non-JSON stdout: {err}; stderr: {stderr}")
                        }),
                    )
                })
            } else {
                Err((
                    StatusCode::BAD_REQUEST,
                    serde_json::json!({
                        "ok": false,
                        "code": code,
                        "stdout": stdout,
                        "stderr": stderr
                    }),
                ))
            }
        }
        Err(err) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            serde_json::json!({
                "ok": false,
                "code": null,
                "stdout": "",
                "stderr": format!("failed to run CLI: {err}")
            }),
        )),
    }
}

async fn run_cli_json(state: Arc<AppState>, args: Vec<String>) -> impl IntoResponse {
    match Command::new(&state.cli_bin).args(&args).output().await {
        Ok(output) => {
            let code = output.status.code();
            let ok = output.status.success();

            let body = CliResponse {
                ok,
                code,
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            };

            let status = if ok { StatusCode::OK } else { StatusCode::BAD_REQUEST };

            (status, Json(body))
        }
        Err(err) => {
            let body = CliResponse {
                ok: false,
                code: None,
                stdout: String::new(),
                stderr: format!("failed to run CLI: {err}"),
            };

            (StatusCode::INTERNAL_SERVER_ERROR, Json(body))
        }
    }
}
