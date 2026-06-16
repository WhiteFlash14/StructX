use std::io;
use std::time::Duration as StdDuration;

use chrono::{Duration, Utc};
use clap::{Parser, Subcommand};
use comfy_table::{presets::UTF8_FULL, Cell, Table};

use deepbook_client::{
    verify_predict_abi, AbiCheckStatus, AbiVerificationReport, DeepBookClient, DeepBookConfig,
    DUSDC_DECIMALS, FreshnessConfig, MarketSnapshot, ObjectOwnerKind, StructxMarketStatus,
    SuiObjectInfo, SuiRpcClient, DEFAULT_SUI_TESTNET_RPC_URL, PREDICT_OBJECT_ID,
    PREDICT_PACKAGE_ID, PREDICT_SERVER_URL, SUI_CLOCK_OBJECT_ID,
};use structx_core::{
    build_quote_plan, build_quote_tx_kind, compile_breakout, select_best_market, CompiledPayoff,
    DisplayPrice, PredictLeg, PriceScale, QuoteAssetDisplay, QuoteCall, QuoteObjectRefs,
    QuotePlan, QuotePreview, QuotePreviewLeg, QuoteTxKind, SelectedMarket, Strike,
};#[derive(Debug, Parser)]
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
}

async fn devinspect_quote_breakout_command(
    args: DevinspectQuoteBreakoutArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = build_client(args.server_url, args.predict_id)?;
    let markets = load_markets(&client, args.freshness).await?;
    let selected = select_best_market(&markets, PriceScale::E9)?;

    print_selected_market(&selected);

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
    let plan = build_quote_plan(&selected, &compiled)?;

    print_breakout_boundaries(&selected, k1, k2, k3, k4);
    print_compiled_payoff(&selected, &compiled);
    print_quote_plan(&selected, &plan);

    let rpc = SuiRpcClient::new(args.rpc_url, StdDuration::from_secs(20))?;

    let predict = resolve_sui_object(&rpc, PREDICT_OBJECT_ID).await?;
    let oracle = resolve_sui_object(&rpc, selected.oracle_id).await?;
    let clock = resolve_sui_object(&rpc, SUI_CLOCK_OBJECT_ID).await?;

    validate_quote_object_refs(&predict, &oracle, &clock)?;

    let tx_kind = build_quote_tx_kind(
        &plan,
        QuoteObjectRefs { predict: &predict, oracle: &oracle, clock: &clock },
        &args.sender,
    )?;

    print_quote_tx_kind(&tx_kind);
    print_quote_call_execution_plan(&plan);

    let response = rpc.dev_inspect_transaction_kind(&tx_kind.sender, &tx_kind.tx_kind_b64).await?;

    print_devinspect_quote_response(&selected, &plan, &tx_kind, &response)?;

    Ok(())
}

fn print_quote_call_execution_plan(plan: &QuotePlan) {
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec!["#", "function", "leg", "strike/lower", "upper", "quantity"]);

    for (idx, call) in plan.calls.iter().enumerate() {
        match call {
            QuoteCall::Binary { function, direction, strike, quantity, .. } => {
                table.add_row(vec![
                    Cell::new(idx),
                    Cell::new(function.to_string()),
                    Cell::new(format!("{direction:?}_binary")),
                    Cell::new(strike.raw),
                    Cell::new("—"),
                    Cell::new(quantity),
                ]);
            }
            QuoteCall::Range { function, lower, upper, quantity, .. } => {
                table.add_row(vec![
                    Cell::new(idx),
                    Cell::new(function.to_string()),
                    Cell::new("range"),
                    Cell::new(lower.raw),
                    Cell::new(upper.raw),
                    Cell::new(quantity),
                ]);
            }
        }
    }

    println!("devInspect quote call execution plan");
    println!("{table}");
    println!();
}

fn print_devinspect_quote_response(
    selected: &SelectedMarket<'_>,
    plan: &QuotePlan,
    tx_kind: &QuoteTxKind,
    response: &serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    print_devinspect_response_summary(response)?;

    let results = response
        .get("results")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing devInspect results"))?;

    let asset = QuoteAssetDisplay {
        symbol: "dUSDC".to_string(),
        decimals: DUSDC_DECIMALS,
    };

    let mut preview_legs = Vec::with_capacity(plan.calls.len());

    for (quote_idx, call) in plan.calls.iter().enumerate() {
        let command_idx = tx_kind
            .quote_result_command_indices
            .get(quote_idx)
            .ok_or_else(|| {
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
            .ok_or_else(|| {
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
            QuoteCall::Binary {
                function,
                direction,
                strike,
                quantity,
                ..
            } => preview_legs.push(QuotePreviewLeg {
                index: quote_idx,
                function: function.to_string(),
                leg: format!("{direction}_binary"),
                strike_or_lower: selected.grid.display(*strike).to_string(),
                upper: None,
                quantity: *quantity,
                mint_cost_raw,
                redeem_payout_raw,
            }),
            QuoteCall::Range {
                function,
                lower,
                upper,
                quantity,
                ..
            } => preview_legs.push(QuotePreviewLeg {
                index: quote_idx,
                function: function.to_string(),
                leg: "range".to_string(),
                strike_or_lower: selected.grid.display(*lower).to_string(),
                upper: Some(selected.grid.display(*upper).to_string()),
                quantity: *quantity,
                mint_cost_raw,
                redeem_payout_raw,
            }),
        }
    }

    let preview = QuotePreview::new(asset, preview_legs);
    print_quote_preview(&preview);

    Ok(())
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
    println!(
        "total redeem payout raw: {}",
        preview.total_redeem_payout_raw
    );
    println!(
        "total redeem payout: {}",
        preview.total_redeem_payout_display()
    );
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
        println!("devInspect response:");
        println!("{}", compact_json_preview(response)?);
        return Err(io::Error::other("devInspect quote preview failed").into());
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

fn compact_json_preview(response: &serde_json::Value) -> Result<String, serde_json::Error> {
    let pretty = serde_json::to_string_pretty(response)?;

    const MAX_CHARS: usize = 8_000;

    if pretty.len() <= MAX_CHARS {
        return Ok(pretty);
    }

    let mut preview = pretty.chars().take(MAX_CHARS).collect::<String>();
    preview.push_str("\n... truncated ...");

    Ok(preview)
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
