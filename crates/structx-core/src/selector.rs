use chrono::{DateTime, Utc};
use deepbook_client::{MarketSnapshot, StructxMarketStatus};
use thiserror::Error;

use crate::price::{DisplayPrice, PriceScale};
use crate::strike_grid::{StrikeGrid, StrikeGridError};

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum MarketSelectionError {
    #[error("no StructX-usable market found")]
    NoUsableMarket,

    #[error("selected market is missing oracle id")]
    MissingOracleId,

    #[error("selected market is missing spot/latest price")]
    MissingSpot,

    #[error("selected market is missing expiry")]
    MissingExpiry,

    #[error("strike grid error: {0}")]
    StrikeGrid(#[from] StrikeGridError),
}

#[derive(Debug)]
pub struct SelectedMarket<'a> {
    pub market: &'a MarketSnapshot,
    pub oracle_id: &'a str,
    pub expiry: DateTime<Utc>,
    pub spot_raw: u64,
    pub spot_display: DisplayPrice,
    pub grid: StrikeGrid,
}

pub fn select_candidate_markets<'a>(
    markets: &'a [MarketSnapshot],
    scale: PriceScale,
) -> Vec<SelectedMarket<'a>> {
    let mut candidates = markets
        .iter()
        .filter(|market| market.structx_status.is_usable())
        .filter_map(|market| build_candidate(market, scale).ok())
        .collect::<Vec<_>>();

    candidates.sort_by(|a, b| {
        market_status_rank(&a.market.structx_status)
            .cmp(&market_status_rank(&b.market.structx_status))
            .then_with(|| a.expiry.cmp(&b.expiry))
    });

    candidates
}

pub fn select_best_market<'a>(
    markets: &'a [MarketSnapshot],
    scale: PriceScale,
) -> Result<SelectedMarket<'a>, MarketSelectionError> {
    select_candidate_markets(markets, scale)
        .into_iter()
        .next()
        .ok_or(MarketSelectionError::NoUsableMarket)
}

fn build_candidate<'a>(
    market: &'a MarketSnapshot,
    scale: PriceScale,
) -> Result<SelectedMarket<'a>, MarketSelectionError> {
    let oracle_id = market.oracle_id().ok_or(MarketSelectionError::MissingOracleId)?;

    let expiry = market.expiry_datetime().ok_or(MarketSelectionError::MissingExpiry)?;

    let spot_api_value = market
        .latest_price
        .as_ref()
        .and_then(|price| price.price)
        .or_else(|| market.latest_svi.as_ref().and_then(|svi| svi.spot))
        .ok_or(MarketSelectionError::MissingSpot)?;

    let spot_raw =
        scale.raw_from_api_number(spot_api_value).ok_or(MarketSelectionError::MissingSpot)?;

    let min_raw = market.min_strike().ok_or(StrikeGridError::MissingMinStrike)?;

    let tick_size_raw = market.tick_size().ok_or(StrikeGridError::MissingTickSize)?;

    let max_raw = market.state.as_ref().and_then(|state| state.max_strike);

    let grid = StrikeGrid::new(min_raw, max_raw, tick_size_raw, scale)?;
    let spot_display = scale.display_from_raw(spot_raw);

    Ok(SelectedMarket { market, oracle_id, expiry, spot_raw, spot_display, grid })
}

fn market_status_rank(status: &StructxMarketStatus) -> u8 {
    match status {
        StructxMarketStatus::Usable => 0,
        StructxMarketStatus::UsableWithWarnings(_) => 1,
        StructxMarketStatus::Rejected { .. } => 2,
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone};
    use deepbook_client::{
        AskBounds, LatestPrice, LatestSvi, OracleListItem, OracleState, StructxMarketStatus,
    };
    use serde_json::json;

    use super::*;

    fn market(
        oracle_id: &str,
        expiry_offset: Duration,
        status: StructxMarketStatus,
    ) -> MarketSnapshot {
        let now = Utc.timestamp_millis_opt(1_900_000_000_000).single().expect("valid timestamp");

        MarketSnapshot {
            list_item: OracleListItem {
                oracle_id: Some(oracle_id.to_string()),
                underlying_asset: Some("BTC".to_string()),
                status: Some("active".to_string()),
                expiry_ms: Some((now + expiry_offset).timestamp_millis()),
                extra: Default::default(),
            },
            state: Some(OracleState {
                oracle_id: Some(oracle_id.to_string()),
                underlying_asset: Some("BTC".to_string()),
                status: Some("active".to_string()),
                expiry_ms: Some((now + expiry_offset).timestamp_millis()),
                min_strike: Some(50_000_000_000_000),
                max_strike: Some(90_000_000_000_000),
                tick_size: Some(1_000_000_000),
                raw: json!({}),
            }),
            latest_price: Some(LatestPrice {
                timestamp_ms: Some(now.timestamp_millis()),
                price: Some(62_773_927_561_148.0),
                raw: json!({}),
            }),
            latest_svi: Some(LatestSvi {
                timestamp_ms: Some(now.timestamp_millis()),
                spot: Some(62_773_927_561_148.0),
                forward: Some(62_800_000_000_000.0),
                raw: json!({}),
            }),
            ask_bounds: Some(AskBounds { raw: json!({}) }),
            structx_status: status,
        }
    }

    #[test]
    fn selects_earliest_usable_market() {
        let markets = vec![
            market("0xlate", Duration::hours(2), StructxMarketStatus::Usable),
            market("0xsoon", Duration::hours(1), StructxMarketStatus::Usable),
        ];

        let selected = select_best_market(&markets, PriceScale::E9).expect("market selected");

        assert_eq!(selected.oracle_id, "0xsoon");
    }

    #[test]
    fn prefers_clean_usable_over_warning_market() {
        let markets = vec![
            market(
                "0xwarn",
                Duration::minutes(30),
                StructxMarketStatus::UsableWithWarnings(vec![]),
            ),
            market("0xclean", Duration::hours(3), StructxMarketStatus::Usable),
        ];

        let selected = select_best_market(&markets, PriceScale::E9).expect("market selected");

        assert_eq!(selected.oracle_id, "0xclean");
    }

    #[test]
    fn prefers_farther_expiry_within_same_status_bucket() {
        let markets = vec![
            market("0xnear", Duration::hours(2), StructxMarketStatus::UsableWithWarnings(vec![])),
            market("0xfar", Duration::hours(8), StructxMarketStatus::UsableWithWarnings(vec![])),
        ];

        let selected = select_best_market(&markets, PriceScale::E9).expect("market selected");

        assert_eq!(selected.oracle_id, "0xfar");
    }

    #[test]
    fn builds_selected_market_grid_and_spot() {
        let markets = vec![market("0xabc", Duration::hours(1), StructxMarketStatus::Usable)];

        let selected = select_best_market(&markets, PriceScale::E9).expect("market selected");

        assert_eq!(selected.spot_raw, 62_773_927_561_148);
        assert_eq!(selected.grid.min_raw, 50_000_000_000_000);
        assert_eq!(selected.grid.tick_size_raw, 1_000_000_000);
    }
}
