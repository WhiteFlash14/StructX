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

pub fn select_best_market<'a>(
    markets: &'a [MarketSnapshot],
    scale: PriceScale,
) -> Result<SelectedMarket<'a>, MarketSelectionError> {
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

    candidates.into_iter().next().ok_or(MarketSelectionError::NoUsableMarket)
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
