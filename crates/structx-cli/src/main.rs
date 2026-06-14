use std::io;
use std::time::Duration as StdDuration;

use chrono::{Duration, Utc};
use clap::{Parser, Subcommand};
use comfy_table::{presets::UTF8_FULL, Cell, Table};

use deepbook_client::{
    DeepBookClient, DeepBookConfig, FreshnessConfig, MarketSnapshot, StructxMarketStatus,
    PREDICT_OBJECT_ID, PREDICT_SERVER_URL,
};
use structx_core::{
    build_quote_plan, compile_breakout, select_best_market, CompiledPayoff, DisplayPrice,
    PredictLeg, PriceScale, QuoteCall, QuotePlan, SelectedMarket, Strike,
};
#[derive(Debug, Parser)]
#[command(name = "structx")]
#[command(about = "StructX CLI for DeepBook Predict market inspection")]
struct Cli {
    #[arg(long, default_value = PREDICT_SERVER_URL)]
    server_url: String,

    #[arg(long, default_value = PREDICT_OBJECT_ID)]
    predict_id: String,

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
