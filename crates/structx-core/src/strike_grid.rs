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

    pub fn centered_strikes_by_display_step(
        self,
        spot_raw: u64,
        display_step: DisplayPrice,
        levels_each_side: u32,
    ) -> Result<Vec<Strike>, StrikeGridError> {
        let center = self.snap_nearest(spot_raw).ok_or(StrikeGridError::SpotOutsideGrid)?;
        let step_ticks = self.step_ticks_for_display_step(display_step)?;
        let step_raw = step_ticks
            .checked_mul(self.tick_size_raw)
            .ok_or(StrikeGridError::InvalidDisplayStep)?;

        let mut strikes = Vec::new();

        for level in (1..=levels_each_side).rev() {
            let delta = u64::from(level)
                .checked_mul(step_raw)
                .ok_or(StrikeGridError::InvalidDisplayStep)?;

            if let Some(raw) = center.raw.checked_sub(delta) {
                if self.is_valid_strike(raw) {
                    strikes.push(Strike { raw });
                }
            }
        }

        strikes.push(center);

        for level in 1..=levels_each_side {
            let delta = u64::from(level)
                .checked_mul(step_raw)
                .ok_or(StrikeGridError::InvalidDisplayStep)?;

            let Some(raw) = center.raw.checked_add(delta) else {
                continue;
            };

            if self.is_valid_strike(raw) {
                strikes.push(Strike { raw });
            }
        }

        strikes.dedup_by_key(|strike| strike.raw);
        Ok(strikes)
    }

    pub fn buckets_from_ordered_strikes(self, strikes: &[Strike]) -> Vec<StrikeBucket> {
        if strikes.is_empty() {
            return Vec::new();
        }

        let mut buckets = Vec::with_capacity(strikes.len() + 1);

        buckets.push(StrikeBucket { lower: None, upper: Some(strikes[0]) });

        for pair in strikes.windows(2) {
            buckets.push(StrikeBucket { lower: Some(pair[0]), upper: Some(pair[1]) });
        }

        buckets.push(StrikeBucket { lower: strikes.last().copied(), upper: None });

        buckets
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> StrikeGrid {
        StrikeGrid::new(50_000_000_000_000, Some(80_000_000_000_000), 1_000_000_000, PriceScale::E9)
            .expect("grid builds")
    }

    #[test]
    fn validates_grid_aligned_strikes() {
        let grid = grid();

        assert!(grid.is_valid_strike(50_000_000_000_000));
        assert!(grid.is_valid_strike(50_001_000_000_000));
        assert!(!grid.is_valid_strike(49_999_000_000_000));
        assert!(!grid.is_valid_strike(50_000_500_000_000));
    }

    #[test]
    fn snaps_to_nearest_valid_strike() {
        let grid = grid();

        let strike = grid.snap_nearest(62_773_927_561_148).expect("strike snaps");

        assert_eq!(strike.raw, 62_774_000_000_000);
    }

    #[test]
    fn builds_centered_strikes_by_display_step() {
        let grid = grid();

        let strikes = grid
            .centered_strikes_by_display_step(62_773_927_561_148, DisplayPrice(250.0), 2)
            .expect("strikes build");

        let display =
            strikes.iter().map(|strike| grid.display(*strike).as_f64()).collect::<Vec<_>>();

        assert_eq!(display, vec![62_274.0, 62_524.0, 62_774.0, 63_024.0, 63_274.0]);
    }

    #[test]
    fn builds_tail_and_interval_buckets() {
        let grid = grid();
        let strikes = vec![Strike { raw: 60_000_000_000_000 }, Strike { raw: 61_000_000_000_000 }];

        let buckets = grid.buckets_from_ordered_strikes(&strikes);

        assert_eq!(buckets.len(), 3);
        assert_eq!(buckets[0].lower, None);
        assert_eq!(buckets[0].upper, Some(strikes[0]));
        assert_eq!(buckets[1].lower, Some(strikes[0]));
        assert_eq!(buckets[1].upper, Some(strikes[1]));
        assert_eq!(buckets[2].lower, Some(strikes[1]));
        assert_eq!(buckets[2].upper, None);
    }
}
