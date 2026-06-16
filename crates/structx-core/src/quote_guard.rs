use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::quote_preview::QuotePreview;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuoteCostGuard {
    pub max_total_mint_cost_raw: u64,
    pub slippage_bps: u16,
}

impl QuoteCostGuard {
    pub const BPS_DENOMINATOR: u64 = 10_000;

    pub fn max_allowed_after_slippage(self) -> Result<u64, QuoteGuardError> {
        let extra = self
            .max_total_mint_cost_raw
            .checked_mul(u64::from(self.slippage_bps))
            .ok_or(QuoteGuardError::Overflow)?
            / Self::BPS_DENOMINATOR;

        self.max_total_mint_cost_raw.checked_add(extra).ok_or(QuoteGuardError::Overflow)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuardedQuotePreview {
    pub total_mint_cost_raw: u64,
    pub max_total_mint_cost_raw: u64,
    pub max_allowed_after_slippage_raw: u64,
    pub slippage_bps: u16,
    pub accepted: bool,
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum QuoteGuardError {
    #[error("quote cost {actual} exceeds max allowed {max_allowed}")]
    CostTooHigh { actual: u64, max_allowed: u64 },

    #[error("arithmetic overflow while computing quote guard")]
    Overflow,
}

pub fn guard_quote_preview(
    preview: &QuotePreview,
    guard: QuoteCostGuard,
) -> Result<GuardedQuotePreview, QuoteGuardError> {
    let max_allowed = guard.max_allowed_after_slippage()?;
    let actual = preview.total_mint_cost_raw;

    if actual > max_allowed {
        return Err(QuoteGuardError::CostTooHigh { actual, max_allowed });
    }

    Ok(GuardedQuotePreview {
        total_mint_cost_raw: actual,
        max_total_mint_cost_raw: guard.max_total_mint_cost_raw,
        max_allowed_after_slippage_raw: max_allowed,
        slippage_bps: guard.slippage_bps,
        accepted: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quote_preview::{QuoteAssetDisplay, QuotePreview, QuotePreviewLeg};

    fn preview(total_a: u64, total_b: u64) -> QuotePreview {
        QuotePreview::new(
            QuoteAssetDisplay::dusdc(),
            vec![
                QuotePreviewLeg {
                    index: 0,
                    function: "get_trade_amounts".to_string(),
                    leg: "down_binary".to_string(),
                    strike_or_lower: "63343.00".to_string(),
                    upper: None,
                    quantity: 1000,
                    mint_cost_raw: total_a,
                    redeem_payout_raw: 0,
                },
                QuotePreviewLeg {
                    index: 1,
                    function: "get_range_trade_amounts".to_string(),
                    leg: "range".to_string(),
                    strike_or_lower: "63343.00".to_string(),
                    upper: Some("63593.00".to_string()),
                    quantity: 400,
                    mint_cost_raw: total_b,
                    redeem_payout_raw: 0,
                },
            ],
        )
    }

    #[test]
    fn accepts_quote_within_guard() {
        let preview = preview(10, 9);

        let guarded = guard_quote_preview(
            &preview,
            QuoteCostGuard { max_total_mint_cost_raw: 19, slippage_bps: 100 },
        )
        .expect("quote accepted");

        assert!(guarded.accepted);
        assert_eq!(guarded.total_mint_cost_raw, 19);
        assert_eq!(guarded.max_allowed_after_slippage_raw, 19);
    }

    #[test]
    fn accepts_quote_with_slippage_buffer() {
        let preview = preview(10, 10);

        let guarded = guard_quote_preview(
            &preview,
            QuoteCostGuard { max_total_mint_cost_raw: 19, slippage_bps: 1_000 },
        )
        .expect("quote accepted");

        assert_eq!(guarded.max_allowed_after_slippage_raw, 20);
    }

    #[test]
    fn rejects_quote_above_guard() {
        let preview = preview(10, 10);

        let err = guard_quote_preview(
            &preview,
            QuoteCostGuard { max_total_mint_cost_raw: 19, slippage_bps: 0 },
        )
        .expect_err("quote rejected");

        assert_eq!(err, QuoteGuardError::CostTooHigh { actual: 20, max_allowed: 19 });
    }
}
