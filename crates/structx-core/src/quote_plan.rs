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
