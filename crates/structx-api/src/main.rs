use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, env, net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::{process::Command, sync::Mutex};
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
    cli_bin: PathBuf,
    compiled: Arc<Mutex<HashMap<String, serde_json::Value>>>,
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

    let state = Arc::new(AppState {
        cli_bin,
        compiled: Arc::new(Mutex::new(HashMap::new())),
    });

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/strategies/compile", post(compile_strategy))
        .route("/api/tx/build-open-strategy", post(build_open_strategy))
        .route("/api/tx/audit-open-strategy", post(audit_open_strategy))
        .route("/api/demo-status", post(demo_status))
        .route("/api/manager-balance", post(manager_balance))
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
    let args = vec![
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

    match run_cli_value(&state, args).await {
        Ok(value) => {
            if let Some(id) = value
                .get("compiledStrategyId")
                .and_then(serde_json::Value::as_str)
            {
                state
                    .compiled
                    .lock()
                    .await
                    .insert(id.to_string(), value.clone());
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

    let oracle_id = compiled
        .get("oracleId")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();

    let legs = compiled
        .get("legs")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();

    let warnings = compiled
        .get("warnings")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "ok": true,
            "buildKind": "FRONTEND_TRANSACTION_BUILDER",
            "network": "sui:testnet",
            "compiledStrategyId": req.compiled_strategy_id,
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
    let artifact = serde_json::json!({
        "digest": req.digest,
        "effects": req.effects,
        "events": req.events,
        "objectChanges": req.object_changes
    });

    let path = std::env::temp_dir().join(format!(
        "structx_audit_{}.json",
        artifact
            .get("digest")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown")
    ));

    if let Err(err) = tokio::fs::write(
        &path,
        match serde_json::to_vec_pretty(&artifact) {
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
        },
    )
    .await
    {
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
        req.manager_id,
        "--sender".to_string(),
        req.owner,
        "--from-execution-json".to_string(),
        path.to_string_lossy().to_string(),
    ];

    match Command::new(&state.cli_bin).args(args).output().await {
        Ok(output) => {
            let ok = output.status.success();
            let status = if ok {
                StatusCode::OK
            } else {
                StatusCode::BAD_REQUEST
            };

            (
                status,
                Json(serde_json::json!({
                    "ok": ok,
                    "compiledStrategyId": req.compiled_strategy_id,
                    "artifactPath": path.to_string_lossy(),
                    "stdout": String::from_utf8_lossy(&output.stdout).to_string(),
                    "stderr": String::from_utf8_lossy(&output.stderr).to_string()
                })),
            )
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "ok": false,
                "error": format!("failed to run audit CLI: {err}")
            })),
        ),
    }
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
