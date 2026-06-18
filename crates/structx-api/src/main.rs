use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{env, net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::process::Command;
use tower_http::cors::CorsLayer;

#[derive(Debug, Clone)]
struct AppState {
    cli_bin: PathBuf,
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

    let state = Arc::new(AppState { cli_bin });

    let app = Router::new()
        .route("/health", get(health))
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
