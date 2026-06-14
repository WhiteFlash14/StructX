use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::strike_grid::{Strike, StrikeBucket};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryDirection {
    Up,
    Down,
}

impl std::fmt::Display for BinaryDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Up => write!(f, "up"),
            Self::Down => write!(f, "down"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PredictLeg {
    Binary { direction: BinaryDirection, strike: Strike, quantity: u64 },
    Range { lower: Strike, upper: Strike, quantity: u64 },
}

impl PredictLeg {
    #[must_use]
    pub fn quantity(&self) -> u64 {
        match self {
            Self::Binary { quantity, .. } | Self::Range { quantity, .. } => *quantity,
        }
    }

    #[must_use]
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Binary { direction: BinaryDirection::Down, .. } => "down_binary",
            Self::Binary { direction: BinaryDirection::Up, .. } => "up_binary",
            Self::Range { .. } => "range",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PayoffBucket {
    pub lower: Option<Strike>,
    pub upper: Option<Strike>,
    pub payout_quantity: u64,
}

impl PayoffBucket {
    #[must_use]
    pub fn new(lower: Option<Strike>, upper: Option<Strike>, payout_quantity: u64) -> Self {
        Self { lower, upper, payout_quantity }
    }

    #[must_use]
    pub fn from_strike_bucket(bucket: StrikeBucket, payout_quantity: u64) -> Self {
        Self { lower: bucket.lower, upper: bucket.upper, payout_quantity }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledPayoff {
    pub buckets: Vec<PayoffBucket>,
    pub legs: Vec<PredictLeg>,
    pub max_payout_quantity: u64,
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum PayoffCompileError {
    #[error("payoff has no paying buckets")]
    NoPayingBuckets,

    #[error("invalid bucket: lower and upper are both missing")]
    InvalidOpenBucket,

    #[error("invalid range: lower strike {lower} must be less than upper strike {upper}")]
    InvalidRange { lower: u64, upper: u64 },

    #[error("invalid breakout strikes: expected k1 < k2 < k3 < k4")]
    InvalidBreakoutStrikes,
}

pub fn compile_bucket_payoff(
    buckets: &[PayoffBucket],
) -> Result<CompiledPayoff, PayoffCompileError> {
    let mut legs = Vec::new();

    for bucket in buckets {
        validate_bucket_shape(bucket)?;

        if bucket.payout_quantity == 0 {
            continue;
        }

        match (bucket.lower, bucket.upper) {
            (None, Some(upper)) => {
                legs.push(PredictLeg::Binary {
                    direction: BinaryDirection::Down,
                    strike: upper,
                    quantity: bucket.payout_quantity,
                });
            }
            (Some(lower), Some(upper)) => {
                legs.push(PredictLeg::Range { lower, upper, quantity: bucket.payout_quantity });
            }
            (Some(lower), None) => {
                legs.push(PredictLeg::Binary {
                    direction: BinaryDirection::Up,
                    strike: lower,
                    quantity: bucket.payout_quantity,
                });
            }
            (None, None) => return Err(PayoffCompileError::InvalidOpenBucket),
        }
    }

    if legs.is_empty() {
        return Err(PayoffCompileError::NoPayingBuckets);
    }

    let max_payout_quantity =
        buckets.iter().map(|bucket| bucket.payout_quantity).max().unwrap_or(0);

    Ok(CompiledPayoff { buckets: buckets.to_vec(), legs, max_payout_quantity })
}

pub fn compile_range_payout(
    lower: Strike,
    upper: Strike,
    quantity: u64,
) -> Result<CompiledPayoff, PayoffCompileError> {
    compile_bucket_payoff(&[PayoffBucket::new(Some(lower), Some(upper), quantity)])
}

pub fn compile_breakout(
    k1: Strike,
    k2: Strike,
    k3: Strike,
    k4: Strike,
    tail_quantity: u64,
    shoulder_quantity: u64,
) -> Result<CompiledPayoff, PayoffCompileError> {
    if !(k1.raw < k2.raw && k2.raw < k3.raw && k3.raw < k4.raw) {
        return Err(PayoffCompileError::InvalidBreakoutStrikes);
    }

    compile_bucket_payoff(&[
        PayoffBucket::new(None, Some(k1), tail_quantity),
        PayoffBucket::new(Some(k1), Some(k2), shoulder_quantity),
        PayoffBucket::new(Some(k2), Some(k3), 0),
        PayoffBucket::new(Some(k3), Some(k4), shoulder_quantity),
        PayoffBucket::new(Some(k4), None, tail_quantity),
    ])
}

fn validate_bucket_shape(bucket: &PayoffBucket) -> Result<(), PayoffCompileError> {
    match (bucket.lower, bucket.upper) {
        (None, None) => Err(PayoffCompileError::InvalidOpenBucket),
        (Some(lower), Some(upper)) if lower.raw >= upper.raw => {
            Err(PayoffCompileError::InvalidRange { lower: lower.raw, upper: upper.raw })
        }
        _ => Ok(()),
    }
}
