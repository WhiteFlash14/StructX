use std::time::Duration as StdDuration;

use chrono::{Duration, Utc};
use clap::{Parser, Subcommand};
use comfy_table::{presets::UTF8_FULL, Cell, Table};

use deepbook_client::{
    DeepBookClient, DeepBookConfig, FreshnessConfig, MarketSnapshot, StructxMarketStatus,
    PREDICT_OBJECT_ID, PREDICT_SERVER_URL,
};
use structx_core::{select_best_market, PriceScale, SelectedMarket};

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
        } => {
            let freshness = build_freshness(
                max_price_age_secs,
                max_svi_age_secs,
                min_time_to_expiry_secs,
                strict_freshness,
            );

            select_market(cli.server_url, cli.predict_id, freshness).await
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
) -> Result<(), Box<dyn std::error::Error>> {
    let client = build_client(server_url, predict_id)?;
    let markets = load_markets(&client, freshness).await?;

    let selected = select_best_market(&markets, PriceScale::E9)?;

    print_selected_market(&selected);

    Ok(())
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
            Cell::new(format_scaled_price_raw(market.min_strike())),
            Cell::new(format_scaled_price_raw(market.tick_size())),
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

fn format_latest_price(market: &MarketSnapshot) -> String {
    market
        .latest_price
        .as_ref()
        .and_then(|price| price.price)
        .or_else(|| market.latest_svi.as_ref().and_then(|svi| svi.spot))
        .map(format_price_like_value)
        .unwrap_or_else(|| "—".to_string())
}

fn format_price_like_value(value: f64) -> String {
    // DeepBook Predict BTC values in the current Testnet data are raw 1e9-scaled.
    // If already human-sized, leave them alone.
    if value.abs() >= 1_000_000_000.0 {
        format!("{:.2}", value / 1_000_000_000.0)
    } else {
        format!("{value:.4}")
    }
}

fn format_scaled_price_raw(value: Option<u64>) -> String {
    value
        .map(|v| {
            if v >= 1_000_000_000 {
                format!("{:.2}", v as f64 / 1_000_000_000.0)
            } else {
                v.to_string()
            }
        })
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
