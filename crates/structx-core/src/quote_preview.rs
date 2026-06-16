use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuoteAssetDisplay {
    pub symbol: String,
    pub decimals: u8,
}

impl QuoteAssetDisplay {
    #[must_use]
    pub fn dusdc() -> Self {
        Self { symbol: "dUSDC".to_string(), decimals: 6 }
    }

    #[must_use]
    pub fn format_amount(&self, raw: u64) -> String {
        format_quote_amount(raw, self.decimals, &self.symbol)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuotePreviewLeg {
    pub index: usize,
    pub function: String,
    pub leg: String,
    pub strike_or_lower: String,
    pub upper: Option<String>,
    pub quantity: u64,
    pub mint_cost_raw: u64,
    pub redeem_payout_raw: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuotePreview {
    pub asset: QuoteAssetDisplay,
    pub legs: Vec<QuotePreviewLeg>,
    pub total_mint_cost_raw: u64,
    pub total_redeem_payout_raw: u64,
}

impl QuotePreview {
    #[must_use]
    pub fn new(asset: QuoteAssetDisplay, legs: Vec<QuotePreviewLeg>) -> Self {
        let total_mint_cost_raw = legs.iter().map(|leg| leg.mint_cost_raw).sum();
        let total_redeem_payout_raw = legs.iter().map(|leg| leg.redeem_payout_raw).sum();

        Self { asset, legs, total_mint_cost_raw, total_redeem_payout_raw }
    }

    #[must_use]
    pub fn total_mint_cost_display(&self) -> String {
        self.asset.format_amount(self.total_mint_cost_raw)
    }

    #[must_use]
    pub fn total_redeem_payout_display(&self) -> String {
        self.asset.format_amount(self.total_redeem_payout_raw)
    }
}

#[must_use]
pub fn format_quote_amount(raw: u64, decimals: u8, symbol: &str) -> String {
    if decimals == 0 {
        return format!("{raw} {symbol}");
    }

    let factor = 10u128.pow(u32::from(decimals));
    let raw = u128::from(raw);
    let whole = raw / factor;
    let fractional = raw % factor;

    let mut fractional_str = format!("{fractional:0width$}", width = decimals as usize);

    while fractional_str.ends_with('0') && fractional_str.len() > 2 {
        fractional_str.pop();
    }

    format!("{whole}.{fractional_str} {symbol}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_dusdc_units() {
        assert_eq!(format_quote_amount(5, 6, "dUSDC"), "0.000005 dUSDC");
        assert_eq!(format_quote_amount(12_345_678, 6, "dUSDC"), "12.345678 dUSDC");
        assert_eq!(format_quote_amount(1_000_000, 6, "dUSDC"), "1.00 dUSDC");
    }

    #[test]
    fn totals_quote_preview() {
        let preview = QuotePreview::new(
            QuoteAssetDisplay::dusdc(),
            vec![
                QuotePreviewLeg {
                    index: 0,
                    function: "get_trade_amounts".to_string(),
                    leg: "down_binary".to_string(),
                    strike_or_lower: "63127.00".to_string(),
                    upper: None,
                    quantity: 1000,
                    mint_cost_raw: 5,
                    redeem_payout_raw: 0,
                },
                QuotePreviewLeg {
                    index: 1,
                    function: "get_range_trade_amounts".to_string(),
                    leg: "range".to_string(),
                    strike_or_lower: "63127.00".to_string(),
                    upper: Some("63377.00".to_string()),
                    quantity: 400,
                    mint_cost_raw: 12,
                    redeem_payout_raw: 8,
                },
            ],
        );

        assert_eq!(preview.total_mint_cost_raw, 17);
        assert_eq!(preview.total_redeem_payout_raw, 8);
        assert_eq!(preview.total_mint_cost_display(), "0.000017 dUSDC");
    }
}
