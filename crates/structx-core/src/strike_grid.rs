use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::price::{DisplayPrice, PriceScale};

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum StrikeGridError {
    #[error("missing min strike")]
    MissingMinStrike,

    #[error("missing tick size")]
    MissingTickSize,

    #[error("tick size must be non-zero")]
    ZeroTickSize,

    #[error("invalid display step")]
    InvalidDisplayStep,

    #[error("spot is outside grid")]
    SpotOutsideGrid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Strike {
    pub raw: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StrikeBucket {
    pub lower: Option<Strike>,
    pub upper: Option<Strike>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrikeGrid {
    pub min_raw: u64,
    pub max_raw: Option<u64>,
    pub tick_size_raw: u64,
    pub scale: PriceScale,
}

impl StrikeGrid {
    pub fn new(
        min_raw: u64,
        max_raw: Option<u64>,
        tick_size_raw: u64,
        scale: PriceScale,
    ) -> Result<Self, StrikeGridError> {
        if tick_size_raw == 0 {
            return Err(StrikeGridError::ZeroTickSize);
        }

        Ok(Self { min_raw, max_raw, tick_size_raw, scale })
    }

    #[must_use]
    pub fn is_valid_strike(self, raw: u64) -> bool {
        if raw < self.min_raw {
            return false;
        }

        if let Some(max_raw) = self.max_raw {
            if raw > max_raw {
                return false;
            }
        }

        (raw - self.min_raw) % self.tick_size_raw == 0
    }
}
