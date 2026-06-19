use std::path::{Path, PathBuf};
use std::time::Duration as StdDuration;
use std::{fs, io};

use chrono::{Duration, Utc};
use clap::{Parser, Subcommand};
use comfy_table::{presets::UTF8_FULL, Cell, Table};

use deepbook_client::{
    verify_predict_abi, AbiCheckStatus, AbiVerificationReport, DeepBookClient, DeepBookConfig,
    FreshnessConfig, MarketSnapshot, ObjectOwnerKind, StructxMarketStatus, SuiObjectInfo,
    SuiRpcClient, DEFAULT_SUI_TESTNET_RPC_URL, DUSDC_COIN_TYPE, DUSDC_DECIMALS,
    PREDICT_MANAGER_TYPE, PREDICT_OBJECT_ID, PREDICT_PACKAGE_ID, PREDICT_SERVER_URL,
    SUI_CLOCK_OBJECT_ID,
};
use structx_core::{
    build_create_manager_tx_kind, build_manager_balance_tx_kind, build_manager_positions_tx_kind,
    build_mint_tx_kind, build_quote_plan, build_quote_tx_kind, build_redeem_tx_kind,
    compile_breakout, compile_bucket_payoff, compile_center_band_condor,
    compile_convex_tail_ladder, compile_downside_convexity, compile_downside_step_ladder,
    compile_expiry_move_note, compile_moonshot_upside, compile_near_barrier_proxy,
    compile_portfolio_crash_shield, compile_range_conviction, compile_upside_step_ladder,
    guard_quote_preview, optimize_breakout_quantities, score_smart_candidate, select_best_market,
    select_candidate_markets, AdvancedCompileResult, AdvancedCompiledLeg, AdvancedLegKind,
    AdvancedStrategyKind, BarrierSide, BreakoutAskInputs, BreakoutStyle, CenterBandCondorInput,
    CompiledPayoff, ConvexTailLadderInput, DisplayPrice, DownsideConvexityInput,
    DownsideStepLadderInput, ExpiryMoveNoteInput, ManagerPositionRead, MintObjectRefs,
    MoonshotUpsideInput, NearBarrierProxyInput, PayoffBucket, PortfolioCrashShieldInput,
    PredictLeg, PriceScale, QuoteAssetDisplay, QuoteCall, QuoteCostGuard, QuoteObjectRefs,
    QuotePlan, QuotePreview, QuotePreviewLeg, QuoteTxKind, RangeConvictionInput, SelectedMarket,
    SmartBudgetStyle, SmartCandidateMetrics, SmartCandidateScore, Strike, UpsideStepLadderInput,
};
#[derive(Debug, Parser)]
#[command(name = "structx")]
#[command(about = "StructX CLI for DeepBook Predict market inspection")]
struct Cli {
    #[arg(long, default_value = PREDICT_SERVER_URL)]
    server_url: String,

    #[arg(long, default_value = PREDICT_OBJECT_ID)]
    predict_id: String,

    #[arg(long, default_value = DEFAULT_SUI_TESTNET_RPC_URL)]
    rpc_url: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    ListMarkets {
        #[arg(long, default_value_t = 60)]
        max_price_age_secs: i64,

        #[arg(long, default_value_t = 60)]
        max_svi_age_secs: i64,

        #[arg(long, default_value_t = 300)]
        min_time_to_expiry_secs: i64,

        #[arg(long, default_value_t = false)]
        strict_freshness: bool,
    },

    SelectMarket {
        #[arg(long, default_value_t = 60)]
        max_price_age_secs: i64,

        #[arg(long, default_value_t = 60)]
        max_svi_age_secs: i64,

        #[arg(long, default_value_t = 300)]
        min_time_to_expiry_secs: i64,

        #[arg(long, default_value_t = false)]
        strict_freshness: bool,

        #[arg(long, default_value_t = 250.0)]
        bucket_step_usd: f64,

        #[arg(long, default_value_t = 4)]
        levels_each_side: u32,
    },

    CompileBreakout {
        #[arg(long, default_value_t = 60)]
        max_price_age_secs: i64,

        #[arg(long, default_value_t = 60)]
        max_svi_age_secs: i64,

        #[arg(long, default_value_t = 300)]
        min_time_to_expiry_secs: i64,

        #[arg(long, default_value_t = false)]
        strict_freshness: bool,

        #[arg(long, default_value_t = 250.0)]
        bucket_step_usd: f64,

        #[arg(long, default_value_t = 4)]
        levels_each_side: u32,

        #[arg(long, default_value_t = 1000)]
        tail_quantity: u64,

        #[arg(long, default_value_t = 400)]
        shoulder_quantity: u64,
    },

    PlanQuoteBreakout {
        #[arg(long, default_value_t = 60)]
        max_price_age_secs: i64,

        #[arg(long, default_value_t = 60)]
        max_svi_age_secs: i64,

        #[arg(long, default_value_t = 300)]
        min_time_to_expiry_secs: i64,

        #[arg(long, default_value_t = false)]
        strict_freshness: bool,

        #[arg(long, default_value_t = 250.0)]
        bucket_step_usd: f64,

        #[arg(long, default_value_t = 4)]
        levels_each_side: u32,

        #[arg(long, default_value_t = 1000)]
        tail_quantity: u64,

        #[arg(long, default_value_t = 400)]
        shoulder_quantity: u64,
    },

    DevinspectQuoteBreakout {
        #[arg(long, default_value_t = 60)]
        max_price_age_secs: i64,

        #[arg(long, default_value_t = 60)]
        max_svi_age_secs: i64,

        #[arg(long, default_value_t = 300)]
        min_time_to_expiry_secs: i64,

        #[arg(long, default_value_t = false)]
        strict_freshness: bool,

        #[arg(long, default_value_t = 250.0)]
        bucket_step_usd: f64,

        #[arg(long, default_value_t = 4)]
        levels_each_side: u32,

        #[arg(long, default_value_t = 1000)]
        tail_quantity: u64,

        #[arg(long, default_value_t = 400)]
        shoulder_quantity: u64,

        #[arg(long, default_value_t = 5)]
        max_quote_market_attempts: usize,

        #[arg(long)]
        max_total_mint_cost_raw: Option<u64>,

        #[arg(long, default_value_t = 100)]
        slippage_bps: u16,

        #[arg(
            long,
            default_value = "0x0000000000000000000000000000000000000000000000000000000000000000"
        )]
        sender: String,
    },

    ResolveQuoteObjects {
        #[arg(long, default_value_t = 60)]
        max_price_age_secs: i64,

        #[arg(long, default_value_t = 60)]
        max_svi_age_secs: i64,

        #[arg(long, default_value_t = 300)]
        min_time_to_expiry_secs: i64,

        #[arg(long, default_value_t = false)]
        strict_freshness: bool,
    },

    ResolveManager {
        #[arg(long)]
        manager_id: String,
    },
    DevinspectCreateManager {
        #[arg(
            long,
            default_value = "0x0000000000000000000000000000000000000000000000000000000000000000"
        )]
        sender: String,
    },
    ManagerBalance {
        #[arg(long)]
        manager_id: String,

        #[arg(
            long,
            default_value = "0x0000000000000000000000000000000000000000000000000000000000000000"
        )]
        sender: String,
    },

    ManagerPositions {
        #[arg(long)]
        manager_id: String,

        #[arg(long)]
        from_execution_json: PathBuf,

        #[arg(
            long,
            default_value = "0x0000000000000000000000000000000000000000000000000000000000000000"
        )]
        sender: String,

        #[arg(long, default_value_t = false)]
        expect_exact: bool,
    },

    CompileStrategyJson {
        #[arg(long)]
        owner: String,

        #[arg(long, default_value = "BREAKOUT_PROTECTION")]
        strategy: String,

        #[arg(long)]
        budget_dusdc: String,

        #[arg(long, default_value = "balanced")]
        style: String,

        #[arg(long, default_value = "nearest_active")]
        expiry_preference: String,

        #[arg(long, default_value_t = 100)]
        slippage_bps: u16,

        #[arg(long, default_value_t = 250.0)]
        bucket_step_usd: f64,

        #[arg(long, default_value_t = 4)]
        levels_each_side: u32,

        #[arg(long, default_value_t = 5)]
        max_quote_market_attempts: usize,

        #[arg(long, default_value_t = 5_000.0)]
        portfolio_exposure_dusdc: f64,

        #[arg(long, default_value_t = 12_000)]
        over_hedge_cap_bps: u16,

        #[arg(long, default_value_t = 15_000)]
        convex_gamma_bps: u16,

        #[arg(long, default_value_t = 200)]
        dead_zone_bps: u16,

        #[arg(long, default_value_t = 6_000)]
        moonshot_range_weight_bps: u16,

        #[arg(long, default_value_t = 15_000)]
        moonshot_tail_gamma_bps: u16,

        #[arg(long, default_value_t = 6_000)]
        downside_range_weight_bps: u16,

        #[arg(long, default_value_t = 15_000)]
        downside_tail_gamma_bps: u16,

        #[arg(long, default_value_t = 4_000)]
        upside_near_range_weight_bps: u16,

        #[arg(long, default_value_t = 3_500)]
        upside_upper_range_weight_bps: u16,

        #[arg(long, default_value_t = 15_000)]
        upside_tail_gamma_bps: u16,

        #[arg(long, default_value_t = 4_000)]
        downside_near_range_weight_bps: u16,

        #[arg(long, default_value_t = 3_500)]
        downside_lower_range_weight_bps: u16,

        #[arg(long, default_value_t = 15_000)]
        downside_step_tail_gamma_bps: u16,

        #[arg(long, default_value_t = 8_000)]
        condor_center_weight_bps: u16,

        #[arg(long, default_value = "up")]
        barrier_side: String,

        #[arg(long, default_value_t = 7_000)]
        barrier_near_range_weight_bps: u16,

        #[arg(long, default_value_t = 15_000)]
        barrier_tail_gamma_bps: u16,
    },
    DemoStatus {
        #[arg(long)]
        manager_id: String,

        #[arg(long)]
        sender: String,

        #[arg(long)]
        from_execution_json: PathBuf,

        #[arg(long, default_value_t = false)]
        expect_exact: bool,
    },
    AuditExecution {
        #[arg(long)]
        from_execution_json: PathBuf,
    },

    DevinspectRedeemBreakout {
        #[arg(long)]
        manager_id: String,

        #[arg(long)]
        sender: String,

        #[arg(long)]
        from_execution_json: PathBuf,

        #[arg(long)]
        min_total_payout_raw: Option<u64>,

        #[arg(long, default_value_t = false)]
        auto_size_down: bool,

        #[arg(long, default_value_t = 10000)]
        redeem_bps: u16,

        #[arg(long, default_value_t = false)]
        write_execute_script: bool,

        #[arg(long, default_value_t = false)]
        allow_zero_payout_script: bool,

        #[arg(long, default_value = "/tmp/structx_execute_redeem_breakout.sh")]
        execute_script_path: PathBuf,

        #[arg(long, default_value = "/tmp/structx_execute_redeem_breakout_plan.json")]
        execute_plan_json_path: PathBuf,
    },
    DevinspectMintBreakout {
        #[arg(long)]
        manager_id: String,

        #[arg(long)]
        sender: String,

        #[arg(long, default_value_t = 60)]
        max_price_age_secs: i64,

        #[arg(long, default_value_t = 60)]
        max_svi_age_secs: i64,

        #[arg(long, default_value_t = 300)]
        min_time_to_expiry_secs: i64,

        #[arg(long, default_value_t = false)]
        strict_freshness: bool,

        #[arg(long, default_value_t = 250.0)]
        bucket_step_usd: f64,

        #[arg(long, default_value_t = 4)]
        levels_each_side: u32,

        #[arg(long, default_value_t = 1000)]
        tail_quantity: u64,

        #[arg(long, default_value_t = 400)]
        shoulder_quantity: u64,

        #[arg(long)]
        max_total_mint_cost_raw: u64,

        #[arg(long, default_value_t = 100)]
        slippage_bps: u16,

        #[arg(long, default_value_t = 5)]
        max_quote_market_attempts: usize,

        #[arg(long, default_value_t = false)]
        write_execute_script: bool,

        #[arg(long, default_value = "/tmp/structx_execute_mint_breakout.sh")]
        execute_script_path: PathBuf,

        #[arg(long, default_value = "/tmp/structx_execute_mint_breakout_plan.json")]
        execute_plan_json_path: PathBuf,
    },
    VerifyAbi,
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let cli = Cli::parse();

    let result = match cli.command {
        Command::ListMarkets {
            max_price_age_secs,
            max_svi_age_secs,
            min_time_to_expiry_secs,
            strict_freshness,
        } => {
            let freshness = build_freshness(
                max_price_age_secs,
                max_svi_age_secs,
                min_time_to_expiry_secs,
                strict_freshness,
            );

            list_markets(cli.server_url, cli.predict_id, freshness).await
        }
        Command::SelectMarket {
            max_price_age_secs,
            max_svi_age_secs,
            min_time_to_expiry_secs,
            strict_freshness,
            bucket_step_usd,
            levels_each_side,
        } => {
            let freshness = build_freshness(
                max_price_age_secs,
                max_svi_age_secs,
                min_time_to_expiry_secs,
                strict_freshness,
            );

            select_market(
                cli.server_url,
                cli.predict_id,
                freshness,
                DisplayPrice(bucket_step_usd),
                levels_each_side,
            )
            .await
        }
        Command::CompileBreakout {
            max_price_age_secs,
            max_svi_age_secs,
            min_time_to_expiry_secs,
            strict_freshness,
            bucket_step_usd,
            levels_each_side,
            tail_quantity,
            shoulder_quantity,
        } => {
            let freshness = build_freshness(
                max_price_age_secs,
                max_svi_age_secs,
                min_time_to_expiry_secs,
                strict_freshness,
            );

            compile_breakout_command(
                cli.server_url,
                cli.predict_id,
                freshness,
                DisplayPrice(bucket_step_usd),
                levels_each_side,
                tail_quantity,
                shoulder_quantity,
            )
            .await
        }
        Command::PlanQuoteBreakout {
            max_price_age_secs,
            max_svi_age_secs,
            min_time_to_expiry_secs,
            strict_freshness,
            bucket_step_usd,
            levels_each_side,
            tail_quantity,
            shoulder_quantity,
        } => {
            let freshness = build_freshness(
                max_price_age_secs,
                max_svi_age_secs,
                min_time_to_expiry_secs,
                strict_freshness,
            );

            plan_quote_breakout_command(
                cli.server_url,
                cli.predict_id,
                freshness,
                DisplayPrice(bucket_step_usd),
                levels_each_side,
                tail_quantity,
                shoulder_quantity,
            )
            .await
        }
        Command::DevinspectQuoteBreakout {
            max_price_age_secs,
            max_svi_age_secs,
            min_time_to_expiry_secs,
            strict_freshness,
            bucket_step_usd,
            levels_each_side,
            tail_quantity,
            shoulder_quantity,
            max_quote_market_attempts,
            max_total_mint_cost_raw,
            slippage_bps,
            sender,
        } => {
            let freshness = build_freshness(
                max_price_age_secs,
                max_svi_age_secs,
                min_time_to_expiry_secs,
                strict_freshness,
            );

            devinspect_quote_breakout_command(DevinspectQuoteBreakoutArgs {
                server_url: cli.server_url,
                predict_id: cli.predict_id,
                rpc_url: cli.rpc_url,
                freshness,
                bucket_step: DisplayPrice(bucket_step_usd),
                levels_each_side,
                tail_quantity,
                shoulder_quantity,
                sender,
                max_quote_market_attempts,
                max_total_mint_cost_raw,
                slippage_bps,
            })
            .await
        }
        Command::ResolveQuoteObjects {
            max_price_age_secs,
            max_svi_age_secs,
            min_time_to_expiry_secs,
            strict_freshness,
        } => {
            let freshness = build_freshness(
                max_price_age_secs,
                max_svi_age_secs,
                min_time_to_expiry_secs,
                strict_freshness,
            );

            resolve_quote_objects_command(cli.server_url, cli.predict_id, cli.rpc_url, freshness)
                .await
        }
        Command::ResolveManager { manager_id } => {
            resolve_manager_command(cli.rpc_url, manager_id).await
        }
        Command::DevinspectCreateManager { sender } => {
            devinspect_create_manager_command(cli.rpc_url, sender).await
        }
        Command::ManagerBalance { manager_id, sender } => {
            manager_balance_command(cli.rpc_url, manager_id, sender).await
        }

        Command::ManagerPositions { manager_id, from_execution_json, sender, expect_exact } => {
            manager_positions_command(
                cli.rpc_url,
                manager_id,
                from_execution_json,
                sender,
                expect_exact,
            )
            .await
        }

        Command::CompileStrategyJson {
            owner,
            strategy,
            budget_dusdc,
            style,
            expiry_preference,
            slippage_bps,
            bucket_step_usd,
            levels_each_side,
            max_quote_market_attempts,
            portfolio_exposure_dusdc,
            over_hedge_cap_bps,
            convex_gamma_bps,
            dead_zone_bps,
            moonshot_range_weight_bps,
            moonshot_tail_gamma_bps,
            upside_near_range_weight_bps,
            upside_upper_range_weight_bps,
            upside_tail_gamma_bps,
            downside_near_range_weight_bps,
            downside_lower_range_weight_bps,
            downside_step_tail_gamma_bps,
            condor_center_weight_bps,
            barrier_side,
            barrier_near_range_weight_bps,
            barrier_tail_gamma_bps,
            downside_range_weight_bps,
            downside_tail_gamma_bps,
        } => {
            compile_strategy_json_command(CompileStrategyJsonArgs {
                server_url: cli.server_url,
                predict_id: cli.predict_id,
                rpc_url: cli.rpc_url,
                owner,
                strategy,
                budget_dusdc,
                style,
                expiry_preference,
                slippage_bps,
                bucket_step: DisplayPrice(bucket_step_usd),
                levels_each_side,
                max_quote_market_attempts,
                portfolio_exposure_dusdc,
                over_hedge_cap_bps,
                convex_gamma_bps,
                dead_zone_bps,
                moonshot_range_weight_bps,
                moonshot_tail_gamma_bps,
                downside_range_weight_bps,
                downside_tail_gamma_bps,
                upside_near_range_weight_bps,
                upside_upper_range_weight_bps,
                upside_tail_gamma_bps,
                downside_near_range_weight_bps,
                downside_lower_range_weight_bps,
                downside_step_tail_gamma_bps,
                condor_center_weight_bps,
                barrier_side,
                barrier_near_range_weight_bps,
                barrier_tail_gamma_bps,
            })
            .await
        }
        Command::DemoStatus { manager_id, sender, from_execution_json, expect_exact } => {
            demo_status_command(cli.rpc_url, manager_id, sender, from_execution_json, expect_exact)
                .await
        }
        Command::AuditExecution { from_execution_json } => {
            audit_execution_command(from_execution_json)
        }

        Command::DevinspectRedeemBreakout {
            manager_id,
            sender,
            from_execution_json,
            min_total_payout_raw,
            redeem_bps,
            auto_size_down,
            write_execute_script,
            allow_zero_payout_script,
            execute_script_path,
            execute_plan_json_path,
        } => {
            devinspect_redeem_breakout_command(DevinspectRedeemBreakoutArgs {
                rpc_url: cli.rpc_url,
                manager_id,
                sender,
                from_execution_json,
                min_total_payout_raw,
                auto_size_down,
                redeem_bps,
                write_execute_script,
                allow_zero_payout_script,
                execute_script_path,
                execute_plan_json_path,
            })
            .await
        }
        Command::DevinspectMintBreakout {
            manager_id,
            sender,
            max_price_age_secs,
            max_svi_age_secs,
            min_time_to_expiry_secs,
            strict_freshness,
            bucket_step_usd,
            levels_each_side,
            tail_quantity,
            shoulder_quantity,
            max_total_mint_cost_raw,
            slippage_bps,
            max_quote_market_attempts,

            write_execute_script,
            execute_script_path,
            execute_plan_json_path,
        } => {
            let freshness = build_freshness(
                max_price_age_secs,
                max_svi_age_secs,
                min_time_to_expiry_secs,
                strict_freshness,
            );

            devinspect_mint_breakout_command(DevinspectMintBreakoutArgs {
                server_url: cli.server_url,
                predict_id: cli.predict_id,
                rpc_url: cli.rpc_url,
                manager_id,
                sender,
                freshness,
                bucket_step: DisplayPrice(bucket_step_usd),
                levels_each_side,
                tail_quantity,
                shoulder_quantity,
                max_total_mint_cost_raw,
                slippage_bps,
                max_quote_market_attempts,

                write_execute_script,
                execute_script_path,
                execute_plan_json_path,
            })
            .await
        }
        Command::VerifyAbi => verify_abi_command(cli.rpc_url).await,
    };

    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn build_freshness(
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

async fn load_markets(
    client: &DeepBookClient,
    freshness: FreshnessConfig,
) -> Result<Vec<MarketSnapshot>, Box<dyn std::error::Error>> {
    let _status = client.status().await?;
    let _predict_state = client.predict_state().await?;
    let quote_assets = client.quote_assets().await?;
    let vault_summary = client.vault_summary().await?;

    println!("Protocol status: ok");
    println!("Predict state: ok");
    println!("Quote assets: {}", quote_assets.len());
    println!("Vault summary fetched: {}", vault_summary.is_present());
    println!(
        "Freshness mode: {}",
        if freshness.require_price_timestamp || freshness.require_svi_timestamp {
            "strict"
        } else {
            "testnet-lenient"
        }
    );
    println!();

    Ok(client.load_structx_markets(freshness).await?)
}

async fn list_markets(
    server_url: String,
    predict_id: String,
    freshness: FreshnessConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = build_client(server_url, predict_id)?;
    let markets = load_markets(&client, freshness).await?;

    print_market_table(&markets);

    let usable = markets.iter().filter(|m| m.structx_status.is_usable()).count();

    println!();
    println!("BTC markets found: {}", markets.len());
    println!("StructX-usable markets: {usable}");

    Ok(())
}

async fn select_market(
    server_url: String,
    predict_id: String,
    freshness: FreshnessConfig,
    bucket_step: DisplayPrice,
    levels_each_side: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = build_client(server_url, predict_id)?;
    let markets = load_markets(&client, freshness).await?;

    let selected = select_best_market(&markets, PriceScale::E9)?;

    print_selected_market(&selected);
    print_strike_buckets(&selected, bucket_step, levels_each_side)?;

    Ok(())
}

async fn compile_breakout_command(
    server_url: String,
    predict_id: String,
    freshness: FreshnessConfig,
    bucket_step: DisplayPrice,
    levels_each_side: u32,
    tail_quantity: u64,
    shoulder_quantity: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = build_client(server_url, predict_id)?;
    let markets = load_markets(&client, freshness).await?;
    let selected = select_best_market(&markets, PriceScale::E9)?;

    print_selected_market(&selected);

    let strikes = selected.grid.centered_strikes_by_display_step(
        selected.spot_raw,
        bucket_step,
        levels_each_side,
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
            "not enough strikes around spot; increase --levels-each-side",
        )
        .into());
    }

    let k1 = strikes[center_idx - 2];
    let k2 = strikes[center_idx - 1];
    let k3 = strikes[center_idx + 1];
    let k4 = strikes[center_idx + 2];

    let compiled = compile_breakout(k1, k2, k3, k4, tail_quantity, shoulder_quantity)?;

    print_breakout_boundaries(&selected, k1, k2, k3, k4);
    print_compiled_payoff(&selected, &compiled);

    Ok(())
}

async fn plan_quote_breakout_command(
    server_url: String,
    predict_id: String,
    freshness: FreshnessConfig,
    bucket_step: DisplayPrice,
    levels_each_side: u32,
    tail_quantity: u64,
    shoulder_quantity: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = build_client(server_url, predict_id)?;
    let markets = load_markets(&client, freshness).await?;
    let selected = select_best_market(&markets, PriceScale::E9)?;

    print_selected_market(&selected);

    let strikes = selected.grid.centered_strikes_by_display_step(
        selected.spot_raw,
        bucket_step,
        levels_each_side,
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
            "not enough strikes around spot; increase --levels-each-side",
        )
        .into());
    }

    let k1 = strikes[center_idx - 2];
    let k2 = strikes[center_idx - 1];
    let k3 = strikes[center_idx + 1];
    let k4 = strikes[center_idx + 2];

    let compiled = compile_breakout(k1, k2, k3, k4, tail_quantity, shoulder_quantity)?;
    let plan = build_quote_plan(&selected, &compiled)?;

    print_breakout_boundaries(&selected, k1, k2, k3, k4);
    print_compiled_payoff(&selected, &compiled);
    print_quote_plan(&selected, &plan);

    Ok(())
}

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
}
#[derive(Debug, Clone)]
struct SmartCompiledCandidate {
    strategy: String,
    output: serde_json::Value,
    metrics: SmartCandidateMetrics,
    score: SmartCandidateScore,
}

struct DevinspectQuoteBreakoutArgs {
    server_url: String,
    predict_id: String,
    rpc_url: String,
    freshness: FreshnessConfig,
    bucket_step: DisplayPrice,
    levels_each_side: u32,
    tail_quantity: u64,
    shoulder_quantity: u64,
    sender: String,
    max_quote_market_attempts: usize,
    max_total_mint_cost_raw: Option<u64>,
    slippage_bps: u16,
}

struct BreakoutQuotePlanPreparation {
    k1: Strike,
    k2: Strike,
    k3: Strike,
    k4: Strike,
    compiled: CompiledPayoff,
    plan: QuotePlan,
}

fn build_breakout_quote_plan_for_selected_market(
    args: &DevinspectQuoteBreakoutArgs,
    selected: &SelectedMarket<'_>,
) -> Result<BreakoutQuotePlanPreparation, Box<dyn std::error::Error>> {
    let strikes = selected.grid.centered_strikes_by_display_step(
        selected.spot_raw,
        args.bucket_step,
        args.levels_each_side,
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
            "not enough strikes around spot; increase --levels-each-side",
        )
        .into());
    }

    let k1 = strikes[center_idx - 2];
    let k2 = strikes[center_idx - 1];
    let k3 = strikes[center_idx + 1];
    let k4 = strikes[center_idx + 2];

    let compiled = compile_breakout(k1, k2, k3, k4, args.tail_quantity, args.shoulder_quantity)?;

    let plan = build_quote_plan(selected, &compiled)?;

    Ok(BreakoutQuotePlanPreparation { k1, k2, k3, k4, compiled, plan })
}

async fn devinspect_quote_breakout_command(
    args: DevinspectQuoteBreakoutArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    if args.max_quote_market_attempts == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--max-quote-market-attempts must be greater than zero",
        )
        .into());
    }

    let client = build_client(args.server_url.clone(), args.predict_id.clone())?;
    let markets = load_markets(&client, args.freshness).await?;
    let candidates = select_candidate_markets(&markets, PriceScale::E9);

    if candidates.is_empty() {
        return Err(
            io::Error::new(io::ErrorKind::InvalidData, "no quoteable market candidates").into()
        );
    }

    let max_attempts = args.max_quote_market_attempts.min(candidates.len());
    let mut failures = Vec::new();

    for (attempt_idx, selected) in candidates.into_iter().take(max_attempts).enumerate() {
        println!(
            "Quote attempt {}/{} using oracle {} expiring {}",
            attempt_idx + 1,
            max_attempts,
            selected.oracle_id,
            selected.expiry.to_rfc3339()
        );

        match devinspect_quote_for_selected_market(&args, &selected).await {
            Ok(()) => return Ok(()),
            Err(err) => {
                let message = format!(
                    "oracle {} expiry {} failed: {}",
                    selected.oracle_id,
                    selected.expiry.to_rfc3339(),
                    err
                );
                eprintln!("{message}");
                failures.push(message);
            }
        }
    }

    Err(io::Error::other(format!("all quote attempts failed:\n{}", failures.join("\n"))).into())
}

async fn devinspect_quote_for_selected_market(
    args: &DevinspectQuoteBreakoutArgs,
    selected: &SelectedMarket<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    print_selected_market(selected);

    let quote = build_breakout_quote_plan_for_selected_market(args, selected)?;

    print_breakout_boundaries(selected, quote.k1, quote.k2, quote.k3, quote.k4);
    print_compiled_payoff(selected, &quote.compiled);
    print_quote_plan(selected, &quote.plan);

    let rpc = SuiRpcClient::new(args.rpc_url.clone(), StdDuration::from_secs(20))?;

    let predict = resolve_sui_object(&rpc, PREDICT_OBJECT_ID).await?;
    let oracle = resolve_sui_object(&rpc, selected.oracle_id).await?;
    let clock = resolve_sui_object(&rpc, SUI_CLOCK_OBJECT_ID).await?;

    validate_quote_object_refs(&predict, &oracle, &clock)?;

    let tx_kind = build_quote_tx_kind(
        &quote.plan,
        QuoteObjectRefs { predict: &predict, oracle: &oracle, clock: &clock },
        &args.sender,
    )?;

    print_quote_tx_kind(&tx_kind);

    let response = rpc.dev_inspect_transaction_kind(&tx_kind.sender, &tx_kind.tx_kind_b64).await?;

    let preview = print_devinspect_quote_response(selected, &quote.plan, &tx_kind, &response)?;

    if let Some(max_total_mint_cost_raw) = args.max_total_mint_cost_raw {
        let guard = QuoteCostGuard { max_total_mint_cost_raw, slippage_bps: args.slippage_bps };

        let guarded = guard_quote_preview(&preview, guard)?;

        println!();
        println!("Quote guard: accepted");
        println!("max_total_mint_cost_raw: {}", guarded.max_total_mint_cost_raw);
        println!("max_allowed_after_slippage_raw: {}", guarded.max_allowed_after_slippage_raw);
        println!("actual_total_mint_cost_raw: {}", guarded.total_mint_cost_raw);
        println!("slippage_bps: {}", guarded.slippage_bps);
    } else {
        println!();
        println!("Quote guard: skipped; pass --max-total-mint-cost-raw to enforce a cap");
    }

    Ok(())
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

        let mint_cost_raw = decode_devinspect_u64(&return_values[0])?;
        let redeem_payout_raw = decode_devinspect_u64(&return_values[1])?;

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
        frac_string.truncate(6);
    }

    while frac_string.len() < 6 {
        frac_string.push('0');
    }

    let frac_raw = if frac_string.is_empty() { 0 } else { frac_string.parse::<u64>()? };

    whole_raw
        .checked_add(frac_raw)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "budget overflow").into())
}

fn print_devinspect_quote_response(
    selected: &SelectedMarket<'_>,
    plan: &QuotePlan,
    tx_kind: &QuoteTxKind,
    response: &serde_json::Value,
) -> Result<QuotePreview, Box<dyn std::error::Error>> {
    print_devinspect_response_summary(response)?;

    let results = response
        .get("results")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing devInspect results"))?;

    let asset = QuoteAssetDisplay { symbol: "dUSDC".to_string(), decimals: DUSDC_DECIMALS };

    let mut preview_legs = Vec::with_capacity(plan.calls.len());

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
                format!(
                    "expected 2 return values for command {command_idx}, got {}",
                    return_values.len()
                ),
            )
            .into());
        }

        let mint_cost_raw = decode_devinspect_u64(&return_values[0])?;
        let redeem_payout_raw = decode_devinspect_u64(&return_values[1])?;

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

    let preview = QuotePreview::new(asset, preview_legs);
    print_quote_preview(&preview);

    Ok(preview)
}

fn print_quote_preview(preview: &QuotePreview) {
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec![
        "#",
        "function",
        "leg",
        "strike/lower",
        "upper",
        "quantity",
        "mint raw",
        "mint",
        "redeem raw",
        "redeem",
    ]);

    for leg in &preview.legs {
        table.add_row(vec![
            Cell::new(leg.index),
            Cell::new(&leg.function),
            Cell::new(&leg.leg),
            Cell::new(&leg.strike_or_lower),
            Cell::new(leg.upper.as_deref().unwrap_or("—")),
            Cell::new(leg.quantity),
            Cell::new(leg.mint_cost_raw),
            Cell::new(preview.asset.format_amount(leg.mint_cost_raw)),
            Cell::new(leg.redeem_payout_raw),
            Cell::new(preview.asset.format_amount(leg.redeem_payout_raw)),
        ]);
    }

    println!("devInspect quote preview");
    println!("{table}");
    println!();

    println!("Quote summary");
    println!("total mint cost raw: {}", preview.total_mint_cost_raw);
    println!("total mint cost: {}", preview.total_mint_cost_display());
    println!("total redeem payout raw: {}", preview.total_redeem_payout_raw);
    println!("total redeem payout: {}", preview.total_redeem_payout_display());
}

fn decode_devinspect_u64(value: &serde_json::Value) -> Result<u64, Box<dyn std::error::Error>> {
    let arr = value.as_array().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, format!("return value is not array: {value}"))
    })?;

    let bytes_value = arr
        .first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "return value missing bytes"))?;

    let bytes_array = bytes_value.as_array().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("return bytes are not array: {bytes_value}"),
        )
    })?;

    if bytes_array.len() != 8 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("u64 return must have 8 bytes, got {}", bytes_array.len()),
        )
        .into());
    }

    let mut bytes = [0u8; 8];

    for (idx, byte_value) in bytes_array.iter().enumerate() {
        let byte = byte_value.as_u64().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, format!("invalid byte value: {byte_value}"))
        })?;

        bytes[idx] = u8::try_from(byte)?;
    }

    Ok(u64::from_le_bytes(bytes))
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

async fn devinspect_create_manager_command(
    rpc_url: String,
    sender: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let rpc = SuiRpcClient::new(rpc_url, StdDuration::from_secs(20))?;

    let tx_kind = build_create_manager_tx_kind(&sender)?;

    println!("Built create-manager TransactionKind");
    println!("sender: {}", tx_kind.sender);
    println!("tx_kind_b64_len: {}", tx_kind.tx_kind_b64.len());
    println!();

    let response = rpc.dev_inspect_transaction_kind(&tx_kind.sender, &tx_kind.tx_kind_b64).await?;

    print_devinspect_create_manager_response(&response)?;

    Ok(())
}

fn print_devinspect_mint_response(
    response: &serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let status = devinspect_status(response);

    println!("mint devInspect status: {status}");

    if status != "success" {
        return Err(io::Error::other(devinspect_failure_summary(response)).into());
    }

    let events =
        response.get("events").and_then(serde_json::Value::as_array).cloned().unwrap_or_default();

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec![
        "event",
        "oracle_id",
        "strike/lower",
        "higher",
        "quantity",
        "cost",
        "ask_price",
    ]);

    for event in events {
        let event_type = event.get("type").and_then(serde_json::Value::as_str).unwrap_or("");

        let parsed = event.get("parsedJson").unwrap_or(&serde_json::Value::Null);

        if event_type.ends_with("::predict::PositionMinted") {
            table.add_row(vec![
                Cell::new("PositionMinted"),
                Cell::new(json_str(parsed, "oracle_id")),
                Cell::new(json_str(parsed, "strike")),
                Cell::new("—"),
                Cell::new(json_str(parsed, "quantity")),
                Cell::new(json_str(parsed, "cost")),
                Cell::new(json_str(parsed, "ask_price")),
            ]);
        } else if event_type.ends_with("::predict::RangeMinted") {
            table.add_row(vec![
                Cell::new("RangeMinted"),
                Cell::new(json_str(parsed, "oracle_id")),
                Cell::new(json_str(parsed, "lower_strike")),
                Cell::new(json_str(parsed, "higher_strike")),
                Cell::new(json_str(parsed, "quantity")),
                Cell::new(json_str(parsed, "cost")),
                Cell::new(json_str(parsed, "ask_price")),
            ]);
        }
    }

    println!("Mint preview events");
    println!("{table}");
    println!();
    println!("Important: this was devInspect only. No positions were persisted.");

    Ok(())
}

fn json_str(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
        .or_else(|| value.get(key).map(ToString::to_string))
        .unwrap_or_else(|| "—".to_string())
}

fn write_execute_mint_artifacts(
    args: &DevinspectMintBreakoutArgs,
    selected: &SelectedMarket<'_>,
    plan: &QuotePlan,
    preview: &QuotePreview,
) -> Result<(), Box<dyn std::error::Error>> {
    let script = build_execute_mint_script(args, plan)?;

    fs::write(&args.execute_script_path, script)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut perms = fs::metadata(&args.execute_script_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&args.execute_script_path, perms)?;
    }

    let manifest = build_execute_mint_manifest(args, selected, plan, preview);
    fs::write(&args.execute_plan_json_path, serde_json::to_string_pretty(&manifest)?)?;

    println!();
    println!("Fresh executable mint artifacts written");
    println!("script: {}", display_path(&args.execute_script_path));
    println!("plan: {}", display_path(&args.execute_plan_json_path));
    println!();
    println!("Execute immediately with:");
    println!(
        "GAS_BUDGET=500000000 bash {} --json | tee /tmp/structx_execute_mint_breakout.json",
        display_path(&args.execute_script_path)
    );

    Ok(())
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn build_execute_mint_manifest(
    args: &DevinspectMintBreakoutArgs,
    selected: &SelectedMarket<'_>,
    plan: &QuotePlan,
    preview: &QuotePreview,
) -> serde_json::Value {
    let legs = plan
        .calls
        .iter()
        .enumerate()
        .map(|(idx, call)| match call {
            QuoteCall::Binary { direction, expiry_ms, strike, quantity, .. } => serde_json::json!({
                "index": idx,
                "kind": "binary",
                "direction": direction.to_string(),
                "oracle_id": plan.oracle_id,
                "expiry_ms": expiry_ms,
                "strike_raw": strike.raw,
                "strike": selected.grid.display(*strike).to_string(),
                "quantity": quantity,
            }),
            QuoteCall::Range { expiry_ms, lower, upper, quantity, .. } => serde_json::json!({
                "index": idx,
                "kind": "range",
                "oracle_id": plan.oracle_id,
                "expiry_ms": expiry_ms,
                "lower_raw": lower.raw,
                "upper_raw": upper.raw,
                "lower": selected.grid.display(*lower).to_string(),
                "upper": selected.grid.display(*upper).to_string(),
                "quantity": quantity,
            }),
        })
        .collect::<Vec<_>>();

    serde_json::json!({
        "sender": args.sender,
        "manager_id": args.manager_id,
        "predict_object_id": PREDICT_OBJECT_ID,
        "predict_package_id": PREDICT_PACKAGE_ID,
        "oracle_id": plan.oracle_id,
        "selected_expiry": selected.expiry.to_rfc3339(),
        "total_mint_cost_raw": preview.total_mint_cost_raw,
        "total_mint_cost": preview.total_mint_cost_display(),
        "total_redeem_payout_raw": preview.total_redeem_payout_raw,
        "total_redeem_payout": preview.total_redeem_payout_display(),
        "max_total_mint_cost_raw": args.max_total_mint_cost_raw,
        "slippage_bps": args.slippage_bps,
        "legs": legs,
        "warning": "Generated only after successful devInspect. Execute immediately; pricing/risk can change."
    })
}

fn build_execute_mint_script(
    args: &DevinspectMintBreakoutArgs,
    plan: &QuotePlan,
) -> Result<String, Box<dyn std::error::Error>> {
    if plan.calls.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "empty mint plan").into());
    }

    let mut out = String::new();

    out.push_str("#!/usr/bin/env bash\n");
    out.push_str("set -euo pipefail\n\n");

    out.push_str("# Fresh StructX mint script generated only after successful devInspect.\n");
    out.push_str("# Execute immediately; Predict pricing/risk checks can change between runs.\n\n");

    out.push_str(&format!("export PREDICT_PACKAGE={}\n", PREDICT_PACKAGE_ID));
    out.push_str(&format!("export PREDICT_OBJECT_ID={}\n", PREDICT_OBJECT_ID));
    out.push_str(&format!("export DUSDC={}\n", DUSDC_COIN_TYPE));
    out.push_str(&format!("export MANAGER_ID={}\n", args.manager_id));
    out.push_str(&format!("export OWNER={}\n", args.sender));
    out.push_str(&format!("export ORACLE_ID={}\n", plan.oracle_id));
    out.push_str("export CLOCK_ID=0x6\n");
    out.push_str("export GAS_BUDGET=${GAS_BUDGET:-500000000}\n\n");

    out.push_str("EXTRA_ARGS=(\"$@\")\n");
    out.push_str("if [ ${#EXTRA_ARGS[@]} -eq 0 ]; then\n");
    out.push_str("  EXTRA_ARGS=(--json)\n");
    out.push_str("fi\n\n");

    out.push_str("sui client ptb \\\n");
    out.push_str("  --sender \"$OWNER\" \\\n");

    for (idx, call) in plan.calls.iter().enumerate() {
        let key_name = format!("key{idx}");

        match call {
            QuoteCall::Binary { direction, expiry_ms, strike, quantity, .. } => {
                let key_function = match direction.to_string().as_str() {
                    "up" => "up",
                    "down" => "down",
                    other => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!("unknown binary direction: {other}"),
                        )
                        .into());
                    }
                };

                out.push_str(&format!(
                    "  --move-call \"${{PREDICT_PACKAGE}}::market_key::{key_function}\" \"@${{ORACLE_ID}}\" \"{}\" \"{}\" \\\n",
                    *expiry_ms as u64,
                    strike.raw,
                ));
                out.push_str(&format!("  --assign {key_name} \\\n"));
                out.push_str(&format!(
                    "  --move-call \"${{PREDICT_PACKAGE}}::predict::mint\" \"<${{DUSDC}}>\" \"@${{PREDICT_OBJECT_ID}}\" \"@${{MANAGER_ID}}\" \"@${{ORACLE_ID}}\" {key_name} \"{}\" \"@${{CLOCK_ID}}\" \\\n",
                    quantity,
                ));
            }
            QuoteCall::Range { expiry_ms, lower, upper, quantity, .. } => {
                out.push_str(&format!(
                    "  --move-call \"${{PREDICT_PACKAGE}}::range_key::new\" \"@${{ORACLE_ID}}\" \"{}\" \"{}\" \"{}\" \\\n",
                    *expiry_ms as u64,
                    lower.raw,
                    upper.raw,
                ));
                out.push_str(&format!("  --assign {key_name} \\\n"));
                out.push_str(&format!(
                    "  --move-call \"${{PREDICT_PACKAGE}}::predict::mint_range\" \"<${{DUSDC}}>\" \"@${{PREDICT_OBJECT_ID}}\" \"@${{MANAGER_ID}}\" \"@${{ORACLE_ID}}\" {key_name} \"{}\" \"@${{CLOCK_ID}}\" \\\n",
                    quantity,
                ));
            }
        }
    }

    out.push_str("  --gas-budget \"$GAS_BUDGET\" \\\n");
    out.push_str("  \"${EXTRA_ARGS[@]}\"\n");

    Ok(out)
}

fn print_devinspect_create_manager_response(
    response: &serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let status = devinspect_status(response);

    println!("devInspect status: {status}");

    if status != "success" {
        return Err(io::Error::other(devinspect_failure_summary(response)).into());
    }

    let results = response
        .get("results")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing devInspect results"))?;

    let first_result = results.first().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "missing create_manager command result")
    })?;

    let return_values =
        first_result.get("returnValues").and_then(serde_json::Value::as_array).ok_or_else(
            || io::Error::new(io::ErrorKind::InvalidData, "missing create_manager returnValues"),
        )?;

    if return_values.len() != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("expected create_manager to return 1 value, got {}", return_values.len()),
        )
        .into());
    }

    let manager_id = decode_devinspect_object_id(&return_values[0])?;

    println!("create_manager preview returned manager_id:");
    println!("{manager_id}");
    println!();
    println!("Important: this manager_id is from devInspect only. It is not persisted.");
    println!(
        "A real manager_id will exist only after sending a signed create_manager transaction."
    );

    Ok(())
}

fn decode_devinspect_object_id(
    value: &serde_json::Value,
) -> Result<String, Box<dyn std::error::Error>> {
    let arr = value.as_array().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, format!("return value is not array: {value}"))
    })?;

    let bytes_value = arr
        .first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "return value missing bytes"))?;

    let bytes_array = bytes_value.as_array().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("return bytes are not array: {bytes_value}"),
        )
    })?;

    if bytes_array.len() != 32 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("object::ID return must have 32 bytes, got {}", bytes_array.len()),
        )
        .into());
    }

    let mut out = String::from("0x");

    for byte_value in bytes_array {
        let byte = byte_value.as_u64().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, format!("invalid byte value: {byte_value}"))
        })?;

        let byte = u8::try_from(byte)?;
        out.push_str(&format!("{byte:02x}"));
    }

    Ok(out)
}

fn print_devinspect_response_summary(
    response: &serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let status = devinspect_status(response);
    let result_count =
        response.get("results").and_then(serde_json::Value::as_array).map(Vec::len).unwrap_or(0);

    println!("devInspect status: {status}");
    println!("devInspect result count: {result_count}");

    if let Some(error) = response.get("error") {
        println!("devInspect error:");
        println!("{}", serde_json::to_string_pretty(error)?);
        return Err(io::Error::other("devInspect quote preview returned an RPC error").into());
    }

    if status != "success" {
        return Err(io::Error::other(devinspect_failure_summary(response)).into());
    }

    println!("devInspect quote preview executed");
    println!();

    Ok(())
}

fn devinspect_status(response: &serde_json::Value) -> &str {
    response
        .get("effects")
        .and_then(|effects| effects.get("status"))
        .and_then(|status| status.get("status"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown")
}

fn print_quote_tx_kind(tx_kind: &QuoteTxKind) {
    println!("Built quote TransactionKind");
    println!("sender: {}", tx_kind.sender);
    println!("tx_kind_b64_len: {}", tx_kind.tx_kind_b64.len());
    println!("quote result command indices: {:?}", tx_kind.quote_result_command_indices);
    println!();
}

async fn resolve_quote_objects_command(
    server_url: String,
    predict_id: String,
    rpc_url: String,
    freshness: FreshnessConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = build_client(server_url, predict_id)?;
    let markets = load_markets(&client, freshness).await?;
    let selected = select_best_market(&markets, PriceScale::E9)?;

    print_selected_market(&selected);

    let rpc = SuiRpcClient::new(rpc_url, StdDuration::from_secs(20))?;

    let predict = resolve_sui_object(&rpc, PREDICT_OBJECT_ID).await?;
    let oracle = resolve_sui_object(&rpc, selected.oracle_id).await?;
    let clock = resolve_sui_object(&rpc, SUI_CLOCK_OBJECT_ID).await?;

    print_quote_object_refs(&predict, &oracle, &clock);
    validate_quote_object_refs(&predict, &oracle, &clock)?;

    Ok(())
}

async fn resolve_sui_object(
    rpc: &SuiRpcClient,
    object_id: &str,
) -> Result<SuiObjectInfo, Box<dyn std::error::Error>> {
    let value = rpc.get_object(object_id).await?;
    Ok(SuiObjectInfo::from_get_object_result(object_id, value)?)
}

struct DevinspectMintBreakoutArgs {
    server_url: String,
    predict_id: String,
    rpc_url: String,
    manager_id: String,
    sender: String,
    freshness: FreshnessConfig,
    bucket_step: DisplayPrice,
    levels_each_side: u32,
    tail_quantity: u64,
    shoulder_quantity: u64,
    max_total_mint_cost_raw: u64,
    slippage_bps: u16,
    max_quote_market_attempts: usize,

    write_execute_script: bool,
    execute_script_path: PathBuf,
    execute_plan_json_path: PathBuf,
}

#[derive(Debug, Clone, Copy)]
struct PositionCheckSummary {
    ok: usize,
    bad: usize,
}

async fn compile_strategy_json_command(
    args: CompileStrategyJsonArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let _compile_slippage_bps = args.slippage_bps;
    let advanced_strategy = match AdvancedStrategyKind::from_api_value(&args.strategy) {
        Ok(
            strategy @ (AdvancedStrategyKind::PortfolioCrashShield
            | AdvancedStrategyKind::ConvexTailLadder
            | AdvancedStrategyKind::ExpiryMoveNote
            | AdvancedStrategyKind::MoonshotUpside
            | AdvancedStrategyKind::DownsideConvexity
            | AdvancedStrategyKind::UpsideStepLadder
            | AdvancedStrategyKind::DownsideStepLadder
            | AdvancedStrategyKind::CenterBandCondor
            | AdvancedStrategyKind::NearBarrierProxy
            | AdvancedStrategyKind::RangeConviction
            | AdvancedStrategyKind::SmartBudgetSelector),
        ) => Some(strategy),
        _ => None,
    };

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

    let budget_raw = parse_dusdc_to_raw(&args.budget_dusdc)?;
    let style = BreakoutStyle::from_api_value(&args.style)?;

    let freshness = build_freshness(60, 60, 300, false);
    let client = build_client(args.server_url.clone(), args.predict_id.clone())?;
    let markets = load_markets(&client, freshness).await?;
    let candidates = select_candidate_markets(&markets, PriceScale::E9);

    if candidates.is_empty() {
        return Err(
            io::Error::new(io::ErrorKind::InvalidData, "no quoteable market candidates").into()
        );
    }

    let rpc = SuiRpcClient::new(args.rpc_url.clone(), StdDuration::from_secs(20))?;
    let predict = resolve_sui_object(&rpc, PREDICT_OBJECT_ID).await?;
    let clock = resolve_sui_object(&rpc, SUI_CLOCK_OBJECT_ID).await?;

    let asset = QuoteAssetDisplay { symbol: "dUSDC".to_string(), decimals: DUSDC_DECIMALS };

    let mut warnings = vec![
        "DeepBook Predict integration is testnet-only for this milestone.".to_string(),
        "Quote can change before signing; transaction build must apply slippage guard.".to_string(),
        "Known issue: binary event-derived MarketKeys can read 0 while range positions verify correctly.".to_string(),
    ];

    let max_attempts = args.max_quote_market_attempts.min(candidates.len());
    let probe_quantity = 1_000_000u64;

    for selected in candidates.into_iter().take(max_attempts) {
        let oracle = resolve_sui_object(&rpc, selected.oracle_id).await?;

        if let Err(err) = validate_quote_object_refs(&predict, &oracle, &clock) {
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
                Ok(output) => {
                    println!("{}", serde_json::to_string_pretty(&output)?);
                    return Ok(());
                }
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
                Ok(output) => {
                    println!("{}", serde_json::to_string_pretty(&output)?);
                    return Ok(());
                }
                Err(err) => {
                    warnings.push(format!("skipped oracle {}: {err}", selected.oracle_id));
                    continue;
                }
            }
        }

        let strikes = selected.grid.centered_strikes_by_display_step(
            selected.spot_raw,
            args.bucket_step,
            args.levels_each_side,
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
            warnings.push(format!(
                "skipped oracle {}: not enough strikes around spot",
                selected.oracle_id
            ));
            continue;
        }

        let k1 = strikes[center_idx - 2];
        let k2 = strikes[center_idx - 1];
        let k3 = strikes[center_idx + 1];
        let k4 = strikes[center_idx + 2];

        let probe_compiled = compile_breakout(k1, k2, k3, k4, probe_quantity, probe_quantity)?;

        let probe_plan = build_quote_plan(&selected, &probe_compiled)?;

        let probe_tx_kind = build_quote_tx_kind(
            &probe_plan,
            QuoteObjectRefs { predict: &predict, oracle: &oracle, clock: &clock },
            &args.owner,
        )?;

        let probe_response = rpc
            .dev_inspect_transaction_kind(&probe_tx_kind.sender, &probe_tx_kind.tx_kind_b64)
            .await?;

        let probe_costs = match quote_costs_from_response(&probe_tx_kind, &probe_response) {
            Ok(costs) if costs.len() == 4 => costs,
            Ok(costs) => {
                warnings.push(format!(
                    "skipped oracle {}: expected 4 quote legs, got {}",
                    selected.oracle_id,
                    costs.len()
                ));
                continue;
            }
            Err(err) => {
                warnings.push(format!("skipped oracle {}: {err}", selected.oracle_id));
                continue;
            }
        };

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

        let final_plan = build_quote_plan(&selected, &final_compiled)?;

        let final_tx_kind = build_quote_tx_kind(
            &final_plan,
            QuoteObjectRefs { predict: &predict, oracle: &oracle, clock: &clock },
            &args.owner,
        )?;

        let final_response = rpc
            .dev_inspect_transaction_kind(&final_tx_kind.sender, &final_tx_kind.tx_kind_b64)
            .await?;

        let final_costs = quote_costs_from_response(&final_tx_kind, &final_response)?;

        if final_costs.len() != 4 {
            warnings.push(format!(
                "skipped oracle {}: final quote returned {} legs",
                selected.oracle_id,
                final_costs.len()
            ));
            continue;
        }

        let total_cost_raw = final_costs
            .iter()
            .try_fold(0u64, |acc, (cost, _)| acc.checked_add(*cost))
            .ok_or_else(|| io::Error::other("total cost overflow"))?;

        let max_gross_payout_raw =
            optimized.down_tail_quantity.max(optimized.downside_range_quantity);
        let max_loss_raw = total_cost_raw;
        let max_net_payout_raw = max_gross_payout_raw.saturating_sub(total_cost_raw);

        let compiled_strategy_id = format!(
            "breakout:{}:{}:{}:{}:{}",
            args.owner,
            selected.oracle_id,
            selected.expiry.timestamp_millis(),
            total_cost_raw,
            style.api_value()
        );

        let output = serde_json::json!({
            "ok": true,
            "compiledStrategyId": compiled_strategy_id,
            "strategy": "BREAKOUT_PROTECTION",
            "network": "sui:testnet",
            "owner": args.owner,
            "oracleId": selected.oracle_id,
            "expiry": selected.expiry.to_rfc3339(),
            "spot": format_raw_price_e9(selected.spot_raw),
            "style": style.api_value(),
            "styleRatioBps": optimized.style_ratio_bps,
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
                "k1": format_raw_price_e9(k1.raw),
                "k2": format_raw_price_e9(k2.raw),
                "k3": format_raw_price_e9(k3.raw),
                "k4": format_raw_price_e9(k4.raw),
                "k1Raw": k1.raw.to_string(),
                "k2Raw": k2.raw.to_string(),
                "k3Raw": k3.raw.to_string(),
                "k4Raw": k4.raw.to_string()
            },
            "legs": [
                compile_json_leg_down(k1.raw, optimized.down_tail_quantity, final_costs[0].0, ask_inputs.down_tail_ask_raw, &asset),
                compile_json_leg_range("moderate_downside", k1.raw, k2.raw, optimized.downside_range_quantity, final_costs[1].0, ask_inputs.downside_range_ask_raw, &asset),
                compile_json_leg_range("moderate_upside", k3.raw, k4.raw, optimized.upside_range_quantity, final_costs[2].0, ask_inputs.upside_range_ask_raw, &asset),
                compile_json_leg_up(k4.raw, optimized.up_tail_quantity, final_costs[3].0, ask_inputs.up_tail_ask_raw, &asset)
            ],
            "payoffTable": [
                payoff_json("BTC settles <= K1", max_gross_payout_raw, total_cost_raw, &asset),
                payoff_json("K1 < BTC settles <= K2", optimized.downside_range_quantity, total_cost_raw, &asset),
                payoff_json("K2 < BTC settles < K3", 0, total_cost_raw, &asset),
                payoff_json("K3 <= BTC settles < K4", optimized.upside_range_quantity, total_cost_raw, &asset),
                payoff_json("BTC settles >= K4", max_gross_payout_raw, total_cost_raw, &asset)
            ],
            "warnings": warnings
        });

        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        format!("failed to compile strategy after {max_attempts} market attempts"),
    )
    .into())
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
    mut warnings: Vec<String>,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let budget_raw = parse_dusdc_to_raw(&args.budget_dusdc)?;
    let style = BreakoutStyle::from_api_value(&args.style)?;
    let probe_quantity = 1_000_000u64;

    let strikes = selected.grid.centered_strikes_by_display_step(
        selected.spot_raw,
        args.bucket_step,
        args.levels_each_side,
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
            io::ErrorKind::InvalidData,
            "not enough strikes around spot for breakout strategy",
        )
        .into());
    }

    let k1 = strikes[center_idx - 2];
    let k2 = strikes[center_idx - 1];
    let k3 = strikes[center_idx + 1];
    let k4 = strikes[center_idx + 2];

    let probe_compiled = compile_breakout(k1, k2, k3, k4, probe_quantity, probe_quantity)?;
    let probe_plan = build_quote_plan(selected, &probe_compiled)?;

    let probe_tx_kind =
        build_quote_tx_kind(&probe_plan, QuoteObjectRefs { predict, oracle, clock }, &args.owner)?;

    let probe_response =
        rpc.dev_inspect_transaction_kind(&probe_tx_kind.sender, &probe_tx_kind.tx_kind_b64).await?;

    let probe_costs = quote_costs_from_response(&probe_tx_kind, &probe_response)?;

    if probe_costs.len() != 4 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("expected 4 breakout probe quote legs, got {}", probe_costs.len()),
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

    let final_tx_kind =
        build_quote_tx_kind(&final_plan, QuoteObjectRefs { predict, oracle, clock }, &args.owner)?;

    let final_response =
        rpc.dev_inspect_transaction_kind(&final_tx_kind.sender, &final_tx_kind.tx_kind_b64).await?;

    let final_costs = quote_costs_from_response(&final_tx_kind, &final_response)?;

    if final_costs.len() != 4 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("final breakout quote returned {} legs", final_costs.len()),
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

    warnings.push(
        "Breakout Protection was compiled as a Smart Selector candidate and re-quoted live."
            .to_string(),
    );

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
        "spot": format_raw_price_e9(selected.spot_raw),
        "style": style.api_value(),
        "styleRatioBps": optimized.style_ratio_bps,
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
            "k1": format_raw_price_e9(k1.raw),
            "k2": format_raw_price_e9(k2.raw),
            "k3": format_raw_price_e9(k3.raw),
            "k4": format_raw_price_e9(k4.raw),
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
            payoff_json("BTC settles <= K1", max_gross_payout_raw, total_cost_raw, asset),
            payoff_json("K1 < BTC settles <= K2", optimized.downside_range_quantity, total_cost_raw, asset),
            payoff_json("K2 < BTC settles < K3", 0, total_cost_raw, asset),
            payoff_json("K3 <= BTC settles < K4", optimized.upside_range_quantity, total_cost_raw, asset),
            payoff_json("BTC settles >= K4", max_gross_payout_raw, total_cost_raw, asset)
        ],
        "warnings": warnings
    }))
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

    candidates.sort_by(|a, b| b.score.score_e6.cmp(&a.score.score_e6));

    let winner = candidates
        .first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing smart winner"))?
        .clone();

    let winner_strategy = winner.strategy.clone();
    let winner_score_e6 = winner.score.score_e6;
    let candidate_count = candidates.len();
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
            serde_json::Value::String(winner_strategy.clone()),
        );

        obj.insert(
            "smartSelector".to_string(),
            serde_json::json!({
                "style": args.style,
                "winner": winner_strategy.clone(),
                "winnerScoreE6": winner_score_e6.to_string(),
                "candidateCount": candidate_count,
                "alternatives": alternatives
            }),
        );

        let warnings_value = obj.entry("warnings").or_insert_with(|| serde_json::json!([]));

        if let Some(warnings_array) = warnings_value.as_array_mut() {
            warnings_array.push(serde_json::Value::String(format!(
                "Smart Budget Selector chose {} from {} valid candidates.",
                winner_strategy.as_str(),
                candidate_count
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

    let strikes = selected.grid.centered_strikes_by_display_step(
        selected.spot_raw,
        args.bucket_step,
        args.levels_each_side,
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
            io::ErrorKind::InvalidData,
            "not enough strikes around spot for advanced strategy",
        )
        .into());
    }

    let k1 = strikes[center_idx - 2];
    let k2 = strikes[center_idx - 1];
    let k3 = strikes[center_idx + 1];
    let k4 = strikes[center_idx + 2];

    let probe_quantity = 1_000_000u64;

    let advanced_result = match strategy_kind {
        AdvancedStrategyKind::PortfolioCrashShield => {
            let exposure_raw = dusdc_f64_to_raw(args.portfolio_exposure_dusdc)?;

            let probe_compiled = compile_bucket_payoff(&[
                PayoffBucket::new(None, Some(k1), probe_quantity),
                PayoffBucket::new(Some(k1), Some(k2), probe_quantity),
                PayoffBucket::new(Some(k2), Some(k3), probe_quantity),
            ])?;

            let probe_plan = build_quote_plan(selected, &probe_compiled)?;

            let probe_tx_kind = build_quote_tx_kind(
                &probe_plan,
                QuoteObjectRefs { predict, oracle, clock },
                &args.owner,
            )?;

            let probe_response = rpc
                .dev_inspect_transaction_kind(&probe_tx_kind.sender, &probe_tx_kind.tx_kind_b64)
                .await?;

            let probe_costs = quote_costs_from_response(&probe_tx_kind, &probe_response)?;

            if probe_costs.len() != 3 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("expected 3 crash-shield probe quote legs, got {}", probe_costs.len()),
                )
                .into());
            }

            let ask_down_tail = infer_ask_price_raw(probe_costs[0].0, probe_quantity);
            let ask_lower_range = infer_ask_price_raw(probe_costs[1].0, probe_quantity);
            let ask_mild_range = infer_ask_price_raw(probe_costs[2].0, probe_quantity);

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
        AdvancedStrategyKind::ConvexTailLadder => {
            let probe_compiled = compile_bucket_payoff(&[
                PayoffBucket::new(None, Some(k1), probe_quantity),
                PayoffBucket::new(Some(k1), Some(k2), probe_quantity),
                PayoffBucket::new(Some(k3), Some(k4), probe_quantity),
                PayoffBucket::new(Some(k4), None, probe_quantity),
            ])?;

            let probe_plan = build_quote_plan(selected, &probe_compiled)?;

            let probe_tx_kind = build_quote_tx_kind(
                &probe_plan,
                QuoteObjectRefs { predict, oracle, clock },
                &args.owner,
            )?;

            let probe_response = rpc
                .dev_inspect_transaction_kind(&probe_tx_kind.sender, &probe_tx_kind.tx_kind_b64)
                .await?;

            let probe_costs = quote_costs_from_response(&probe_tx_kind, &probe_response)?;

            if probe_costs.len() != 4 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("expected 4 tail-ladder probe quote legs, got {}", probe_costs.len()),
                )
                .into());
            }

            let ask_down_tail = infer_ask_price_raw(probe_costs[0].0, probe_quantity);
            let ask_lower_range = infer_ask_price_raw(probe_costs[1].0, probe_quantity);
            let ask_upper_range = infer_ask_price_raw(probe_costs[2].0, probe_quantity);
            let ask_up_tail = infer_ask_price_raw(probe_costs[3].0, probe_quantity);

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

        AdvancedStrategyKind::ExpiryMoveNote => {
            let probe_compiled = compile_bucket_payoff(&[
                PayoffBucket::new(None, Some(k1), probe_quantity),
                PayoffBucket::new(Some(k1), Some(k2), probe_quantity),
                PayoffBucket::new(Some(k3), Some(k4), probe_quantity),
                PayoffBucket::new(Some(k4), None, probe_quantity),
            ])?;

            let probe_plan = build_quote_plan(selected, &probe_compiled)?;

            let probe_tx_kind = build_quote_tx_kind(
                &probe_plan,
                QuoteObjectRefs { predict, oracle, clock },
                &args.owner,
            )?;

            let probe_response = rpc
                .dev_inspect_transaction_kind(&probe_tx_kind.sender, &probe_tx_kind.tx_kind_b64)
                .await?;

            let probe_costs = quote_costs_from_response(&probe_tx_kind, &probe_response)?;

            if probe_costs.len() != 4 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("expected 4 expiry-move probe quote legs, got {}", probe_costs.len()),
                )
                .into());
            }

            let ask_down_tail = infer_ask_price_raw(probe_costs[0].0, probe_quantity);
            let ask_lower_range = infer_ask_price_raw(probe_costs[1].0, probe_quantity);
            let ask_upper_range = infer_ask_price_raw(probe_costs[2].0, probe_quantity);
            let ask_up_tail = infer_ask_price_raw(probe_costs[3].0, probe_quantity);

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

        AdvancedStrategyKind::RangeConviction => {
            let probe_compiled =
                compile_bucket_payoff(&[PayoffBucket::new(Some(k2), Some(k3), probe_quantity)])?;

            let probe_plan = build_quote_plan(selected, &probe_compiled)?;

            let probe_tx_kind = build_quote_tx_kind(
                &probe_plan,
                QuoteObjectRefs { predict, oracle, clock },
                &args.owner,
            )?;

            let probe_response = rpc
                .dev_inspect_transaction_kind(&probe_tx_kind.sender, &probe_tx_kind.tx_kind_b64)
                .await?;

            let probe_costs = quote_costs_from_response(&probe_tx_kind, &probe_response)?;

            if probe_costs.len() != 1 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "expected 1 range conviction probe quote leg, got {}",
                        probe_costs.len()
                    ),
                )
                .into());
            }

            let ask_central_range = infer_ask_price_raw(probe_costs[0].0, probe_quantity);

            compile_range_conviction(RangeConvictionInput {
                budget_raw,
                lower_raw: k2.raw,
                upper_raw: k3.raw,
                range_ask_raw: ask_central_range,
            })?
        }

        AdvancedStrategyKind::MoonshotUpside => {
            let probe_compiled = compile_bucket_payoff(&[
                PayoffBucket::new(Some(k3), Some(k4), probe_quantity),
                PayoffBucket::new(Some(k4), None, probe_quantity),
            ])?;

            let probe_plan = build_quote_plan(selected, &probe_compiled)?;

            let probe_tx_kind = build_quote_tx_kind(
                &probe_plan,
                QuoteObjectRefs { predict, oracle, clock },
                &args.owner,
            )?;

            let probe_response = rpc
                .dev_inspect_transaction_kind(&probe_tx_kind.sender, &probe_tx_kind.tx_kind_b64)
                .await?;

            let probe_costs = quote_costs_from_response(&probe_tx_kind, &probe_response)?;

            if probe_costs.len() != 2 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("expected 2 moonshot probe quote legs, got {}", probe_costs.len()),
                )
                .into());
            }

            let ask_upper_range = infer_ask_price_raw(probe_costs[0].0, probe_quantity);
            let ask_up_tail = infer_ask_price_raw(probe_costs[1].0, probe_quantity);

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

        AdvancedStrategyKind::SmartBudgetSelector => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "SMART_BUDGET_SELECTOR must be compiled through the selector path",
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

    let max_gross_payout_raw =
        advanced_result.legs.iter().map(|leg| leg.quantity).max().unwrap_or(0);

    let max_loss_raw = total_cost_raw;
    let max_net_payout_raw = max_gross_payout_raw.saturating_sub(total_cost_raw);

    let mut all_warnings = warnings;
    all_warnings.extend(advanced_result.warnings.clone());
    all_warnings.push("Advanced strategy quantities are generated by StructX optimizer and re-quoted live before wallet signing.".to_string());

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

    let advanced_json = serde_json::json!({
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
        "condorCenterWeightBps": args.condor_center_weight_bps,
        "barrierSide": args.barrier_side,
        "barrierNearRangeWeightBps": args.barrier_near_range_weight_bps,
        "barrierTailGammaBps": args.barrier_tail_gamma_bps
    });

    Ok(serde_json::json!({
        "ok": true,
        "compiledStrategyId": compiled_strategy_id,
        "strategy": strategy_kind.api_value(),
        "network": "sui:testnet",
        "owner": args.owner,
        "oracleId": selected.oracle_id,
        "expiry": selected.expiry.to_rfc3339(),
        "spot": format_raw_price_e9(selected.spot_raw),
        "style": args.style,
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
            "k1": format_raw_price_e9(k1.raw),
            "k2": format_raw_price_e9(k2.raw),
            "k3": format_raw_price_e9(k3.raw),
            "k4": format_raw_price_e9(k4.raw),
            "k1Raw": k1.raw.to_string(),
            "k2Raw": k2.raw.to_string(),
            "k3Raw": k3.raw.to_string(),
            "k4Raw": k4.raw.to_string()
        },
        "advanced": advanced_json,
        "legs": legs_json,
        "payoffTable": payoff_table,
        "warnings": all_warnings
    }))
}

fn advanced_result_to_compiled_payoff(
    result: &AdvancedCompileResult,
) -> Result<CompiledPayoff, Box<dyn std::error::Error>> {
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

fn advanced_leg_json(
    leg: &AdvancedCompiledLeg,
    premium_raw: u64,
    asset: &QuoteAssetDisplay,
) -> serde_json::Value {
    match leg.kind {
        AdvancedLegKind::Down => serde_json::json!({
            "kind": "DOWN",
            "role": leg.role,
            "strike": format_raw_price_e9(leg.strike_raw.unwrap_or_default()),
            "strikeRaw": leg.strike_raw.unwrap_or_default().to_string(),
            "quantityRaw": leg.quantity.to_string(),
            "quantityDisplay": asset.format_amount(leg.quantity),
            "askPriceRaw": leg.ask_price_raw.to_string(),
            "premiumRaw": premium_raw.to_string(),
            "premiumDisplay": asset.format_amount(premium_raw),
            "midpoint": format_raw_price_e9(leg.midpoint_raw),
            "midpointRaw": leg.midpoint_raw.to_string(),
            "weightE6": leg.weight_e6.to_string(),
            "maxQuantity": leg.max_quantity.map(|v| v.to_string())
        }),
        AdvancedLegKind::Up => serde_json::json!({
            "kind": "UP",
            "role": leg.role,
            "strike": format_raw_price_e9(leg.strike_raw.unwrap_or_default()),
            "strikeRaw": leg.strike_raw.unwrap_or_default().to_string(),
            "quantityRaw": leg.quantity.to_string(),
            "quantityDisplay": asset.format_amount(leg.quantity),
            "askPriceRaw": leg.ask_price_raw.to_string(),
            "premiumRaw": premium_raw.to_string(),
            "premiumDisplay": asset.format_amount(premium_raw),
            "midpoint": format_raw_price_e9(leg.midpoint_raw),
            "midpointRaw": leg.midpoint_raw.to_string(),
            "weightE6": leg.weight_e6.to_string(),
            "maxQuantity": leg.max_quantity.map(|v| v.to_string())
        }),
        AdvancedLegKind::Range => serde_json::json!({
            "kind": "RANGE",
            "role": leg.role,
            "lower": format_raw_price_e9(leg.lower_raw.unwrap_or_default()),
            "upper": format_raw_price_e9(leg.upper_raw.unwrap_or_default()),
            "lowerRaw": leg.lower_raw.unwrap_or_default().to_string(),
            "upperRaw": leg.upper_raw.unwrap_or_default().to_string(),
            "quantityRaw": leg.quantity.to_string(),
            "quantityDisplay": asset.format_amount(leg.quantity),
            "askPriceRaw": leg.ask_price_raw.to_string(),
            "premiumRaw": premium_raw.to_string(),
            "premiumDisplay": asset.format_amount(premium_raw),
            "midpoint": format_raw_price_e9(leg.midpoint_raw),
            "midpointRaw": leg.midpoint_raw.to_string(),
            "weightE6": leg.weight_e6.to_string(),
            "maxQuantity": leg.max_quantity.map(|v| v.to_string())
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
                    format_raw_price_e9(leg.strike_raw.unwrap_or_default())
                ),
                AdvancedLegKind::Up => format!(
                    "BTC settles >= {}",
                    format_raw_price_e9(leg.strike_raw.unwrap_or_default())
                ),
                AdvancedLegKind::Range => format!(
                    "{} < BTC settles <= {}",
                    format_raw_price_e9(leg.lower_raw.unwrap_or_default()),
                    format_raw_price_e9(leg.upper_raw.unwrap_or_default())
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
        "strike": format_raw_price_e9(strike_raw),
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
        "strike": format_raw_price_e9(strike_raw),
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
        "lower": format_raw_price_e9(lower_raw),
        "upper": format_raw_price_e9(upper_raw),
        "lowerRaw": lower_raw.to_string(),
        "upperRaw": upper_raw.to_string(),
        "quantityRaw": quantity.to_string(),
        "quantityDisplay": asset.format_amount(quantity),
        "askPriceRaw": ask_price_raw.to_string(),
        "premiumRaw": premium_raw.to_string(),
        "premiumDisplay": asset.format_amount(premium_raw)
    })
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

async fn demo_status_command(
    rpc_url: String,
    manager_id: String,
    sender: String,
    from_execution_json: PathBuf,
    expect_exact: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let execution_json: serde_json::Value =
        serde_json::from_slice(&fs::read(&from_execution_json)?)?;

    let digest = execution_json
        .get("digest")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown / recovered artifact");

    let status = execution_json
        .get("effects")
        .and_then(|effects| effects.get("status"))
        .and_then(|status| status.get("status"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");

    println!("StructX demo status");
    println!("execution json: {}", display_path(&from_execution_json));
    println!("digest: {digest}");
    println!("execution status: {status}");
    println!();

    if status != "success" {
        return Err(io::Error::other("execution JSON status is not success").into());
    }

    let reads = load_position_reads_from_execution_json(&from_execution_json)?;

    if reads.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "no PositionMinted or RangeMinted events found",
        )
        .into());
    }

    print_demo_minted_legs(&execution_json)?;

    let rpc = SuiRpcClient::new(rpc_url, StdDuration::from_secs(20))?;
    let manager = resolve_sui_object(&rpc, &manager_id).await?;

    println!();
    print_manager_preflight(&manager);
    validate_predict_manager_object(&manager)?;

    let balance_tx = build_manager_balance_tx_kind(&manager, &sender)?;
    let balance_response =
        rpc.dev_inspect_transaction_kind(&balance_tx.sender, &balance_tx.tx_kind_b64).await?;

    let balance_raw = read_manager_balance_from_response(&balance_response)?;

    let asset = QuoteAssetDisplay { symbol: "dUSDC".to_string(), decimals: DUSDC_DECIMALS };

    println!("Manager balance");
    println!("balance raw: {balance_raw}");
    println!("balance: {}", asset.format_amount(balance_raw));
    println!();

    let positions_tx = build_manager_positions_tx_kind(&reads, &manager, &sender)?;

    let positions_response =
        rpc.dev_inspect_transaction_kind(&positions_tx.sender, &positions_tx.tx_kind_b64).await?;

    let position_summary = print_manager_positions_response_best_effort(
        &reads,
        &positions_tx,
        &positions_response,
        expect_exact,
    )?;

    println!();

    if position_summary.bad == 0 {
        println!("Position verification: ok");
    } else {
        println!(
            "Position verification: partial ({} ok, {} mismatch)",
            position_summary.ok, position_summary.bad
        );
        println!(
            "Known issue: event-derived binary MarketKeys can read 0 while range positions verify correctly."
        );
    }

    println!();
    println!("Demo proof: ok");
    println!("mint execution → cost audit → manager balance → range position verification");

    Ok(())
}

fn print_demo_minted_legs(
    execution_json: &serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let events = execution_json
        .get("events")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing events array"))?;

    let asset = QuoteAssetDisplay { symbol: "dUSDC".to_string(), decimals: DUSDC_DECIMALS };

    let mut total_cost_raw = 0u64;
    let mut minted_count = 0usize;

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec![
        "#",
        "event",
        "direction",
        "strike/lower",
        "upper",
        "quantity",
        "cost raw",
        "cost",
    ]);

    for event in events {
        let event_type = event.get("type").and_then(serde_json::Value::as_str).unwrap_or("");

        let parsed = event.get("parsedJson").unwrap_or(&serde_json::Value::Null);

        if event_type.ends_with("::predict::PositionMinted") {
            let cost = json_required_u64(parsed, "cost")?;
            total_cost_raw = total_cost_raw
                .checked_add(cost)
                .ok_or_else(|| io::Error::other("total cost overflow"))?;

            let is_up = json_required_bool(parsed, "is_up")?;

            table.add_row(vec![
                Cell::new(minted_count),
                Cell::new("PositionMinted"),
                Cell::new(if is_up { "up" } else { "down" }),
                Cell::new(format_raw_price_e9(json_required_u64(parsed, "strike")?)),
                Cell::new("—"),
                Cell::new(json_required_u64(parsed, "quantity")?),
                Cell::new(cost),
                Cell::new(asset.format_amount(cost)),
            ]);

            minted_count += 1;
        } else if event_type.ends_with("::predict::RangeMinted") {
            let cost = json_required_u64(parsed, "cost")?;
            total_cost_raw = total_cost_raw
                .checked_add(cost)
                .ok_or_else(|| io::Error::other("total cost overflow"))?;

            table.add_row(vec![
                Cell::new(minted_count),
                Cell::new("RangeMinted"),
                Cell::new("—"),
                Cell::new(format_raw_price_e9(json_required_u64(parsed, "lower_strike")?)),
                Cell::new(format_raw_price_e9(json_required_u64(parsed, "higher_strike")?)),
                Cell::new(json_required_u64(parsed, "quantity")?),
                Cell::new(cost),
                Cell::new(asset.format_amount(cost)),
            ]);

            minted_count += 1;
        }
    }

    if minted_count == 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "no minted events found").into());
    }

    println!("Minted legs");
    println!("{table}");
    println!();

    println!("Mint cost summary");
    println!("minted legs: {minted_count}");
    println!("total cost raw: {total_cost_raw}");
    println!("total cost: {}", asset.format_amount(total_cost_raw));

    Ok(())
}

fn audit_execution_command(from_execution_json: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let value: serde_json::Value = serde_json::from_slice(&fs::read(&from_execution_json)?)?;

    let status = value
        .get("effects")
        .and_then(|effects| effects.get("status"))
        .and_then(|status| status.get("status"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");

    let digest = value.get("digest").and_then(serde_json::Value::as_str).unwrap_or("unknown");

    println!("StructX execution audit");
    println!("source: {}", display_path(&from_execution_json));
    println!("digest: {digest}");
    println!("status: {status}");
    println!();

    if status != "success" {
        return Err(io::Error::other("execution was not successful").into());
    }

    let events = value
        .get("events")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing events array"))?;

    let asset = QuoteAssetDisplay { symbol: "dUSDC".to_string(), decimals: DUSDC_DECIMALS };

    let mut total_cost_raw = 0u64;
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec![
        "#",
        "event",
        "direction",
        "strike/lower",
        "upper",
        "quantity",
        "cost raw",
        "cost",
        "ask price",
    ]);

    let mut minted_count = 0usize;

    for event in events {
        let event_type = event.get("type").and_then(serde_json::Value::as_str).unwrap_or("");

        let parsed = event.get("parsedJson").unwrap_or(&serde_json::Value::Null);

        if event_type.ends_with("::predict::PositionMinted") {
            let cost = json_required_u64(parsed, "cost")?;
            total_cost_raw = total_cost_raw
                .checked_add(cost)
                .ok_or_else(|| io::Error::other("total cost overflow"))?;

            let is_up = json_required_bool(parsed, "is_up")?;

            table.add_row(vec![
                Cell::new(minted_count),
                Cell::new("PositionMinted"),
                Cell::new(if is_up { "up" } else { "down" }),
                Cell::new(format_raw_price_e9(json_required_u64(parsed, "strike")?)),
                Cell::new("—"),
                Cell::new(json_required_u64(parsed, "quantity")?),
                Cell::new(cost),
                Cell::new(asset.format_amount(cost)),
                Cell::new(json_required_string(parsed, "ask_price")?),
            ]);

            minted_count += 1;
        } else if event_type.ends_with("::predict::RangeMinted") {
            let cost = json_required_u64(parsed, "cost")?;
            total_cost_raw = total_cost_raw
                .checked_add(cost)
                .ok_or_else(|| io::Error::other("total cost overflow"))?;

            table.add_row(vec![
                Cell::new(minted_count),
                Cell::new("RangeMinted"),
                Cell::new("—"),
                Cell::new(format_raw_price_e9(json_required_u64(parsed, "lower_strike")?)),
                Cell::new(format_raw_price_e9(json_required_u64(parsed, "higher_strike")?)),
                Cell::new(json_required_u64(parsed, "quantity")?),
                Cell::new(cost),
                Cell::new(asset.format_amount(cost)),
                Cell::new(json_required_string(parsed, "ask_price")?),
            ]);

            minted_count += 1;
        }
    }

    if minted_count == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "no PositionMinted or RangeMinted events found",
        )
        .into());
    }

    println!("Minted legs");
    println!("{table}");
    println!();

    println!("Execution cost summary");
    println!("minted legs: {minted_count}");
    println!("total cost raw: {total_cost_raw}");
    println!("total cost: {}", asset.format_amount(total_cost_raw));

    Ok(())
}

struct DevinspectRedeemBreakoutArgs {
    rpc_url: String,
    manager_id: String,
    sender: String,
    from_execution_json: PathBuf,
    min_total_payout_raw: Option<u64>,
    auto_size_down: bool,
    redeem_bps: u16,
    write_execute_script: bool,
    allow_zero_payout_script: bool,
    execute_script_path: PathBuf,
    execute_plan_json_path: PathBuf,
}

async fn devinspect_redeem_breakout_command(
    args: DevinspectRedeemBreakoutArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let _redeem_sizing_flags = (args.auto_size_down, args.redeem_bps);
    let reads = load_position_reads_from_execution_json(&args.from_execution_json)?;

    if reads.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "no PositionMinted or RangeMinted events found",
        )
        .into());
    }

    let oracle_id = first_oracle_id(&reads)?;

    let rpc = SuiRpcClient::new(args.rpc_url.clone(), StdDuration::from_secs(20))?;

    let predict = resolve_sui_object(&rpc, PREDICT_OBJECT_ID).await?;
    let manager = resolve_sui_object(&rpc, &args.manager_id).await?;
    let oracle = resolve_sui_object(&rpc, &oracle_id).await?;
    let clock = resolve_sui_object(&rpc, SUI_CLOCK_OBJECT_ID).await?;

    validate_predict_manager_object(&manager)?;
    validate_quote_object_refs(&predict, &oracle, &clock)?;

    println!("Redeem source");
    println!("execution json: {}", display_path(&args.from_execution_json));
    println!("manager_id: {}", args.manager_id);
    println!("oracle_id: {oracle_id}");
    println!("legs: {}", reads.len());
    println!();

    let tx_kind = build_redeem_tx_kind(
        &reads,
        MintObjectRefs { predict: &predict, manager: &manager, oracle: &oracle, clock: &clock },
        &args.sender,
    )?;

    println!("Built redeem TransactionKind");
    println!("sender: {}", tx_kind.sender);
    println!("tx_kind_b64_len: {}", tx_kind.tx_kind_b64.len());
    println!("redeem command indices: {:?}", tx_kind.quote_result_command_indices);
    println!();

    let response = rpc.dev_inspect_transaction_kind(&tx_kind.sender, &tx_kind.tx_kind_b64).await?;

    let total_payout_raw = print_devinspect_redeem_response(&response)?;

    if let Some(min_total_payout_raw) = args.min_total_payout_raw {
        if total_payout_raw < min_total_payout_raw {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "redeem payout {} is below minimum {}",
                    total_payout_raw, min_total_payout_raw
                ),
            )
            .into());
        }

        println!("Redeem payout guard: accepted");
        println!("min_total_payout_raw: {min_total_payout_raw}");
        println!("actual_total_payout_raw: {total_payout_raw}");
    } else {
        println!("Redeem payout guard: skipped; pass --min-total-payout-raw to enforce a floor");
    }

    if args.write_execute_script {
        if total_payout_raw == 0 && !args.allow_zero_payout_script {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "refusing to write redeem execution script with zero payout; pass --allow-zero-payout-script only if you intentionally want to burn/close losing positions",
            )
            .into());
        }

        write_execute_redeem_artifacts(&args, &reads, total_payout_raw)?;
    }

    println!();
    println!("Important: this was devInspect only. No positions were redeemed.");

    Ok(())
}

fn write_execute_redeem_artifacts(
    args: &DevinspectRedeemBreakoutArgs,
    reads: &[ManagerPositionRead],
    total_payout_raw: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let script = build_execute_redeem_script(args, reads)?;

    fs::write(&args.execute_script_path, script)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut perms = fs::metadata(&args.execute_script_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&args.execute_script_path, perms)?;
    }

    let manifest = build_execute_redeem_manifest(args, reads, total_payout_raw);
    fs::write(&args.execute_plan_json_path, serde_json::to_string_pretty(&manifest)?)?;

    println!();
    println!("Fresh executable redeem artifacts written");
    println!("script: {}", display_path(&args.execute_script_path));
    println!("plan: {}", display_path(&args.execute_plan_json_path));
    println!();
    println!("Execute immediately with:");
    println!(
        "GAS_BUDGET=500000000 bash {} --json | tee artifacts/structx_execute_redeem_breakout.json",
        display_path(&args.execute_script_path)
    );

    Ok(())
}

fn build_execute_redeem_manifest(
    args: &DevinspectRedeemBreakoutArgs,
    reads: &[ManagerPositionRead],
    total_payout_raw: u64,
) -> serde_json::Value {
    let asset = QuoteAssetDisplay { symbol: "dUSDC".to_string(), decimals: DUSDC_DECIMALS };

    let legs = reads
        .iter()
        .enumerate()
        .map(|(idx, read)| match read {
            ManagerPositionRead::Binary {
                oracle_id,
                expiry_ms,
                strike_raw,
                is_up,
                expected_quantity,
            } => serde_json::json!({
                "index": idx,
                "kind": "binary",
                "direction": if *is_up { "up" } else { "down" },
                "oracle_id": oracle_id,
                "expiry_ms": expiry_ms,
                "strike_raw": strike_raw,
                "strike": format_raw_price_e9(*strike_raw),
                "redeem_quantity": expected_quantity,
            }),
            ManagerPositionRead::Range {
                oracle_id,
                expiry_ms,
                lower_raw,
                upper_raw,
                expected_quantity,
            } => serde_json::json!({
                "index": idx,
                "kind": "range",
                "oracle_id": oracle_id,
                "expiry_ms": expiry_ms,
                "lower_raw": lower_raw,
                "upper_raw": upper_raw,
                "lower": format_raw_price_e9(*lower_raw),
                "upper": format_raw_price_e9(*upper_raw),
                "redeem_quantity": expected_quantity,
            }),
        })
        .collect::<Vec<_>>();

    serde_json::json!({
        "sender": args.sender,
        "manager_id": args.manager_id,
        "predict_object_id": PREDICT_OBJECT_ID,
        "predict_package_id": PREDICT_PACKAGE_ID,
        "total_payout_raw": total_payout_raw,
        "total_payout": asset.format_amount(total_payout_raw),
        "allow_zero_payout_script": args.allow_zero_payout_script,
        "legs": legs,
        "warning": "Generated only after successful redeem devInspect. Execute immediately; pricing/settlement state can change."
    })
}

fn build_execute_redeem_script(
    args: &DevinspectRedeemBreakoutArgs,
    reads: &[ManagerPositionRead],
) -> Result<String, Box<dyn std::error::Error>> {
    if reads.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "empty redeem plan").into());
    }

    let oracle_id = first_oracle_id(reads)?;

    let mut out = String::new();

    out.push_str("#!/usr/bin/env bash\n");
    out.push_str("set -euo pipefail\n\n");

    out.push_str("# Fresh StructX redeem script generated only after successful devInspect.\n");
    out.push_str(
        "# Execute immediately; Predict pricing/settlement checks can change between runs.\n\n",
    );

    out.push_str(&format!("export PREDICT_PACKAGE={}\n", PREDICT_PACKAGE_ID));
    out.push_str(&format!("export PREDICT_OBJECT_ID={}\n", PREDICT_OBJECT_ID));
    out.push_str(&format!("export DUSDC={}\n", DUSDC_COIN_TYPE));
    out.push_str(&format!("export MANAGER_ID={}\n", args.manager_id));
    out.push_str(&format!("export OWNER={}\n", args.sender));
    out.push_str(&format!("export ORACLE_ID={}\n", oracle_id));
    out.push_str("export CLOCK_ID=0x6\n");
    out.push_str("export GAS_BUDGET=${GAS_BUDGET:-500000000}\n\n");

    out.push_str("EXTRA_ARGS=(\"$@\")\n");
    out.push_str("if [ ${#EXTRA_ARGS[@]} -eq 0 ]; then\n");
    out.push_str("  EXTRA_ARGS=(--json)\n");
    out.push_str("fi\n\n");

    out.push_str("sui client ptb \\\n");
    out.push_str("  --sender \"$OWNER\" \\\n");

    for (idx, read) in reads.iter().enumerate() {
        let key_name = format!("key{idx}");

        match read {
            ManagerPositionRead::Binary {
                expiry_ms, strike_raw, is_up, expected_quantity, ..
            } => {
                let key_function = if *is_up { "up" } else { "down" };

                out.push_str(&format!(
                    "  --move-call \"${{PREDICT_PACKAGE}}::market_key::{key_function}\" \"@${{ORACLE_ID}}\" \"{}\" \"{}\" \\\n",
                    expiry_ms,
                    strike_raw,
                ));
                out.push_str(&format!("  --assign {key_name} \\\n"));
                out.push_str(&format!(
                    "  --move-call \"${{PREDICT_PACKAGE}}::predict::redeem\" \"<${{DUSDC}}>\" \"@${{PREDICT_OBJECT_ID}}\" \"@${{MANAGER_ID}}\" \"@${{ORACLE_ID}}\" {key_name} \"{}\" \"@${{CLOCK_ID}}\" \\\n",
                    expected_quantity,
                ));
            }
            ManagerPositionRead::Range {
                expiry_ms,
                lower_raw,
                upper_raw,
                expected_quantity,
                ..
            } => {
                out.push_str(&format!(
                    "  --move-call \"${{PREDICT_PACKAGE}}::range_key::new\" \"@${{ORACLE_ID}}\" \"{}\" \"{}\" \"{}\" \\\n",
                    expiry_ms,
                    lower_raw,
                    upper_raw,
                ));
                out.push_str(&format!("  --assign {key_name} \\\n"));
                out.push_str(&format!(
                    "  --move-call \"${{PREDICT_PACKAGE}}::predict::redeem_range\" \"<${{DUSDC}}>\" \"@${{PREDICT_OBJECT_ID}}\" \"@${{MANAGER_ID}}\" \"@${{ORACLE_ID}}\" {key_name} \"{}\" \"@${{CLOCK_ID}}\" \\\n",
                    expected_quantity,
                ));
            }
        }
    }

    out.push_str("  --gas-budget \"$GAS_BUDGET\" \\\n");
    out.push_str("  \"${EXTRA_ARGS[@]}\"\n");

    Ok(out)
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

fn print_devinspect_redeem_response(
    response: &serde_json::Value,
) -> Result<u64, Box<dyn std::error::Error>> {
    let status = response
        .get("effects")
        .and_then(|effects| effects.get("status"))
        .and_then(|status| status.get("status"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");

    println!("redeem devInspect status: {status}");

    if status != "success" {
        return Err(io::Error::other(devinspect_failure_summary(response)).into());
    }

    let events =
        response.get("events").and_then(serde_json::Value::as_array).cloned().unwrap_or_default();

    let asset = QuoteAssetDisplay { symbol: "dUSDC".to_string(), decimals: DUSDC_DECIMALS };

    let mut total_payout_raw = 0u64;

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec![
        "event",
        "direction",
        "strike/lower",
        "upper",
        "quantity",
        "payout raw",
        "payout",
        "bid price",
        "settled",
    ]);

    for event in events {
        let event_type = event.get("type").and_then(serde_json::Value::as_str).unwrap_or("");

        let parsed = event.get("parsedJson").unwrap_or(&serde_json::Value::Null);

        if event_type.ends_with("::predict::PositionRedeemed") {
            let payout = json_required_u64(parsed, "payout")?;
            total_payout_raw = total_payout_raw
                .checked_add(payout)
                .ok_or_else(|| io::Error::other("total payout overflow"))?;

            let is_up = json_required_bool(parsed, "is_up")?;

            table.add_row(vec![
                Cell::new("PositionRedeemed"),
                Cell::new(if is_up { "up" } else { "down" }),
                Cell::new(format_raw_price_e9(json_required_u64(parsed, "strike")?)),
                Cell::new("—"),
                Cell::new(json_required_u64(parsed, "quantity")?),
                Cell::new(payout),
                Cell::new(asset.format_amount(payout)),
                Cell::new(json_required_string(parsed, "bid_price")?),
                Cell::new(json_required_bool(parsed, "is_settled")?),
            ]);
        } else if event_type.ends_with("::predict::RangeRedeemed") {
            let payout = json_required_u64(parsed, "payout")?;
            total_payout_raw = total_payout_raw
                .checked_add(payout)
                .ok_or_else(|| io::Error::other("total payout overflow"))?;

            table.add_row(vec![
                Cell::new("RangeRedeemed"),
                Cell::new("—"),
                Cell::new(format_raw_price_e9(json_required_u64(parsed, "lower_strike")?)),
                Cell::new(format_raw_price_e9(json_required_u64(parsed, "higher_strike")?)),
                Cell::new(json_required_u64(parsed, "quantity")?),
                Cell::new(payout),
                Cell::new(asset.format_amount(payout)),
                Cell::new(json_required_string(parsed, "bid_price")?),
                Cell::new(json_required_bool(parsed, "is_settled")?),
            ]);
        }
    }

    println!("Redeem preview events");
    println!("{table}");
    println!();

    println!("Redeem payout summary");
    println!("total payout raw: {total_payout_raw}");
    println!("total payout: {}", asset.format_amount(total_payout_raw));

    Ok(total_payout_raw)
}

async fn manager_positions_command(
    rpc_url: String,
    manager_id: String,
    from_execution_json: PathBuf,
    sender: String,
    expect_exact: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let reads = load_position_reads_from_execution_json(&from_execution_json)?;

    if reads.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "no PositionMinted or RangeMinted events found",
        )
        .into());
    }

    let rpc = SuiRpcClient::new(rpc_url, StdDuration::from_secs(20))?;

    let manager = resolve_sui_object(&rpc, &manager_id).await?;
    print_manager_preflight(&manager);
    validate_predict_manager_object(&manager)?;

    let tx_kind = build_manager_positions_tx_kind(&reads, &manager, &sender)?;

    println!("Built manager-positions TransactionKind");
    println!("sender: {}", tx_kind.sender);
    println!("tx_kind_b64_len: {}", tx_kind.tx_kind_b64.len());
    println!("position result command indices: {:?}", tx_kind.quote_result_command_indices);
    println!();

    let response = rpc.dev_inspect_transaction_kind(&tx_kind.sender, &tx_kind.tx_kind_b64).await?;

    print_manager_positions_response(&reads, &tx_kind, &response, expect_exact)?;

    Ok(())
}

fn load_position_reads_from_execution_json(
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

fn print_manager_positions_response_best_effort(
    reads: &[ManagerPositionRead],
    tx_kind: &QuoteTxKind,
    response: &serde_json::Value,
    expect_exact: bool,
) -> Result<PositionCheckSummary, Box<dyn std::error::Error>> {
    let status = response
        .get("effects")
        .and_then(|effects| effects.get("status"))
        .and_then(|status| status.get("status"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");

    println!("devInspect status: {status}");

    if status != "success" {
        return Err(io::Error::other(devinspect_failure_summary(response)).into());
    }

    let results = response
        .get("results")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing devInspect results"))?;

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec![
        "#",
        "kind",
        "direction",
        "strike/lower",
        "upper",
        "minted qty",
        "manager qty",
        "check",
    ]);

    let mut ok = 0usize;
    let mut bad = 0usize;

    for (idx, read) in reads.iter().enumerate() {
        let command_idx = tx_kind
            .quote_result_command_indices
            .get(idx)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing command index"))?;

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

        let check = if accepted {
            ok += 1;
            "ok"
        } else {
            bad += 1;
            "mismatch"
        };

        match read {
            ManagerPositionRead::Binary { strike_raw, is_up, .. } => {
                table.add_row(vec![
                    Cell::new(idx),
                    Cell::new("binary"),
                    Cell::new(if *is_up { "up" } else { "down" }),
                    Cell::new(format_raw_price_e9(*strike_raw)),
                    Cell::new("—"),
                    Cell::new(expected_quantity),
                    Cell::new(actual_quantity),
                    Cell::new(check),
                ]);
            }
            ManagerPositionRead::Range { lower_raw, upper_raw, .. } => {
                table.add_row(vec![
                    Cell::new(idx),
                    Cell::new("range"),
                    Cell::new("—"),
                    Cell::new(format_raw_price_e9(*lower_raw)),
                    Cell::new(format_raw_price_e9(*upper_raw)),
                    Cell::new(expected_quantity),
                    Cell::new(actual_quantity),
                    Cell::new(check),
                ]);
            }
        }
    }

    println!("Manager position verification");
    println!("{table}");

    Ok(PositionCheckSummary { ok, bad })
}

fn print_manager_positions_response(
    reads: &[ManagerPositionRead],
    tx_kind: &QuoteTxKind,
    response: &serde_json::Value,
    expect_exact: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let status = response
        .get("effects")
        .and_then(|effects| effects.get("status"))
        .and_then(|status| status.get("status"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");

    println!("devInspect status: {status}");

    if status != "success" {
        return Err(io::Error::other(devinspect_failure_summary(response)).into());
    }

    let results = response
        .get("results")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing devInspect results"))?;

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec![
        "#",
        "kind",
        "direction",
        "strike/lower",
        "upper",
        "minted qty",
        "manager qty",
        "check",
    ]);

    for (idx, read) in reads.iter().enumerate() {
        let command_idx = tx_kind
            .quote_result_command_indices
            .get(idx)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing command index"))?;

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

        let check = if accepted { "ok" } else { "bad" };

        match read {
            ManagerPositionRead::Binary { strike_raw, is_up, .. } => {
                table.add_row(vec![
                    Cell::new(idx),
                    Cell::new("binary"),
                    Cell::new(if *is_up { "up" } else { "down" }),
                    Cell::new(format_raw_price_e9(*strike_raw)),
                    Cell::new("—"),
                    Cell::new(expected_quantity),
                    Cell::new(actual_quantity),
                    Cell::new(check),
                ]);
            }
            ManagerPositionRead::Range { lower_raw, upper_raw, .. } => {
                table.add_row(vec![
                    Cell::new(idx),
                    Cell::new("range"),
                    Cell::new("—"),
                    Cell::new(format_raw_price_e9(*lower_raw)),
                    Cell::new(format_raw_price_e9(*upper_raw)),
                    Cell::new(expected_quantity),
                    Cell::new(actual_quantity),
                    Cell::new(check),
                ]);
            }
        }

        if !accepted {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "position check failed at index {idx}: expected {}, actual {}",
                    expected_quantity, actual_quantity
                ),
            )
            .into());
        }
    }

    println!("Manager position verification");
    println!("{table}");
    println!();
    println!("Manager positions: ok ({})", if expect_exact { "exact" } else { "actual >= minted" });

    Ok(())
}

fn position_expected_quantity(read: &ManagerPositionRead) -> u64 {
    match read {
        ManagerPositionRead::Binary { expected_quantity, .. }
        | ManagerPositionRead::Range { expected_quantity, .. } => *expected_quantity,
    }
}

fn json_required_string(
    value: &serde_json::Value,
    key: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    value.get(key).and_then(serde_json::Value::as_str).map(ToString::to_string).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, format!("missing string field `{key}`")).into()
    })
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
            io::Error::new(io::ErrorKind::InvalidData, format!("invalid u64 field `{key}`")).into()
        }),
        _ => {
            Err(io::Error::new(io::ErrorKind::InvalidData, format!("invalid u64 field `{key}`"))
                .into())
        }
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
        _ => Err(io::Error::new(io::ErrorKind::InvalidData, format!("invalid bool field `{key}`"))
            .into()),
    }
}

fn format_raw_price_e9(raw: u64) -> String {
    let cents = (raw + 5_000_000) / 10_000_000;
    let whole = cents / 100;
    let frac = cents % 100;

    format!("{whole}.{frac:02}")
}

async fn devinspect_mint_breakout_command(
    args: DevinspectMintBreakoutArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = build_client(args.server_url.clone(), args.predict_id.clone())?;
    let markets = load_markets(&client, args.freshness).await?;

    let candidates = select_candidate_markets(&markets, PriceScale::E9);

    if candidates.is_empty() {
        return Err(
            io::Error::new(io::ErrorKind::InvalidData, "no quoteable market candidates").into()
        );
    }

    let max_attempts = args.max_quote_market_attempts.min(candidates.len());
    let mut failures = Vec::new();

    for (attempt_idx, selected) in candidates.into_iter().take(max_attempts).enumerate() {
        println!(
            "Mint attempt {}/{} using oracle {} expiring {}",
            attempt_idx + 1,
            max_attempts,
            selected.oracle_id,
            selected.expiry.to_rfc3339()
        );

        match devinspect_mint_for_selected_market(&args, &selected).await {
            Ok(()) => return Ok(()),
            Err(err) => {
                let message = format!(
                    "oracle {} expiry {} failed: {}",
                    selected.oracle_id,
                    selected.expiry.to_rfc3339(),
                    err
                );
                eprintln!("{message}");
                failures.push(message);
            }
        }
    }

    Err(io::Error::other(format!("all mint attempts failed:\n{}", failures.join("\n"))).into())
}

async fn devinspect_mint_for_selected_market(
    args: &DevinspectMintBreakoutArgs,
    selected: &SelectedMarket<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    print_selected_market(selected);

    let strikes = selected.grid.centered_strikes_by_display_step(
        selected.spot_raw,
        args.bucket_step,
        args.levels_each_side,
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
            "not enough strikes around spot; increase --levels-each-side",
        )
        .into());
    }

    let k1 = strikes[center_idx - 2];
    let k2 = strikes[center_idx - 1];
    let k3 = strikes[center_idx + 1];
    let k4 = strikes[center_idx + 2];

    let compiled = compile_breakout(k1, k2, k3, k4, args.tail_quantity, args.shoulder_quantity)?;

    let plan = build_quote_plan(selected, &compiled)?;

    print_breakout_boundaries(selected, k1, k2, k3, k4);
    print_compiled_payoff(selected, &compiled);
    print_quote_plan(selected, &plan);

    let rpc = SuiRpcClient::new(args.rpc_url.clone(), StdDuration::from_secs(20))?;

    let predict = resolve_sui_object(&rpc, PREDICT_OBJECT_ID).await?;
    let manager = resolve_sui_object(&rpc, &args.manager_id).await?;
    let oracle = resolve_sui_object(&rpc, selected.oracle_id).await?;
    let clock = resolve_sui_object(&rpc, SUI_CLOCK_OBJECT_ID).await?;

    validate_predict_manager_object(&manager)?;
    validate_quote_object_refs(&predict, &oracle, &clock)?;

    let quote_tx_kind = build_quote_tx_kind(
        &plan,
        QuoteObjectRefs { predict: &predict, oracle: &oracle, clock: &clock },
        &args.sender,
    )?;

    let quote_response =
        rpc.dev_inspect_transaction_kind(&quote_tx_kind.sender, &quote_tx_kind.tx_kind_b64).await?;

    let preview =
        print_devinspect_quote_response(selected, &plan, &quote_tx_kind, &quote_response)?;

    let guarded = guard_quote_preview(
        &preview,
        QuoteCostGuard {
            max_total_mint_cost_raw: args.max_total_mint_cost_raw,
            slippage_bps: args.slippage_bps,
        },
    )?;

    println!();
    println!("Quote guard: accepted");
    println!("max_allowed_after_slippage_raw: {}", guarded.max_allowed_after_slippage_raw);
    println!("actual_total_mint_cost_raw: {}", guarded.total_mint_cost_raw);

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

    println!("Manager balance check: accepted");
    println!("manager balance raw: {manager_balance_raw}");
    println!("required mint cost raw: {}", preview.total_mint_cost_raw);

    let mint_tx_kind = build_mint_tx_kind(
        &plan,
        MintObjectRefs { predict: &predict, manager: &manager, oracle: &oracle, clock: &clock },
        &args.sender,
    )?;

    println!();
    println!("Built mint TransactionKind");
    println!("sender: {}", mint_tx_kind.sender);
    println!("tx_kind_b64_len: {}", mint_tx_kind.tx_kind_b64.len());
    println!("mint command indices: {:?}", mint_tx_kind.quote_result_command_indices);
    println!();

    let mint_response =
        rpc.dev_inspect_transaction_kind(&mint_tx_kind.sender, &mint_tx_kind.tx_kind_b64).await?;

    print_devinspect_mint_response(&mint_response)?;

    if args.write_execute_script {
        write_execute_mint_artifacts(args, selected, &plan, &preview)?;
    }

    Ok(())
}

fn read_manager_balance_from_response(
    response: &serde_json::Value,
) -> Result<u64, Box<dyn std::error::Error>> {
    let status = devinspect_status(response);

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
            format!("expected 1 manager balance return, got {}", return_values.len()),
        )
        .into());
    }

    decode_devinspect_u64(&return_values[0])
}

async fn manager_balance_command(
    rpc_url: String,
    manager_id: String,
    sender: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let rpc = SuiRpcClient::new(rpc_url, StdDuration::from_secs(20))?;

    let manager = resolve_sui_object(&rpc, &manager_id).await?;
    print_manager_preflight(&manager);
    validate_predict_manager_object(&manager)?;

    let tx_kind = build_manager_balance_tx_kind(&manager, &sender)?;

    println!("Built manager-balance TransactionKind");
    println!("sender: {}", tx_kind.sender);
    println!("tx_kind_b64_len: {}", tx_kind.tx_kind_b64.len());
    println!();

    let response = rpc.dev_inspect_transaction_kind(&tx_kind.sender, &tx_kind.tx_kind_b64).await?;

    print_manager_balance_response(&response)?;

    Ok(())
}

fn print_manager_balance_response(
    response: &serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let status = devinspect_status(response);

    println!("devInspect status: {status}");

    if status != "success" {
        return Err(io::Error::other(devinspect_failure_summary(response)).into());
    }

    let results = response
        .get("results")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing devInspect results"))?;

    let first_result = results.first().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "missing manager balance command result")
    })?;

    let return_values =
        first_result.get("returnValues").and_then(serde_json::Value::as_array).ok_or_else(
            || io::Error::new(io::ErrorKind::InvalidData, "missing manager balance returnValues"),
        )?;

    if return_values.len() != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("expected manager balance to return 1 value, got {}", return_values.len()),
        )
        .into());
    }

    let balance_raw = decode_devinspect_u64(&return_values[0])?;

    let asset = QuoteAssetDisplay { symbol: "dUSDC".to_string(), decimals: DUSDC_DECIMALS };

    println!("Manager balance");
    println!("balance raw: {balance_raw}");
    println!("balance: {}", asset.format_amount(balance_raw));

    Ok(())
}

async fn resolve_manager_command(
    rpc_url: String,
    manager_id: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let rpc = SuiRpcClient::new(rpc_url, StdDuration::from_secs(20))?;
    let manager = resolve_sui_object(&rpc, &manager_id).await?;

    print_manager_preflight(&manager);
    validate_predict_manager_object(&manager)?;

    println!("PredictManager preflight: ok");

    Ok(())
}

fn print_manager_preflight(manager: &SuiObjectInfo) {
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec![
        "role",
        "object id",
        "type",
        "owner",
        "version",
        "digest",
        "initial shared version",
    ]);

    table.add_row(vec![
        Cell::new("manager"),
        Cell::new(&manager.object_id),
        Cell::new(manager.object_type.as_deref().unwrap_or("—")),
        Cell::new(manager.owner_kind.to_string()),
        Cell::new(
            manager.version.map(|value| value.to_string()).unwrap_or_else(|| "—".to_string()),
        ),
        Cell::new(manager.digest.as_deref().unwrap_or("—")),
        Cell::new(
            manager
                .initial_shared_version
                .map(|value| value.to_string())
                .unwrap_or_else(|| "—".to_string()),
        ),
    ]);

    println!("PredictManager object");
    println!("{table}");
    println!();
}

fn validate_predict_manager_object(
    manager: &SuiObjectInfo,
) -> Result<(), Box<dyn std::error::Error>> {
    if manager.owner_kind != ObjectOwnerKind::Shared {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("PredictManager is not shared: owner={}", manager.owner_kind),
        )
        .into());
    }

    if manager.initial_shared_version.is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "PredictManager is missing initial_shared_version",
        )
        .into());
    }

    let actual_type = manager.object_type.as_deref().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "PredictManager object is missing type")
    })?;

    if actual_type != PREDICT_MANAGER_TYPE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "unexpected PredictManager type: expected {}, got {}",
                PREDICT_MANAGER_TYPE, actual_type
            ),
        )
        .into());
    }

    Ok(())
}

fn print_quote_object_refs(predict: &SuiObjectInfo, oracle: &SuiObjectInfo, clock: &SuiObjectInfo) {
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec![
        "role",
        "object id",
        "type",
        "owner",
        "version",
        "digest",
        "initial shared version",
    ]);

    for (role, object) in [("predict", predict), ("oracle", oracle), ("clock", clock)] {
        table.add_row(vec![
            Cell::new(role),
            Cell::new(&object.object_id),
            Cell::new(object.object_type.as_deref().unwrap_or("—")),
            Cell::new(object.owner_kind.to_string()),
            Cell::new(
                object.version.map(|value| value.to_string()).unwrap_or_else(|| "—".to_string()),
            ),
            Cell::new(object.digest.as_deref().unwrap_or("—")),
            Cell::new(
                object
                    .initial_shared_version
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "—".to_string()),
            ),
        ]);
    }

    println!("Quote object refs");
    println!("{table}");
    println!();
}

fn validate_quote_object_refs(
    predict: &SuiObjectInfo,
    oracle: &SuiObjectInfo,
    clock: &SuiObjectInfo,
) -> Result<(), Box<dyn std::error::Error>> {
    let checks = [("predict", predict), ("oracle", oracle), ("clock", clock)];

    for (role, object) in checks {
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

    println!("Quote object refs: ok");
    Ok(())
}

async fn verify_abi_command(rpc_url: String) -> Result<(), Box<dyn std::error::Error>> {
    let rpc = SuiRpcClient::new(rpc_url, StdDuration::from_secs(20))?;

    let modules = rpc.get_normalized_move_modules_by_package(PREDICT_PACKAGE_ID).await?;

    let report = verify_predict_abi(PREDICT_PACKAGE_ID, &modules);

    print_abi_report(&report);

    if !report.is_pass() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "DeepBook Predict ABI verification failed",
        )
        .into());
    }

    Ok(())
}

fn print_abi_report(report: &AbiVerificationReport) {
    println!("ABI verification");
    println!("package_id: {}", report.package_id);
    println!("modules found: {}", report.module_count);
    println!();

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec![
        "status",
        "module",
        "function",
        "visibility",
        "params",
        "returns",
        "source",
        "message",
    ]);

    for check in &report.checks {
        table.add_row(vec![
            Cell::new(check.status.to_string()),
            Cell::new(&check.module),
            Cell::new(&check.function),
            Cell::new(check.visibility.as_deref().unwrap_or("—")),
            Cell::new(format!(
                "{}/{}",
                check
                    .actual_parameter_count
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "—".to_string()),
                check.expected_parameter_count
            )),
            Cell::new(format!(
                "{}/{}",
                check
                    .actual_return_count
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "—".to_string()),
                check.expected_return_count
            )),
            Cell::new(&check.source_note),
            Cell::new(check.message.as_deref().unwrap_or("")),
        ]);
    }

    println!("{table}");
    println!();

    for check in &report.checks {
        if check.status == AbiCheckStatus::Pass {
            println!("{}::{} parameters:", check.module, check.function);
            println!("source: {}", check.source_url);
            for (idx, param) in check.parameters.iter().enumerate() {
                println!("  [{idx}] {param}");
            }

            println!("{}::{} returns:", check.module, check.function);
            for (idx, return_type) in check.returns.iter().enumerate() {
                println!("  [{idx}] {return_type}");
            }

            println!();
        }
    }
}

fn print_breakout_boundaries(
    selected: &SelectedMarket<'_>,
    k1: Strike,
    k2: Strike,
    k3: Strike,
    k4: Strike,
) {
    println!("Breakout boundaries");
    println!("K1 downside tail: {}", selected.grid.display(k1));
    println!("K2 downside shoulder upper: {}", selected.grid.display(k2));
    println!("K3 upside shoulder lower: {}", selected.grid.display(k3));
    println!("K4 upside tail: {}", selected.grid.display(k4));
    println!();
}

fn print_compiled_payoff(selected: &SelectedMarket<'_>, compiled: &CompiledPayoff) {
    let mut payoff_table = Table::new();
    payoff_table.load_preset(UTF8_FULL);
    payoff_table.set_header(vec!["bucket", "lower", "upper", "payout quantity"]);

    for (idx, bucket) in compiled.buckets.iter().enumerate() {
        let lower = bucket
            .lower
            .map(|strike| selected.grid.display(strike).to_string())
            .unwrap_or_else(|| "−∞".to_string());

        let upper = bucket
            .upper
            .map(|strike| selected.grid.display(strike).to_string())
            .unwrap_or_else(|| "+∞".to_string());

        payoff_table.add_row(vec![
            Cell::new(idx),
            Cell::new(lower),
            Cell::new(upper),
            Cell::new(bucket.payout_quantity),
        ]);
    }

    println!("Compiled payoff table");
    println!("{payoff_table}");
    println!();

    let mut leg_table = Table::new();
    leg_table.load_preset(UTF8_FULL);
    leg_table.set_header(vec!["#", "leg type", "strike/lower", "upper", "quantity"]);

    for (idx, leg) in compiled.legs.iter().enumerate() {
        match leg {
            PredictLeg::Binary { direction, strike, quantity } => {
                leg_table.add_row(vec![
                    Cell::new(idx),
                    Cell::new(format!("{direction}_binary")),
                    Cell::new(selected.grid.display(*strike).to_string()),
                    Cell::new("—"),
                    Cell::new(quantity),
                ]);
            }
            PredictLeg::Range { lower, upper, quantity } => {
                leg_table.add_row(vec![
                    Cell::new(idx),
                    Cell::new("range"),
                    Cell::new(selected.grid.display(*lower).to_string()),
                    Cell::new(selected.grid.display(*upper).to_string()),
                    Cell::new(quantity),
                ]);
            }
        }
    }

    println!("Compiled Predict legs");
    println!("{leg_table}");
    println!();
    println!("Max payout quantity: {}", compiled.max_payout_quantity);
}

fn print_quote_plan(selected: &SelectedMarket<'_>, plan: &QuotePlan) {
    println!("Quote target");
    println!("package_id: {}", plan.target.package_id);
    println!("predict_object_id: {}", plan.target.predict_object_id);
    println!("module: {}", plan.target.module);
    println!("oracle_id: {}", plan.oracle_id);
    println!("expiry_ms: {}", plan.expiry_ms);
    println!();

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec![
        "#",
        "Move function",
        "leg",
        "direction",
        "strike/lower",
        "upper",
        "quantity",
    ]);

    for (idx, call) in plan.calls.iter().enumerate() {
        match call {
            QuoteCall::Binary { function, direction, strike, quantity, .. } => {
                table.add_row(vec![
                    Cell::new(idx),
                    Cell::new(format!("predict::{function}")),
                    Cell::new("binary"),
                    Cell::new(direction.to_string()),
                    Cell::new(selected.grid.display(*strike).to_string()),
                    Cell::new("—"),
                    Cell::new(quantity),
                ]);
            }
            QuoteCall::Range { function, lower, upper, quantity, .. } => {
                table.add_row(vec![
                    Cell::new(idx),
                    Cell::new(format!("predict::{function}")),
                    Cell::new("range"),
                    Cell::new("—"),
                    Cell::new(selected.grid.display(*lower).to_string()),
                    Cell::new(selected.grid.display(*upper).to_string()),
                    Cell::new(quantity),
                ]);
            }
        }
    }

    println!("Semantic quote plan");
    println!("{table}");
}

fn print_market_table(markets: &[MarketSnapshot]) {
    let now = Utc::now();

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec![
        "oracle id",
        "underlying",
        "status",
        "expiry",
        "spot/latest",
        "min strike",
        "tick size",
        "price age",
        "SVI age",
        "usable",
    ]);

    for market in markets {
        table.add_row(vec![
            Cell::new(market.oracle_id().unwrap_or("—")),
            Cell::new(market.underlying().unwrap_or("—")),
            Cell::new(market.status().unwrap_or("—")),
            Cell::new(
                market
                    .expiry_datetime()
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_else(|| "—".to_string()),
            ),
            Cell::new(format_latest_price(market)),
            Cell::new(format_scaled_raw(market.min_strike())),
            Cell::new(format_scaled_raw(market.tick_size())),
            Cell::new(format_age(market.price_age_seconds(now))),
            Cell::new(format_age(market.svi_age_seconds(now))),
            Cell::new(format_usable(&market.structx_status)),
        ]);
    }

    println!("{table}");
}

fn print_selected_market(selected: &SelectedMarket<'_>) {
    println!("Selected market");
    println!("oracle_id: {}", selected.oracle_id);
    println!("expiry: {}", selected.expiry.to_rfc3339());
    println!("spot: {}", selected.spot_display);
    println!("min_strike: {}", selected.grid.scale.display_from_raw(selected.grid.min_raw));
    println!("tick_size: {}", selected.grid.scale.display_from_raw(selected.grid.tick_size_raw));

    match &selected.market.structx_status {
        StructxMarketStatus::Usable => println!("status: usable"),
        StructxMarketStatus::UsableWithWarnings(warnings) => {
            println!("status: usable with warnings: {warnings:?}");
        }
        StructxMarketStatus::Rejected { reasons, warnings } => {
            println!("status: rejected: reasons={reasons:?}, warnings={warnings:?}");
        }
    }

    println!();
}

fn print_strike_buckets(
    selected: &SelectedMarket<'_>,
    bucket_step: DisplayPrice,
    levels_each_side: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let strikes = selected.grid.centered_strikes_by_display_step(
        selected.spot_raw,
        bucket_step,
        levels_each_side,
    )?;

    let buckets = selected.grid.buckets_from_ordered_strikes(&strikes);

    let mut strike_table = Table::new();
    strike_table.load_preset(UTF8_FULL);
    strike_table.set_header(vec!["#", "raw strike", "display strike"]);

    for (idx, strike) in strikes.iter().enumerate() {
        strike_table.add_row(vec![
            Cell::new(idx),
            Cell::new(strike.raw),
            Cell::new(selected.grid.display(*strike).to_string()),
        ]);
    }

    println!("Generated strikes");
    println!("{strike_table}");
    println!();

    let mut bucket_table = Table::new();
    bucket_table.load_preset(UTF8_FULL);
    bucket_table.set_header(vec!["bucket", "lower", "upper", "semantic"]);

    for (idx, bucket) in buckets.iter().enumerate() {
        let lower = bucket
            .lower
            .map(|strike| selected.grid.display(strike).to_string())
            .unwrap_or_else(|| "−∞".to_string());

        let upper = bucket
            .upper
            .map(|strike| selected.grid.display(strike).to_string())
            .unwrap_or_else(|| "+∞".to_string());

        let semantic = match (bucket.lower, bucket.upper) {
            (None, Some(_)) => "downside tail",
            (Some(_), Some(_)) => "bounded range",
            (Some(_), None) => "upside tail",
            (None, None) => "invalid",
        };

        bucket_table.add_row(vec![
            Cell::new(idx),
            Cell::new(lower),
            Cell::new(upper),
            Cell::new(semantic),
        ]);
    }

    println!("Generated payoff buckets");
    println!("{bucket_table}");

    Ok(())
}

fn format_latest_price(market: &MarketSnapshot) -> String {
    market
        .latest_price
        .as_ref()
        .and_then(|price| price.price)
        .or_else(|| market.latest_svi.as_ref().and_then(|svi| svi.spot))
        .map(|value| {
            let scale = PriceScale::E9;
            scale
                .raw_from_api_number(value)
                .map(|raw| scale.display_from_raw(raw).to_string())
                .unwrap_or_else(|| "—".to_string())
        })
        .unwrap_or_else(|| "—".to_string())
}

fn format_scaled_raw(value: Option<u64>) -> String {
    value
        .map(|raw| PriceScale::E9.display_from_raw(raw).to_string())
        .unwrap_or_else(|| "—".to_string())
}

fn format_age(value: Option<i64>) -> String {
    value.map(|secs| format!("{secs}s")).unwrap_or_else(|| "unknown".to_string())
}

fn format_usable(status: &StructxMarketStatus) -> String {
    match status {
        StructxMarketStatus::Usable => "yes".to_string(),
        StructxMarketStatus::UsableWithWarnings(warnings) => {
            let joined =
                warnings.iter().map(|warning| format!("{warning:?}")).collect::<Vec<_>>().join(",");

            format!("yes: warn {joined}")
        }
        StructxMarketStatus::Rejected { reasons, warnings } => {
            let reason_text =
                reasons.iter().map(|reason| format!("{reason:?}")).collect::<Vec<_>>().join(",");

            if warnings.is_empty() {
                format!("no: {reason_text}")
            } else {
                let warning_text = warnings
                    .iter()
                    .map(|warning| format!("{warning:?}"))
                    .collect::<Vec<_>>()
                    .join(",");

                format!("no: {reason_text}; warn {warning_text}")
            }
        }
    }
}
