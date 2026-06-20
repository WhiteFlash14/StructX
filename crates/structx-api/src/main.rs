#[allow(dead_code)]
mod intent_audit;
#[allow(dead_code)]
mod intent_positions;
#[allow(dead_code)]
mod proposal_store;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    env, fs,
    net::SocketAddr,
    path::{Path as StdPath, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
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

#[tokio::main]
async fn main() {
    let cli_bin = env::var("STRUCTX_CLI_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("target/debug/structx-cli"));

    let audits_dir = env::var("STRUCTX_AUDITS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("artifacts/audits"));

    fs::create_dir_all(&audits_dir).expect("create audits dir");

    let state = Arc::new(AppState {
        cli_bin,
        compiled: Arc::new(Mutex::new(HashMap::new())),
        audits_dir,
        audit_lock: Arc::new(Mutex::new(())),
    });

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/market/live-price", get(live_price))
        .route("/api/intent/parse", post(parse_intent))
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
