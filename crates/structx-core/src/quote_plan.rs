use deepbook_client::{PREDICT_OBJECT_ID, PREDICT_PACKAGE_ID};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::payoff::{BinaryDirection, CompiledPayoff, PredictLeg};
use crate::selector::SelectedMarket;
use crate::strike_grid::Strike;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuoteTarget {
    pub package_id: String,
    pub predict_object_id: String,
    pub module: String,
}

impl Default for QuoteTarget {
    fn default() -> Self {
        Self {
            package_id: PREDICT_PACKAGE_ID.to_string(),
            predict_object_id: PREDICT_OBJECT_ID.to_string(),
            module: "predict".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuoteFunction {
    GetTradeAmounts,
    GetRangeTradeAmounts,
}

impl QuoteFunction {
    #[must_use]
    pub fn move_function_name(self) -> &'static str {
        match self {
            Self::GetTradeAmounts => "get_trade_amounts",
            Self::GetRangeTradeAmounts => "get_range_trade_amounts",
        }
    }
}

impl std::fmt::Display for QuoteFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.move_function_name())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuoteCall {
    Binary {
        function: QuoteFunction,
        oracle_id: String,
        expiry_ms: i64,
        direction: BinaryDirection,
        strike: Strike,
        quantity: u64,
    },
    Range {
        function: QuoteFunction,
        oracle_id: String,
        expiry_ms: i64,
        lower: Strike,
        upper: Strike,
        quantity: u64,
    },
}

impl QuoteCall {
    #[must_use]
    pub fn function(&self) -> QuoteFunction {
        match self {
            Self::Binary { function, .. } | Self::Range { function, .. } => *function,
        }
    }

    #[must_use]
    pub fn quantity(&self) -> u64 {
        match self {
            Self::Binary { quantity, .. } | Self::Range { quantity, .. } => *quantity,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuotePlan {
    pub target: QuoteTarget,
    pub oracle_id: String,
    pub expiry_ms: i64,
    pub calls: Vec<QuoteCall>,
    pub max_payout_quantity: u64,
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum QuotePlanError {
    #[error("selected market is missing oracle id")]
    MissingOracleId,

    #[error("selected market is missing expiry")]
    MissingExpiry,

    #[error("compiled payoff has no quoteable legs")]
    EmptyLegs,
}

pub fn build_quote_plan(
    selected: &SelectedMarket<'_>,
    compiled: &CompiledPayoff,
) -> Result<QuotePlan, QuotePlanError> {
    if compiled.legs.is_empty() {
        return Err(QuotePlanError::EmptyLegs);
    }

    let oracle_id = selected.oracle_id.to_string();
    let expiry_ms = selected.market.expiry_ms().ok_or(QuotePlanError::MissingExpiry)?;

    let calls = compiled
        .legs
        .iter()
        .map(|leg| quote_call_from_leg(&oracle_id, expiry_ms, leg))
        .collect::<Vec<_>>();

    Ok(QuotePlan {
        target: QuoteTarget::default(),
        oracle_id,
        expiry_ms,
        calls,
        max_payout_quantity: compiled.max_payout_quantity,
    })
}

fn quote_call_from_leg(oracle_id: &str, expiry_ms: i64, leg: &PredictLeg) -> QuoteCall {
    match leg {
        PredictLeg::Binary { direction, strike, quantity } => QuoteCall::Binary {
            function: QuoteFunction::GetTradeAmounts,
            oracle_id: oracle_id.to_string(),
            expiry_ms,
            direction: *direction,
            strike: *strike,
            quantity: *quantity,
        },
        PredictLeg::Range { lower, upper, quantity } => QuoteCall::Range {
            function: QuoteFunction::GetRangeTradeAmounts,
            oracle_id: oracle_id.to_string(),
            expiry_ms,
            lower: *lower,
            upper: *upper,
            quantity: *quantity,
        },
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone, Utc};
    use deepbook_client::{
        AskBounds, LatestPrice, LatestSvi, OracleListItem, OracleState, StructxMarketStatus,
    };
    use serde_json::json;

    use super::*;
    use crate::payoff::{compile_breakout, BinaryDirection};
    use crate::price::{DisplayPrice, PriceScale};
    use crate::selector::SelectedMarket;
    use crate::strike_grid::{Strike, StrikeGrid};

    fn strike(raw: u64) -> Strike {
        Strike { raw }
    }

    fn selected_market<'a>(market: &'a deepbook_client::MarketSnapshot) -> SelectedMarket<'a> {
        SelectedMarket {
            market,
            oracle_id: "0xoracle",
            expiry: Utc
                .timestamp_millis_opt(1_900_000_000_000 + Duration::hours(1).num_milliseconds())
                .single()
                .expect("valid timestamp"),
            spot_raw: 62_900_000_000_000,
            spot_display: DisplayPrice(62_900.0),
            grid: StrikeGrid::new(
                50_000_000_000_000,
                Some(90_000_000_000_000),
                1_000_000_000,
                PriceScale::E9,
            )
            .expect("grid builds"),
        }
    }

    fn market_snapshot() -> deepbook_client::MarketSnapshot {
        let now = Utc.timestamp_millis_opt(1_900_000_000_000).single().expect("valid timestamp");

        deepbook_client::MarketSnapshot {
            list_item: OracleListItem {
                oracle_id: Some("0xoracle".to_string()),
                underlying_asset: Some("BTC".to_string()),
                status: Some("active".to_string()),
                expiry_ms: Some((now + Duration::hours(1)).timestamp_millis()),
                extra: Default::default(),
            },
            state: Some(OracleState {
                oracle_id: Some("0xoracle".to_string()),
                underlying_asset: Some("BTC".to_string()),
                status: Some("active".to_string()),
                expiry_ms: Some((now + Duration::hours(1)).timestamp_millis()),
                min_strike: Some(50_000_000_000_000),
                max_strike: Some(90_000_000_000_000),
                tick_size: Some(1_000_000_000),
                raw: json!({}),
            }),
            latest_price: Some(LatestPrice {
                timestamp_ms: Some(now.timestamp_millis()),
                price: Some(62_900_000_000_000.0),
                raw: json!({}),
            }),
            latest_svi: Some(LatestSvi {
                timestamp_ms: Some(now.timestamp_millis()),
                spot: Some(62_900_000_000_000.0),
                forward: Some(63_000_000_000_000.0),
                raw: json!({}),
            }),
            ask_bounds: Some(AskBounds { raw: json!({}) }),
            structx_status: StructxMarketStatus::Usable,
        }
    }

    #[test]
    fn maps_breakout_legs_to_quote_calls() {
        let market = market_snapshot();
        let selected = selected_market(&market);

        let compiled = compile_breakout(
            strike(62_400_000_000_000),
            strike(62_650_000_000_000),
            strike(63_150_000_000_000),
            strike(63_400_000_000_000),
            1_000,
            400,
        )
        .expect("breakout compiles");

        let plan = build_quote_plan(&selected, &compiled).expect("quote plan builds");

        assert_eq!(plan.calls.len(), 4);
        assert_eq!(plan.max_payout_quantity, 1_000);

        assert!(matches!(
            plan.calls[0],
            QuoteCall::Binary {
                function: QuoteFunction::GetTradeAmounts,
                direction: BinaryDirection::Down,
                quantity: 1_000,
                ..
            }
        ));

        assert!(matches!(
            plan.calls[1],
            QuoteCall::Range { function: QuoteFunction::GetRangeTradeAmounts, quantity: 400, .. }
        ));

        assert!(matches!(
            plan.calls[2],
            QuoteCall::Range { function: QuoteFunction::GetRangeTradeAmounts, quantity: 400, .. }
        ));

        assert!(matches!(
            plan.calls[3],
            QuoteCall::Binary {
                function: QuoteFunction::GetTradeAmounts,
                direction: BinaryDirection::Up,
                quantity: 1_000,
                ..
            }
        ));
    }

    #[test]
    fn rejects_empty_quote_plan() {
        let market = market_snapshot();
        let selected = selected_market(&market);

        let compiled = CompiledPayoff { buckets: vec![], legs: vec![], max_payout_quantity: 0 };

        let err = build_quote_plan(&selected, &compiled).expect_err("empty plan should fail");
        assert_eq!(err, QuotePlanError::EmptyLegs);
    }
}
