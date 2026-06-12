use clap::{Parser, Subcommand};

use deepbook_client::{
    DeepBookClient, DeepBookConfig, FreshnessConfig, StructxMarketStatus,
    PREDICT_OBJECT_ID, PREDICT_SERVER_URL,
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
    ListMarkets,
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let cli = Cli::parse();

    let result = match cli.command {
        Command::ListMarkets => list_markets(cli.server_url, cli.predict_id).await,
    };

    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::ExitCode::FAILURE
        }
    }
}

async fn list_markets(
    server_url: String,
    predict_id: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = DeepBookClient::new(DeepBookConfig {
        server_url,
        predict_id,
    })?;

    let _status = client.status().await?;
    let _predict_state = client.predict_state().await?;
    let quote_assets = client.quote_assets().await?;
    let vault_summary = client.vault_summary().await?;

    println!("Protocol status: ok");
    println!("Predict state: ok");
    println!("Quote assets: {}", quote_assets.len());
    println!("Vault summary fetched: {}", vault_summary.is_present());
    println!();

    let markets = client.load_structx_markets(FreshnessConfig::default()).await?;

    for market in &markets {
        let usable = match &market.structx_status {
            StructxMarketStatus::Usable => "yes".to_string(),
            StructxMarketStatus::Rejected(reasons) => format!("no: {reasons:?}"),
        };

        println!(
            "oracle={} underlying={} status={} usable={}",
            market.oracle_id().unwrap_or("—"),
            market.underlying().unwrap_or("—"),
            market.status().unwrap_or("—"),
            usable
        );
    }

    let usable_count = markets
        .iter()
        .filter(|market| market.structx_status.is_usable())
        .count();

    println!();
    println!("BTC markets found: {}", markets.len());
    println!("StructX-usable markets: {usable_count}");

    Ok(())
}
