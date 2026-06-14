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

#[cfg(test)]
mod tests {
    use super::*;

    fn strike(raw: u64) -> Strike {
        Strike { raw }
    }

    #[test]
    fn compiles_custom_bucket_payoff_to_predict_legs() {
        let k1 = strike(60);
        let k2 = strike(61);
        let k3 = strike(62);

        let compiled = compile_bucket_payoff(&[
            PayoffBucket::new(None, Some(k1), 1_000),
            PayoffBucket::new(Some(k1), Some(k2), 400),
            PayoffBucket::new(Some(k2), Some(k3), 0),
            PayoffBucket::new(Some(k3), None, 1_200),
        ])
        .expect("payoff compiles");

        assert_eq!(compiled.legs.len(), 3);
        assert_eq!(compiled.max_payout_quantity, 1_200);

        assert_eq!(
            compiled.legs[0],
            PredictLeg::Binary { direction: BinaryDirection::Down, strike: k1, quantity: 1_000 }
        );

        assert_eq!(compiled.legs[1], PredictLeg::Range { lower: k1, upper: k2, quantity: 400 });

        assert_eq!(
            compiled.legs[2],
            PredictLeg::Binary { direction: BinaryDirection::Up, strike: k3, quantity: 1_200 }
        );
    }

    #[test]
    fn compiles_breakout_structure() {
        let k1 = strike(60);
        let k2 = strike(61);
        let k3 = strike(63);
        let k4 = strike(64);

        let compiled = compile_breakout(k1, k2, k3, k4, 1_000, 400).expect("breakout compiles");

        assert_eq!(compiled.legs.len(), 4);
        assert_eq!(compiled.max_payout_quantity, 1_000);

        assert_eq!(compiled.legs[0].kind_name(), "down_binary");
        assert_eq!(compiled.legs[1].kind_name(), "range");
        assert_eq!(compiled.legs[2].kind_name(), "range");
        assert_eq!(compiled.legs[3].kind_name(), "up_binary");
    }

    #[test]
    fn compiles_single_range_payout() {
        let compiled = compile_range_payout(strike(60), strike(61), 500).expect("range compiles");

        assert_eq!(compiled.legs.len(), 1);
        assert_eq!(
            compiled.legs[0],
            PredictLeg::Range { lower: strike(60), upper: strike(61), quantity: 500 }
        );
    }

    #[test]
    fn rejects_invalid_range() {
        let err = compile_range_payout(strike(61), strike(60), 500)
            .expect_err("invalid range should fail");

        assert_eq!(err, PayoffCompileError::InvalidRange { lower: 61, upper: 60 });
    }

    #[test]
    fn rejects_empty_zero_payoff() {
        let err = compile_bucket_payoff(&[
            PayoffBucket::new(None, Some(strike(60)), 0),
            PayoffBucket::new(Some(strike(60)), None, 0),
        ])
        .expect_err("zero payoff should fail");

        assert_eq!(err, PayoffCompileError::NoPayingBuckets);
    }

    #[test]
    fn rejects_invalid_breakout_ordering() {
        let err = compile_breakout(strike(60), strike(61), strike(61), strike(64), 1_000, 400)
            .expect_err("bad breakout should fail");

        assert_eq!(err, PayoffCompileError::InvalidBreakoutStrikes);
    }
}
