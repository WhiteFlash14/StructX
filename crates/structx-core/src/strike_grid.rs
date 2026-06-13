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

    #[must_use]
    pub fn snap_down(self, raw: u64) -> Option<Strike> {
        if raw < self.min_raw {
            return None;
        }

        let offset = raw - self.min_raw;
        let ticks = offset / self.tick_size_raw;
        let snapped = self.min_raw.checked_add(ticks.checked_mul(self.tick_size_raw)?)?;

        if self.max_raw.is_some_and(|max| snapped > max) {
            return None;
        }

        Some(Strike { raw: snapped })
    }

    #[must_use]
    pub fn snap_up(self, raw: u64) -> Option<Strike> {
        if raw <= self.min_raw {
            return Some(Strike { raw: self.min_raw });
        }

        let offset = raw - self.min_raw;
        let ticks = offset.div_ceil(self.tick_size_raw);
        let snapped = self.min_raw.checked_add(ticks.checked_mul(self.tick_size_raw)?)?;

        if self.max_raw.is_some_and(|max| snapped > max) {
            return None;
        }

        Some(Strike { raw: snapped })
    }

    #[must_use]
    pub fn snap_nearest(self, raw: u64) -> Option<Strike> {
        let down = self.snap_down(raw)?;
        let up = self.snap_up(raw).unwrap_or(down);

        let down_distance = raw.saturating_sub(down.raw);
        let up_distance = up.raw.saturating_sub(raw);

        if down_distance <= up_distance {
            Some(down)
        } else {
            Some(up)
        }
    }

    #[must_use]
    pub fn display(self, strike: Strike) -> DisplayPrice {
        self.scale.display_from_raw(strike.raw)
    }

    pub fn step_ticks_for_display_step(
        self,
        display_step: DisplayPrice,
    ) -> Result<u64, StrikeGridError> {
        let raw_step =
            self.scale.raw_from_display(display_step).ok_or(StrikeGridError::InvalidDisplayStep)?;

        if raw_step == 0 {
            return Err(StrikeGridError::InvalidDisplayStep);
        }

        Ok(raw_step.div_ceil(self.tick_size_raw).max(1))
    }
}
