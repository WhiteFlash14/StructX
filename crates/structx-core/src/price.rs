use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriceScale {
    pub factor: u64,
}

impl PriceScale {
    pub const ONE: Self = Self { factor: 1 };
    pub const E9: Self = Self { factor: 1_000_000_000 };

    #[must_use]
    pub fn display_from_raw(self, raw: u64) -> DisplayPrice {
        DisplayPrice(raw as f64 / self.factor as f64)
    }

    #[must_use]
    pub fn raw_from_display(self, display: DisplayPrice) -> Option<u64> {
        if !display.0.is_finite() || display.0 < 0.0 {
            return None;
        }

        let raw = display.0 * self.factor as f64;
        if raw > u64::MAX as f64 {
            return None;
        }

        Some(raw.round() as u64)
    }

    #[must_use]
    pub fn raw_from_api_number(self, value: f64) -> Option<u64> {
        if !value.is_finite() || value < 0.0 {
            return None;
        }

        // Current BTC Predict Testnet responses expose spot like values as raw
        // 1e9-scaled integers. If a future server returns human display prices,
        // this still handles them.
        if value >= self.factor as f64 {
            Some(value.round() as u64)
        } else {
            self.raw_from_display(DisplayPrice(value))
        }
    }
}

impl Default for PriceScale {
    fn default() -> Self {
        Self::E9
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct DisplayPrice(pub f64);

impl DisplayPrice {
    #[must_use]
    pub fn as_f64(self) -> f64 {
        self.0
    }
}

impl std::fmt::Display for DisplayPrice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0.abs() >= 1_000.0 {
            write!(f, "{:.2}", self.0)
        } else {
            write!(f, "{:.4}", self.0)
        }
    }
}
