use anyhow::{anyhow, Context};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::market_catalog::{
    now_ms, CatalogMarketSnapshot, MarketCatalog, MarketCatalogSource, MarketCategory, MarketKind,
    MarketStatus,
};
use crate::market_store::MarketStore;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogBuildReport {
    pub total_input_items: usize,
    pub accepted_markets: usize,
    pub rejected_items: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogStatus {
    pub exists: bool,
    pub schema_version: Option<u32>,
    pub market_count: usize,
    pub active_market_count: usize,
    pub last_refreshed_at_ms: Option<u64>,
    pub age_ms: Option<u64>,
    pub source: Option<MarketCatalogSource>,
    pub warnings: Vec<String>,
}

pub fn build_catalog_from_markets_json(
    raw: &Value,
) -> anyhow::Result<(MarketCatalog, CatalogBuildReport)> {
    let now = now_ms();
    let items = extract_market_items(raw)?;

    let mut markets = Vec::new();
    let mut warnings = Vec::new();
    let mut rejected = 0usize;

    for (idx, item) in items.iter().enumerate() {
        match normalize_market_json(item, now) {
            Ok(market) => markets.push(market),
            Err(err) => {
                rejected += 1;
                warnings.push(format!("rejected market item #{idx}: {err}"));
            }
        }
    }

    let catalog = MarketCatalog {
        schema_version: MarketCatalog::SCHEMA_VERSION,
        markets,
        last_refreshed_at_ms: now,
        source: MarketCatalogSource::PredictServer,
    };

    let report = CatalogBuildReport {
        total_input_items: items.len(),
        accepted_markets: catalog.markets.len(),
        rejected_items: rejected,
        warnings,
    };

    Ok((catalog, report))
}

pub async fn load_or_refresh_catalog_from_json<S: MarketStore + ?Sized>(
    store: &S,
    raw: &Value,
    max_staleness_ms: u64,
) -> anyhow::Result<(MarketCatalog, CatalogBuildReport)> {
    if let Some(existing) = store.load_latest_catalog().await? {
        let now = now_ms();
        if !existing.is_stale(now, max_staleness_ms) {
            let report = CatalogBuildReport {
                total_input_items: existing.markets.len(),
                accepted_markets: existing.markets.len(),
                rejected_items: 0,
                warnings: vec!["loaded fresh catalog from disk cache".to_string()],
            };
            return Ok((existing, report));
        }
    }

    let (catalog, report) = build_catalog_from_markets_json(raw)?;
    store.save_catalog(&catalog).await?;
    Ok((catalog, report))
}

pub async fn load_catalog_status<S: MarketStore + ?Sized>(
    store: &S,
) -> anyhow::Result<CatalogStatus> {
    let now = now_ms();
    let Some(catalog) = store.load_latest_catalog().await? else {
        return Ok(CatalogStatus {
            exists: false,
            schema_version: None,
            market_count: 0,
            active_market_count: 0,
            last_refreshed_at_ms: None,
            age_ms: None,
            source: None,
            warnings: vec!["market catalog does not exist yet".to_string()],
        });
    };

    let active_count = catalog.markets.iter().filter(|m| m.status == MarketStatus::Active).count();

    Ok(CatalogStatus {
        exists: true,
        schema_version: Some(catalog.schema_version),
        market_count: catalog.markets.len(),
        active_market_count: active_count,
        last_refreshed_at_ms: Some(catalog.last_refreshed_at_ms),
        age_ms: Some(now.saturating_sub(catalog.last_refreshed_at_ms)),
        source: Some(catalog.source),
        warnings: vec![],
    })
}

pub async fn refresh_catalog_from_existing_markets_json<S: MarketStore + ?Sized>(
    store: &S,
    raw_markets_json: Value,
) -> anyhow::Result<(MarketCatalog, CatalogBuildReport)> {
    let (catalog, report) = build_catalog_from_markets_json(&raw_markets_json)?;
    store.save_catalog(&catalog).await.context("failed to persist refreshed market catalog")?;
    Ok((catalog, report))
}

pub fn normalize_market_json(
    raw: &Value,
    fetched_at_ms: u64,
) -> anyhow::Result<CatalogMarketSnapshot> {
    let oracle_id = first_string(
        raw,
        &[
            "oracle_id",
            "oracleId",
            "oracle.id",
            "oracle.object_id",
            "oracle.objectId",
            "id",
            "object_id",
            "objectId",
            "list_item.oracle_id",
            "state.oracle_id",
        ],
    )
    .ok_or_else(|| anyhow!("missing oracle id"))?;

    let underlying = first_string(
        raw,
        &[
            "underlying",
            "asset",
            "symbol",
            "base",
            "base_asset",
            "baseAsset",
            "market.underlying",
            "oracle.underlying",
            "list_item.underlying_asset",
            "state.underlying_asset",
        ],
    )
    .unwrap_or_else(|| infer_underlying_from_text(raw).unwrap_or_else(|| "UNKNOWN".to_string()));

    let display_name = first_string(
        raw,
        &["display_name", "displayName", "name", "title", "market_name", "marketName"],
    )
    .unwrap_or_else(|| format!("{} Predict Market", underlying.to_uppercase()));

    let expiry_ms = first_u64(
        raw,
        &[
            "expiry_ms",
            "expiryMs",
            "expiry",
            "expiry_timestamp_ms",
            "expiryTimestampMs",
            "oracle.expiry_ms",
            "oracle.expiryMs",
            "oracle.expiry",
            "list_item.expiry_ms",
            "state.expiry_ms",
        ],
    )
    .unwrap_or_default();

    let valid_strikes = first_u64_array(
        raw,
        &[
            "valid_strikes",
            "validStrikes",
            "strikes",
            "strike_grid",
            "strikeGrid",
            "oracle.valid_strikes",
            "oracle.validStrikes",
        ],
    )
    .unwrap_or_default();

    let min_strike = valid_strikes.iter().min().copied().or_else(|| {
        first_u64(
            raw,
            &[
                "min_strike",
                "minStrike",
                "strike_min",
                "strikeMin",
                "oracle.min_strike",
                "oracle.minStrike",
            ],
        )
    });
    let max_strike = valid_strikes.iter().max().copied().or_else(|| {
        first_u64(
            raw,
            &[
                "max_strike",
                "maxStrike",
                "strike_max",
                "strikeMax",
                "oracle.max_strike",
                "oracle.maxStrike",
                "state.max_strike",
            ],
        )
    });

    let spot = first_f64(
        raw,
        &[
            "spot",
            "price",
            "latest_price",
            "latestPrice",
            "latest_price.price",
            "latestPrice.price",
            "latest_price.value",
            "latestPrice.value",
            "latest_price.raw_price",
            "oracle.spot",
            "oracle.price",
            "latest_price.value",
            "latest_price.price",
        ],
    );

    let settlement_price = first_f64(
        raw,
        &[
            "settlement_price",
            "settlementPrice",
            "oracle.settlement_price",
            "oracle.settlementPrice",
        ],
    );

    let quote_assets = first_string_array(
        raw,
        &[
            "quote_assets",
            "quoteAssets",
            "supported_quote_assets",
            "supportedQuoteAssets",
            "quote_asset_symbols",
            "quoteAssetSymbols",
        ],
    )
    .filter(|assets| !assets.is_empty())
    .unwrap_or_else(|| vec!["DUSDC".to_string()]);

    let preferred_quote_asset = first_string(
        raw,
        &["preferred_quote_asset", "preferredQuoteAsset", "quote_asset", "quoteAsset"],
    )
    .unwrap_or_else(|| quote_assets.first().cloned().unwrap_or_else(|| "DUSDC".to_string()));

    let latest_price_updated_at_ms = first_u64(
        raw,
        &[
            "latest_price_updated_at_ms",
            "latestPriceUpdatedAtMs",
            "latest_price.timestamp_ms",
            "latestPrice.timestampMs",
            "latest_price.updated_at_ms",
            "latestPrice.updatedAtMs",
        ],
    );
    let svi_updated_at_ms = first_u64(
        raw,
        &[
            "svi_updated_at_ms",
            "sviUpdatedAtMs",
            "latest_svi.timestamp_ms",
            "latestSvi.timestampMs",
            "svi.timestamp_ms",
            "svi.timestampMs",
        ],
    );

    let status = infer_status(raw, expiry_ms, settlement_price, fetched_at_ms);
    let category = infer_category(&underlying, &display_name);
    let market_kind = infer_market_kind(&underlying, &display_name, &valid_strikes);

    let mut tags = vec![
        underlying.to_ascii_lowercase(),
        display_name.to_ascii_lowercase(),
        preferred_quote_asset.to_ascii_lowercase(),
    ];
    if category == MarketCategory::Crypto {
        tags.push("crypto".to_string());
    }

    let market_id = first_string(raw, &["market_id", "marketId", "id"])
        .unwrap_or_else(|| format!("{}:{}", underlying.to_ascii_uppercase(), oracle_id));

    Ok(CatalogMarketSnapshot {
        market_id,
        oracle_id,
        underlying: underlying.to_ascii_uppercase(),
        display_name,
        category,
        market_kind,
        expiry_ms,
        status,
        spot,
        settlement_price,
        valid_strikes,
        min_strike,
        max_strike,
        quote_assets,
        preferred_quote_asset,
        latest_price_updated_at_ms,
        svi_updated_at_ms,
        fetched_at_ms,
        tags,
        metadata: raw.clone(),
    })
}

fn extract_market_items(raw: &Value) -> anyhow::Result<Vec<Value>> {
    if let Some(arr) = raw.as_array() {
        return Ok(arr.clone());
    }

    for path in [
        "markets",
        "oracles",
        "items",
        "data",
        "data.markets",
        "data.oracles",
        "result",
        "result.markets",
        "result.oracles",
    ] {
        if let Some(value) = get_path(raw, path) {
            if let Some(arr) = value.as_array() {
                return Ok(arr.clone());
            }
        }
    }

    Err(anyhow!("could not find market array in JSON; expected one of markets/oracles/data/result"))
}

fn infer_status(
    raw: &Value,
    expiry_ms: u64,
    settlement_price: Option<f64>,
    now: u64,
) -> MarketStatus {
    let status_text = first_string(
        raw,
        &[
            "status",
            "state",
            "oracle_status",
            "oracleStatus",
            "oracle.status",
            "oracle.state",
            "state.status",
            "list_item.status",
        ],
    )
    .unwrap_or_default()
    .to_ascii_lowercase();

    if status_text.contains("pending") {
        return MarketStatus::PendingSettlement;
    }
    if status_text.contains("settled") {
        return MarketStatus::Settled;
    }
    if status_text.contains("active") || status_text.contains("live") {
        return MarketStatus::Active;
    }
    if status_text.contains("inactive") || status_text.contains("disabled") {
        return MarketStatus::Inactive;
    }
    if settlement_price.is_some() {
        return MarketStatus::Settled;
    }
    if expiry_ms > 0 && expiry_ms <= now {
        return MarketStatus::ExpiredUnknown;
    }
    if expiry_ms > now {
        return MarketStatus::Active;
    }
    MarketStatus::Unknown
}

fn infer_category(underlying: &str, display_name: &str) -> MarketCategory {
    let text = format!("{} {}", underlying, display_name).to_ascii_lowercase();
    if contains_any(&text, &["btc", "bitcoin", "eth", "ethereum", "sui", "sol", "crypto"]) {
        return MarketCategory::Crypto;
    }
    if contains_any(&text, &["cpi", "inflation", "fed", "rate", "macro"]) {
        return MarketCategory::Macro;
    }
    if contains_any(&text, &["election", "president", "senate", "politic"]) {
        return MarketCategory::Politics;
    }
    if contains_any(&text, &["nba", "nfl", "cricket", "football", "tennis", "sport"]) {
        return MarketCategory::Sports;
    }
    if contains_any(&text, &["nasdaq", "spx", "s&p", "stock", "gold", "oil"]) {
        return MarketCategory::Finance;
    }
    MarketCategory::Unknown
}

fn infer_market_kind(underlying: &str, display_name: &str, valid_strikes: &[u64]) -> MarketKind {
    let text = format!("{} {}", underlying, display_name).to_ascii_lowercase();

    if contains_any(&text, &["yes", "no", "will ", "wins", "win "]) {
        return MarketKind::BinaryEvent;
    }

    if contains_any(&text, &["btc", "bitcoin", "eth", "ethereum", "sui", "sol", "solana", "price"])
    {
        return MarketKind::ScalarPrice;
    }

    if !valid_strikes.is_empty() {
        return MarketKind::ScalarEvent;
    }

    MarketKind::Unknown
}

fn infer_underlying_from_text(raw: &Value) -> Option<String> {
    let text = raw.to_string().to_ascii_lowercase();
    if contains_any(&text, &["bitcoin", "btc"]) {
        Some("BTC".to_string())
    } else if contains_any(&text, &["ethereum", "eth"]) {
        Some("ETH".to_string())
    } else if contains_any(&text, &["sui"]) {
        Some("SUI".to_string())
    } else {
        None
    }
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

fn first_string(raw: &Value, paths: &[&str]) -> Option<String> {
    paths.iter().find_map(|path| {
        let value = get_path(raw, path)?;
        if let Some(s) = value.as_str() {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
        if value.is_number() {
            return Some(value.to_string());
        }
        None
    })
}

fn first_u64(raw: &Value, paths: &[&str]) -> Option<u64> {
    paths.iter().find_map(|path| parse_u64_value(get_path(raw, path)?))
}

fn first_f64(raw: &Value, paths: &[&str]) -> Option<f64> {
    paths.iter().find_map(|path| parse_f64_value(get_path(raw, path)?))
}

fn first_u64_array(raw: &Value, paths: &[&str]) -> Option<Vec<u64>> {
    paths.iter().find_map(|path| {
        let arr = get_path(raw, path)?.as_array()?;
        let values: Vec<u64> = arr.iter().filter_map(parse_u64_value).collect();
        Some(values)
    })
}

fn first_string_array(raw: &Value, paths: &[&str]) -> Option<Vec<String>> {
    paths.iter().find_map(|path| {
        let arr = get_path(raw, path)?.as_array()?;
        let values: Vec<String> = arr
            .iter()
            .filter_map(|value| value.as_str().map(|s| s.trim().to_string()))
            .filter(|s| !s.is_empty())
            .collect();
        Some(values)
    })
}

fn parse_u64_value(value: &Value) -> Option<u64> {
    if let Some(n) = value.as_u64() {
        return Some(n);
    }
    if let Some(n) = value.as_i64() {
        return u64::try_from(n).ok();
    }
    if let Some(f) = value.as_f64() {
        if f.is_finite() && f >= 0.0 {
            return Some(f.round() as u64);
        }
    }
    if let Some(s) = value.as_str() {
        return s.replace('_', "").parse::<u64>().ok();
    }
    None
}

fn parse_f64_value(value: &Value) -> Option<f64> {
    if let Some(f) = value.as_f64() {
        return Some(f);
    }
    if let Some(n) = value.as_i64() {
        return Some(n as f64);
    }
    if let Some(n) = value.as_u64() {
        return Some(n as f64);
    }
    if let Some(s) = value.as_str() {
        return s.replace(',', "").parse::<f64>().ok();
    }
    None
}

fn get_path<'a>(raw: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = raw;
    for part in path.split('.') {
        current = current.get(part)?;
    }
    Some(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_basic_btc_market_json() {
        let raw = serde_json::json!({
            "oracle_id": "0xabc",
            "underlying": "BTC",
            "display_name": "BTC Weekly",
            "expiry_ms": 1_900_000_000_000_u64,
            "status": "active",
            "latest_price": { "price": 100000.0, "timestamp_ms": 1_800_000_000_000_u64 },
            "valid_strikes": [90000_u64, 100000_u64, 110000_u64],
            "quote_assets": ["DUSDC"]
        });

        let market = normalize_market_json(&raw, 1_800_000_000_000).unwrap();
        assert_eq!(market.oracle_id, "0xabc");
        assert_eq!(market.underlying, "BTC");
        assert_eq!(market.category, MarketCategory::Crypto);
        assert_eq!(market.market_kind, MarketKind::ScalarPrice);
        assert_eq!(market.status, MarketStatus::Active);
        assert_eq!(market.valid_strikes.len(), 3);
        assert_eq!(market.preferred_quote_asset, "DUSDC");
    }

    #[test]
    fn builds_catalog_from_oracles_array() {
        let raw = serde_json::json!({
            "oracles": [
                {
                    "oracleId": "0xbtc",
                    "symbol": "BTC",
                    "expiryMs": 1_900_000_000_000_u64,
                    "state": "active",
                    "strikes": [90, 100, 110],
                    "quoteAssets": ["DUSDC"]
                }
            ]
        });

        let (catalog, report) = build_catalog_from_markets_json(&raw).unwrap();
        assert_eq!(report.total_input_items, 1);
        assert_eq!(report.accepted_markets, 1);
        assert_eq!(catalog.markets[0].underlying, "BTC");
    }
}
