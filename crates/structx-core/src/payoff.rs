use serde::{Deserialize, Serialize};

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
