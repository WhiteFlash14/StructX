use thiserror::Error;

pub const PRICE_SCALE_E9: u128 = 1_000_000_000;
pub const BPS_SCALE: u128 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakoutStyle {
    TailHeavy,
    Balanced,
    HigherHitRate,
}

impl BreakoutStyle {
    pub fn from_api_value(value: &str) -> Result<Self, BreakoutOptimizerError> {
        match value {
            "tail-heavy" | "TAIL_HEAVY" => Ok(Self::TailHeavy),
            "balanced" | "BALANCED" => Ok(Self::Balanced),
            "higher-hit-rate" | "HIGHER_HIT_RATE" => Ok(Self::HigherHitRate),
            other => Err(BreakoutOptimizerError::UnknownStyle(other.to_string())),
        }
    }

    pub fn api_value(self) -> &'static str {
        match self {
            Self::TailHeavy => "tail-heavy",
            Self::Balanced => "balanced",
            Self::HigherHitRate => "higher-hit-rate",
        }
    }

    pub fn moderate_ratio_bps(self) -> u128 {
        match self {
            Self::TailHeavy => 2_500,
            Self::Balanced => 4_500,
            Self::HigherHitRate => 7_500,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BreakoutAskInputs {
    pub down_tail_ask_raw: u64,
    pub downside_range_ask_raw: u64,
    pub upside_range_ask_raw: u64,
    pub up_tail_ask_raw: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptimizedBreakoutQuantities {
    pub down_tail_quantity: u64,
    pub downside_range_quantity: u64,
    pub upside_range_quantity: u64,
    pub up_tail_quantity: u64,
    pub style_ratio_bps: u16,
    pub estimated_premium_raw: u64,
}

#[derive(Debug, Error)]
pub enum BreakoutOptimizerError {
    #[error("unknown breakout style `{0}`")]
    UnknownStyle(String),

    #[error("budget must be greater than zero")]
    ZeroBudget,

    #[error("all asks must be greater than zero")]
    ZeroAsk,

    #[error("budget is too small for non-zero quantities")]
    BudgetTooSmall,

    #[error("optimizer arithmetic overflow")]
    Overflow,
}

pub fn optimize_breakout_quantities(
    budget_raw: u64,
    asks: BreakoutAskInputs,
    style: BreakoutStyle,
) -> Result<OptimizedBreakoutQuantities, BreakoutOptimizerError> {
    if budget_raw == 0 {
        return Err(BreakoutOptimizerError::ZeroBudget);
    }

    if asks.down_tail_ask_raw == 0
        || asks.downside_range_ask_raw == 0
        || asks.upside_range_ask_raw == 0
        || asks.up_tail_ask_raw == 0
    {
        return Err(BreakoutOptimizerError::ZeroAsk);
    }

    let r_bps = style.moderate_ratio_bps();

    let extreme_sum = asks.down_tail_ask_raw as u128 + asks.up_tail_ask_raw as u128;
    let moderate_sum = asks.downside_range_ask_raw as u128 + asks.upside_range_ask_raw as u128;

    let weighted_moderate = checked_div_ceil(
        moderate_sum.checked_mul(r_bps).ok_or(BreakoutOptimizerError::Overflow)?,
        BPS_SCALE,
    )?;

    let denominator =
        extreme_sum.checked_add(weighted_moderate).ok_or(BreakoutOptimizerError::Overflow)?;

    if denominator == 0 {
        return Err(BreakoutOptimizerError::ZeroAsk);
    }

    let q_extreme =
        (budget_raw as u128).checked_mul(PRICE_SCALE_E9).ok_or(BreakoutOptimizerError::Overflow)?
            / denominator;

    if q_extreme == 0 {
        return Err(BreakoutOptimizerError::BudgetTooSmall);
    }

    let q_moderate =
        q_extreme.checked_mul(r_bps).ok_or(BreakoutOptimizerError::Overflow)? / BPS_SCALE;

    if q_moderate == 0 {
        return Err(BreakoutOptimizerError::BudgetTooSmall);
    }

    let mut result = OptimizedBreakoutQuantities {
        down_tail_quantity: u64_checked(q_extreme)?,
        downside_range_quantity: u64_checked(q_moderate)?,
        upside_range_quantity: u64_checked(q_moderate)?,
        up_tail_quantity: u64_checked(q_extreme)?,
        style_ratio_bps: u16_checked(r_bps)?,
        estimated_premium_raw: 0,
    };

    result.estimated_premium_raw = estimate_breakout_premium_raw(asks, result)?;

    if result.estimated_premium_raw > budget_raw {
        result = scale_down_to_budget(asks, result, budget_raw)?;
    }

    Ok(result)
}

pub fn estimate_breakout_premium_raw(
    asks: BreakoutAskInputs,
    quantities: OptimizedBreakoutQuantities,
) -> Result<u64, BreakoutOptimizerError> {
    let total = estimate_leg_cost_ceil(asks.down_tail_ask_raw, quantities.down_tail_quantity)?
        .checked_add(estimate_leg_cost_ceil(
            asks.downside_range_ask_raw,
            quantities.downside_range_quantity,
        )?)
        .ok_or(BreakoutOptimizerError::Overflow)?
        .checked_add(estimate_leg_cost_ceil(
            asks.upside_range_ask_raw,
            quantities.upside_range_quantity,
        )?)
        .ok_or(BreakoutOptimizerError::Overflow)?
        .checked_add(estimate_leg_cost_ceil(asks.up_tail_ask_raw, quantities.up_tail_quantity)?)
        .ok_or(BreakoutOptimizerError::Overflow)?;

    u64_checked(total)
}

fn estimate_leg_cost_ceil(ask_raw: u64, quantity: u64) -> Result<u128, BreakoutOptimizerError> {
    checked_div_ceil(
        (ask_raw as u128).checked_mul(quantity as u128).ok_or(BreakoutOptimizerError::Overflow)?,
        PRICE_SCALE_E9,
    )
}

fn scale_down_to_budget(
    asks: BreakoutAskInputs,
    mut quantities: OptimizedBreakoutQuantities,
    budget_raw: u64,
) -> Result<OptimizedBreakoutQuantities, BreakoutOptimizerError> {
    let premium = quantities.estimated_premium_raw;

    if premium == 0 {
        return Ok(quantities);
    }

    let scale_num = budget_raw as u128;
    let scale_den = premium as u128;

    quantities.down_tail_quantity =
        scale_quantity(quantities.down_tail_quantity, scale_num, scale_den)?;
    quantities.downside_range_quantity =
        scale_quantity(quantities.downside_range_quantity, scale_num, scale_den)?;
    quantities.upside_range_quantity =
        scale_quantity(quantities.upside_range_quantity, scale_num, scale_den)?;
    quantities.up_tail_quantity =
        scale_quantity(quantities.up_tail_quantity, scale_num, scale_den)?;

    if quantities.down_tail_quantity == 0
        || quantities.downside_range_quantity == 0
        || quantities.upside_range_quantity == 0
        || quantities.up_tail_quantity == 0
    {
        return Err(BreakoutOptimizerError::BudgetTooSmall);
    }

    quantities.estimated_premium_raw = estimate_breakout_premium_raw(asks, quantities)?;

    while quantities.estimated_premium_raw > budget_raw {
        quantities.down_tail_quantity = quantities.down_tail_quantity.saturating_sub(1);
        quantities.downside_range_quantity = quantities.downside_range_quantity.saturating_sub(1);
        quantities.upside_range_quantity = quantities.upside_range_quantity.saturating_sub(1);
        quantities.up_tail_quantity = quantities.up_tail_quantity.saturating_sub(1);

        if quantities.down_tail_quantity == 0
            || quantities.downside_range_quantity == 0
            || quantities.upside_range_quantity == 0
            || quantities.up_tail_quantity == 0
        {
            return Err(BreakoutOptimizerError::BudgetTooSmall);
        }

        quantities.estimated_premium_raw = estimate_breakout_premium_raw(asks, quantities)?;
    }

    Ok(quantities)
}

fn scale_quantity(
    quantity: u64,
    scale_num: u128,
    scale_den: u128,
) -> Result<u64, BreakoutOptimizerError> {
    let scaled =
        (quantity as u128).checked_mul(scale_num).ok_or(BreakoutOptimizerError::Overflow)?
            / scale_den;

    u64_checked(scaled)
}

fn checked_div_ceil(numerator: u128, denominator: u128) -> Result<u128, BreakoutOptimizerError> {
    if denominator == 0 {
        return Err(BreakoutOptimizerError::Overflow);
    }

    Ok(numerator.checked_add(denominator - 1).ok_or(BreakoutOptimizerError::Overflow)?
        / denominator)
}

fn u64_checked(value: u128) -> Result<u64, BreakoutOptimizerError> {
    u64::try_from(value).map_err(|_| BreakoutOptimizerError::Overflow)
}

fn u16_checked(value: u128) -> Result<u16, BreakoutOptimizerError> {
    u16::try_from(value).map_err(|_| BreakoutOptimizerError::Overflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balanced_optimizer_respects_budget() {
        let asks = BreakoutAskInputs {
            down_tail_ask_raw: 20_000_000,
            downside_range_ask_raw: 140_000_000,
            upside_range_ask_raw: 90_000_000,
            up_tail_ask_raw: 15_000_000,
        };

        let result =
            optimize_breakout_quantities(250_000_000, asks, BreakoutStyle::Balanced).unwrap();

        assert!(result.estimated_premium_raw <= 250_000_000);
        assert_eq!(result.downside_range_quantity, result.upside_range_quantity);
        assert_eq!(result.down_tail_quantity, result.up_tail_quantity);
        assert!(result.down_tail_quantity > result.downside_range_quantity);
    }

    #[test]
    fn style_changes_moderate_quantity_ratio() {
        let asks = BreakoutAskInputs {
            down_tail_ask_raw: 20_000_000,
            downside_range_ask_raw: 140_000_000,
            upside_range_ask_raw: 90_000_000,
            up_tail_ask_raw: 15_000_000,
        };

        let tail_heavy =
            optimize_breakout_quantities(250_000_000, asks, BreakoutStyle::TailHeavy).unwrap();
        let hit_rate =
            optimize_breakout_quantities(250_000_000, asks, BreakoutStyle::HigherHitRate).unwrap();

        assert!(hit_rate.downside_range_quantity > tail_heavy.downside_range_quantity);
        assert!(tail_heavy.down_tail_quantity > hit_rate.down_tail_quantity);
    }

    #[test]
    fn rejects_zero_budget() {
        let asks = BreakoutAskInputs {
            down_tail_ask_raw: 1,
            downside_range_ask_raw: 1,
            upside_range_ask_raw: 1,
            up_tail_ask_raw: 1,
        };

        assert!(matches!(
            optimize_breakout_quantities(0, asks, BreakoutStyle::Balanced),
            Err(BreakoutOptimizerError::ZeroBudget)
        ));
    }
}
