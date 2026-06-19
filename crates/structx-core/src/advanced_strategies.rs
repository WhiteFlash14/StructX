use thiserror::Error;

pub const PRICE_SCALE_E9: u128 = 1_000_000_000;
pub const DUSDC_SCALE: u128 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvancedStrategyKind {
    PortfolioCrashShield,
    ConvexTailLadder,
    ExpiryMoveNote,
    MoonshotUpside,
    DownsideConvexity,
    UpsideStepLadder,
    DownsideStepLadder,
    CenterBandCondor,
    RangeConviction,
    SmartBudgetSelector,
}

impl AdvancedStrategyKind {
    pub fn from_api_value(value: &str) -> Result<Self, AdvancedStrategyError> {
        match value {
            "PORTFOLIO_CRASH_SHIELD" | "portfolio_crash_shield" => Ok(Self::PortfolioCrashShield),
            "CONVEX_TAIL_LADDER" | "convex_tail_ladder" => Ok(Self::ConvexTailLadder),
            "EXPIRY_MOVE_NOTE" | "expiry_move_note" => Ok(Self::ExpiryMoveNote),
            "MOONSHOT_UPSIDE" | "moonshot_upside" => Ok(Self::MoonshotUpside),
            "DOWNSIDE_CONVEXITY" | "downside_convexity" => Ok(Self::DownsideConvexity),
            "UPSIDE_STEP_LADDER" | "upside_step_ladder" => Ok(Self::UpsideStepLadder),
            "DOWNSIDE_STEP_LADDER" | "downside_step_ladder" => Ok(Self::DownsideStepLadder),
            "CENTER_BAND_CONDOR" | "center_band_condor" => Ok(Self::CenterBandCondor),
            "RANGE_CONVICTION" | "range_conviction" => Ok(Self::RangeConviction),
            "SMART_BUDGET_SELECTOR" | "smart_budget_selector" => Ok(Self::SmartBudgetSelector),
            other => Err(AdvancedStrategyError::UnknownStrategy(other.to_string())),
        }
    }

    pub fn api_value(self) -> &'static str {
        match self {
            Self::PortfolioCrashShield => "PORTFOLIO_CRASH_SHIELD",
            Self::ConvexTailLadder => "CONVEX_TAIL_LADDER",
            Self::ExpiryMoveNote => "EXPIRY_MOVE_NOTE",
            Self::MoonshotUpside => "MOONSHOT_UPSIDE",
            Self::DownsideConvexity => "DOWNSIDE_CONVEXITY",
            Self::UpsideStepLadder => "UPSIDE_STEP_LADDER",
            Self::DownsideStepLadder => "DOWNSIDE_STEP_LADDER",
            Self::CenterBandCondor => "CENTER_BAND_CONDOR",
            Self::RangeConviction => "RANGE_CONVICTION",
            Self::SmartBudgetSelector => "SMART_BUDGET_SELECTOR",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvancedLegKind {
    Down,
    Range,
    Up,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvancedLegInput {
    pub kind: AdvancedLegKind,
    pub role: &'static str,
    pub strike_raw: Option<u64>,
    pub lower_raw: Option<u64>,
    pub upper_raw: Option<u64>,
    pub midpoint_raw: u64,
    pub ask_price_raw: u64,
    pub base_weight_e6: u64,
    pub max_quantity: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvancedCompiledLeg {
    pub kind: AdvancedLegKind,
    pub role: &'static str,
    pub strike_raw: Option<u64>,
    pub lower_raw: Option<u64>,
    pub upper_raw: Option<u64>,
    pub midpoint_raw: u64,
    pub ask_price_raw: u64,
    pub weight_e6: u64,
    pub quantity: u64,
    pub premium_raw: u64,
    pub max_quantity: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvancedCompileResult {
    pub strategy: AdvancedStrategyKind,
    pub requested_budget_raw: u64,
    pub used_budget_raw: u64,
    pub unused_budget_raw: u64,
    pub legs: Vec<AdvancedCompiledLeg>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Error)]
pub enum AdvancedStrategyError {
    #[error("unknown strategy `{0}`")]
    UnknownStrategy(String),

    #[error("budget must be greater than zero")]
    ZeroBudget,

    #[error("ask price must be greater than zero")]
    ZeroAsk,

    #[error("no positive-weight legs")]
    NoPositiveWeights,

    #[error("budget too small for nonzero allocation")]
    BudgetTooSmall,

    #[error("arithmetic overflow")]
    Overflow,

    #[error("invalid input: {0}")]
    InvalidInput(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmartBudgetStyle {
    TailHeavy,
    Balanced,
    HigherHitRate,
}

impl SmartBudgetStyle {
    pub fn from_api_value(value: &str) -> Self {
        match value {
            "tail-heavy" | "TAIL_HEAVY" => Self::TailHeavy,
            "higher-hit-rate" | "HIGHER_HIT_RATE" => Self::HigherHitRate,
            _ => Self::Balanced,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SmartCandidateMetrics {
    pub premium_raw: u64,
    pub max_payout_raw: u64,
    pub expected_payout_raw: u64,
    pub hit_probability_bps: u16,
    pub worst_case_improvement_raw: u64,
    pub complexity_penalty_bps: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SmartCandidateScore {
    pub score_e6: i128,
    pub max_payout_score_e6: i128,
    pub expected_payout_score_e6: i128,
    pub hit_probability_score_e6: i128,
    pub worst_case_score_e6: i128,
    pub complexity_penalty_e6: i128,
}

pub fn score_smart_candidate(
    metrics: SmartCandidateMetrics,
    style: SmartBudgetStyle,
) -> Result<SmartCandidateScore, AdvancedStrategyError> {
    if metrics.premium_raw == 0 {
        return Err(AdvancedStrategyError::InvalidInput(
            "candidate premium must be greater than zero".to_string(),
        ));
    }

    let params = smart_score_params(style);
    let premium = metrics.premium_raw as i128;

    let max_payout_score = ratio_score_e6(metrics.max_payout_raw, premium, params.alpha_bps)?;
    let expected_score = ratio_score_e6(metrics.expected_payout_raw, premium, params.beta_bps)?;
    let hit_score = (metrics.hit_probability_bps as i128)
        .checked_mul(params.eta_bps as i128)
        .ok_or(AdvancedStrategyError::Overflow)?
        .checked_div(100)
        .ok_or(AdvancedStrategyError::Overflow)?;

    let worst_case_score =
        ratio_score_e6(metrics.worst_case_improvement_raw, premium, params.rho_bps)?;

    let complexity_penalty = (metrics.complexity_penalty_bps as i128)
        .checked_mul(params.delta_bps as i128)
        .ok_or(AdvancedStrategyError::Overflow)?
        .checked_div(100)
        .ok_or(AdvancedStrategyError::Overflow)?;

    let total = max_payout_score
        .checked_add(expected_score)
        .ok_or(AdvancedStrategyError::Overflow)?
        .checked_add(hit_score)
        .ok_or(AdvancedStrategyError::Overflow)?
        .checked_add(worst_case_score)
        .ok_or(AdvancedStrategyError::Overflow)?
        .checked_sub(complexity_penalty)
        .ok_or(AdvancedStrategyError::Overflow)?;

    Ok(SmartCandidateScore {
        score_e6: total,
        max_payout_score_e6: max_payout_score,
        expected_payout_score_e6: expected_score,
        hit_probability_score_e6: hit_score,
        worst_case_score_e6: worst_case_score,
        complexity_penalty_e6: complexity_penalty,
    })
}

#[derive(Debug, Clone, Copy)]
struct SmartScoreParams {
    alpha_bps: u16,
    beta_bps: u16,
    eta_bps: u16,
    rho_bps: u16,
    delta_bps: u16,
}

fn smart_score_params(style: SmartBudgetStyle) -> SmartScoreParams {
    match style {
        SmartBudgetStyle::TailHeavy => SmartScoreParams {
            alpha_bps: 4_500,
            beta_bps: 1_500,
            eta_bps: 500,
            rho_bps: 3_500,
            delta_bps: 500,
        },
        SmartBudgetStyle::Balanced => SmartScoreParams {
            alpha_bps: 2_500,
            beta_bps: 3_000,
            eta_bps: 2_000,
            rho_bps: 2_500,
            delta_bps: 500,
        },
        SmartBudgetStyle::HigherHitRate => SmartScoreParams {
            alpha_bps: 1_000,
            beta_bps: 2_500,
            eta_bps: 5_000,
            rho_bps: 1_500,
            delta_bps: 500,
        },
    }
}

fn ratio_score_e6(
    numerator_raw: u64,
    premium_raw: i128,
    weight_bps: u16,
) -> Result<i128, AdvancedStrategyError> {
    let numerator = numerator_raw as i128;

    numerator
        .checked_mul(1_000_000)
        .ok_or(AdvancedStrategyError::Overflow)?
        .checked_div(premium_raw)
        .ok_or(AdvancedStrategyError::Overflow)?
        .checked_mul(weight_bps as i128)
        .ok_or(AdvancedStrategyError::Overflow)?
        .checked_div(10_000)
        .ok_or(AdvancedStrategyError::Overflow)
}

pub fn allocate_weighted_budget(
    strategy: AdvancedStrategyKind,
    budget_raw: u64,
    legs: Vec<AdvancedLegInput>,
) -> Result<AdvancedCompileResult, AdvancedStrategyError> {
    if budget_raw == 0 {
        return Err(AdvancedStrategyError::ZeroBudget);
    }

    let mut active: Vec<AdvancedLegInput> =
        legs.into_iter().filter(|leg| leg.base_weight_e6 > 0).collect();

    if active.is_empty() {
        return Err(AdvancedStrategyError::NoPositiveWeights);
    }

    if active.iter().any(|leg| leg.ask_price_raw == 0) {
        return Err(AdvancedStrategyError::ZeroAsk);
    }

    let mut remaining_budget = budget_raw as u128;
    let mut compiled = Vec::<AdvancedCompiledLeg>::new();
    let mut warnings = Vec::<String>::new();

    while !active.is_empty() && remaining_budget > 0 {
        let denom = active.iter().try_fold(0u128, |acc, leg| {
            let term = (leg.ask_price_raw as u128)
                .checked_mul(leg.base_weight_e6 as u128)
                .ok_or(AdvancedStrategyError::Overflow)?;
            acc.checked_add(term).ok_or(AdvancedStrategyError::Overflow)
        })?;

        if denom == 0 {
            break;
        }

        let lambda_num =
            remaining_budget.checked_mul(PRICE_SCALE_E9).ok_or(AdvancedStrategyError::Overflow)?;

        let mut next_active = Vec::<AdvancedLegInput>::new();
        let mut any_finalized = false;

        for leg in active {
            let raw_qty = lambda_num
                .checked_mul(leg.base_weight_e6 as u128)
                .ok_or(AdvancedStrategyError::Overflow)?
                / denom;

            if raw_qty == 0 {
                continue;
            }

            let capped_qty = match leg.max_quantity {
                Some(max_qty) if raw_qty > max_qty as u128 => max_qty as u128,
                _ => raw_qty,
            };

            if capped_qty == 0 {
                continue;
            }

            let premium = ceil_div(
                (leg.ask_price_raw as u128)
                    .checked_mul(capped_qty)
                    .ok_or(AdvancedStrategyError::Overflow)?,
                PRICE_SCALE_E9,
            )?;

            if premium == 0 {
                continue;
            }

            if premium > remaining_budget {
                let affordable_qty = remaining_budget
                    .checked_mul(PRICE_SCALE_E9)
                    .ok_or(AdvancedStrategyError::Overflow)?
                    / leg.ask_price_raw as u128;

                if affordable_qty == 0 {
                    continue;
                }

                let affordable_premium = ceil_div(
                    (leg.ask_price_raw as u128)
                        .checked_mul(affordable_qty)
                        .ok_or(AdvancedStrategyError::Overflow)?,
                    PRICE_SCALE_E9,
                )?;

                compiled.push(AdvancedCompiledLeg {
                    kind: leg.kind,
                    role: leg.role,
                    strike_raw: leg.strike_raw,
                    lower_raw: leg.lower_raw,
                    upper_raw: leg.upper_raw,
                    midpoint_raw: leg.midpoint_raw,
                    ask_price_raw: leg.ask_price_raw,
                    weight_e6: leg.base_weight_e6,
                    quantity: u64_checked(affordable_qty)?,
                    premium_raw: u64_checked(affordable_premium)?,
                    max_quantity: leg.max_quantity,
                });

                remaining_budget = remaining_budget.saturating_sub(affordable_premium);
                any_finalized = true;
                continue;
            }

            compiled.push(AdvancedCompiledLeg {
                kind: leg.kind,
                role: leg.role,
                strike_raw: leg.strike_raw,
                lower_raw: leg.lower_raw,
                upper_raw: leg.upper_raw,
                midpoint_raw: leg.midpoint_raw,
                ask_price_raw: leg.ask_price_raw,
                weight_e6: leg.base_weight_e6,
                quantity: u64_checked(capped_qty)?,
                premium_raw: u64_checked(premium)?,
                max_quantity: leg.max_quantity,
            });

            remaining_budget = remaining_budget.saturating_sub(premium);
            any_finalized = true;

            if leg.max_quantity.is_none() {
                // Uncapped legs are done in this MVP allocator. We do not re-enter
                // them because one pass gives the exact weight allocation.
            } else if raw_qty <= capped_qty {
                // Capped leg did not bind, so it is done.
            } else {
                // Capped leg bound, do not re-add it.
            }
        }

        if !any_finalized {
            break;
        }

        active = std::mem::take(&mut next_active);
    }

    if compiled.is_empty() {
        return Err(AdvancedStrategyError::BudgetTooSmall);
    }

    let used_budget = compiled.iter().try_fold(0u64, |acc, leg| {
        acc.checked_add(leg.premium_raw).ok_or(AdvancedStrategyError::Overflow)
    })?;

    if used_budget < budget_raw {
        warnings.push(format!(
            "Strategy used {} raw dUSDC and left {} raw unused because caps or available buckets constrained allocation.",
            used_budget,
            budget_raw - used_budget
        ));
    }

    Ok(AdvancedCompileResult {
        strategy,
        requested_budget_raw: budget_raw,
        used_budget_raw: used_budget,
        unused_budget_raw: budget_raw.saturating_sub(used_budget),
        legs: compiled,
        warnings,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortfolioCrashShieldInput {
    pub spot_raw: u64,
    pub exposure_raw: u64,
    pub budget_raw: u64,
    pub over_hedge_cap_bps: u16,
    pub gamma_bps: u16,
    pub down_tail_strike_raw: u64,
    pub lower_range_upper_raw: u64,
    pub mild_range_upper_raw: Option<u64>,
    pub down_tail_ask_raw: u64,
    pub lower_range_ask_raw: u64,
    pub mild_range_ask_raw: Option<u64>,
}

pub fn compile_portfolio_crash_shield(
    input: PortfolioCrashShieldInput,
) -> Result<AdvancedCompileResult, AdvancedStrategyError> {
    if input.spot_raw == 0 {
        return Err(AdvancedStrategyError::InvalidInput(
            "spot_raw must be greater than zero".to_string(),
        ));
    }

    if input.exposure_raw == 0 {
        return Err(AdvancedStrategyError::InvalidInput(
            "portfolio exposure must be greater than zero".to_string(),
        ));
    }

    let mut legs = Vec::new();

    let severe_midpoint = input
        .down_tail_strike_raw
        .saturating_mul(9)
        .checked_div(10)
        .unwrap_or(input.down_tail_strike_raw);

    let severe = crash_bucket_leg(
        AdvancedLegKind::Down,
        "severe_downside",
        Some(input.down_tail_strike_raw),
        None,
        None,
        severe_midpoint,
        input.down_tail_ask_raw,
        input.spot_raw,
        input.exposure_raw,
        input.over_hedge_cap_bps,
        input.gamma_bps,
    )?;

    legs.push(severe);

    let lower_midpoint = midpoint(input.down_tail_strike_raw, input.lower_range_upper_raw)?;

    let lower = crash_bucket_leg(
        AdvancedLegKind::Range,
        "moderate_downside",
        None,
        Some(input.down_tail_strike_raw),
        Some(input.lower_range_upper_raw),
        lower_midpoint,
        input.lower_range_ask_raw,
        input.spot_raw,
        input.exposure_raw,
        input.over_hedge_cap_bps,
        input.gamma_bps,
    )?;

    legs.push(lower);

    if let (Some(mild_upper), Some(mild_ask)) =
        (input.mild_range_upper_raw, input.mild_range_ask_raw)
    {
        let mild_midpoint = midpoint(input.lower_range_upper_raw, mild_upper)?;

        let mild = crash_bucket_leg(
            AdvancedLegKind::Range,
            "mild_downside",
            None,
            Some(input.lower_range_upper_raw),
            Some(mild_upper),
            mild_midpoint,
            mild_ask,
            input.spot_raw,
            input.exposure_raw,
            input.over_hedge_cap_bps,
            input.gamma_bps,
        )?;

        legs.push(mild);
    }

    let mut result = allocate_weighted_budget(
        AdvancedStrategyKind::PortfolioCrashShield,
        input.budget_raw,
        legs,
    )?;

    result.warnings.push(
        "Portfolio-Aware Crash Shield only estimates BTC-equivalent exposure; it is not a guarantee against portfolio losses."
            .to_string(),
    );

    Ok(result)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpiryMoveNoteInput {
    pub spot_raw: u64,
    pub budget_raw: u64,
    pub k1_raw: u64,
    pub k2_raw: u64,
    pub k3_raw: u64,
    pub k4_raw: u64,
    pub down_tail_ask_raw: u64,
    pub lower_range_ask_raw: u64,
    pub upper_range_ask_raw: u64,
    pub up_tail_ask_raw: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoonshotUpsideInput {
    pub spot_raw: u64,
    pub budget_raw: u64,
    pub k3_raw: u64,
    pub k4_raw: u64,
    pub upper_range_ask_raw: u64,
    pub up_tail_ask_raw: u64,
    pub range_weight_bps: u16,
    pub tail_gamma_bps: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DownsideConvexityInput {
    pub spot_raw: u64,
    pub budget_raw: u64,
    pub k1_raw: u64,
    pub k2_raw: u64,
    pub down_tail_ask_raw: u64,
    pub lower_range_ask_raw: u64,
    pub range_weight_bps: u16,
    pub tail_gamma_bps: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpsideStepLadderInput {
    pub spot_raw: u64,
    pub budget_raw: u64,
    pub center_raw: u64,
    pub k3_raw: u64,
    pub k4_raw: u64,
    pub near_up_range_ask_raw: u64,
    pub upper_range_ask_raw: u64,
    pub up_tail_ask_raw: u64,
    pub near_range_weight_bps: u16,
    pub upper_range_weight_bps: u16,
    pub tail_gamma_bps: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DownsideStepLadderInput {
    pub spot_raw: u64,
    pub budget_raw: u64,
    pub k1_raw: u64,
    pub k2_raw: u64,
    pub center_raw: u64,
    pub down_tail_ask_raw: u64,
    pub lower_range_ask_raw: u64,
    pub near_down_range_ask_raw: u64,
    pub near_range_weight_bps: u16,
    pub lower_range_weight_bps: u16,
    pub tail_gamma_bps: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CenterBandCondorInput {
    pub budget_raw: u64,
    pub k1_raw: u64,
    pub k2_raw: u64,
    pub center_raw: u64,
    pub k3_raw: u64,
    pub k4_raw: u64,
    pub lower_wing_ask_raw: u64,
    pub lower_center_ask_raw: u64,
    pub upper_center_ask_raw: u64,
    pub upper_wing_ask_raw: u64,
    pub center_weight_bps: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RangeConvictionInput {
    pub budget_raw: u64,
    pub lower_raw: u64,
    pub upper_raw: u64,
    pub range_ask_raw: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConvexTailLadderInput {
    pub spot_raw: u64,
    pub budget_raw: u64,
    pub dead_zone_bps: u16,
    pub gamma_bps: u16,
    pub k1_raw: u64,
    pub k2_raw: u64,
    pub k3_raw: u64,
    pub k4_raw: u64,
    pub down_tail_ask_raw: u64,
    pub lower_range_ask_raw: u64,
    pub upper_range_ask_raw: u64,
    pub up_tail_ask_raw: u64,
}

pub fn compile_convex_tail_ladder(
    input: ConvexTailLadderInput,
) -> Result<AdvancedCompileResult, AdvancedStrategyError> {
    if input.spot_raw == 0 {
        return Err(AdvancedStrategyError::InvalidInput(
            "spot_raw must be greater than zero".to_string(),
        ));
    }

    let severe_down_mid = input.k1_raw.saturating_mul(99).checked_div(100).unwrap_or(input.k1_raw);

    let down_range_mid = midpoint(input.k1_raw, input.k2_raw)?;
    let up_range_mid = midpoint(input.k3_raw, input.k4_raw)?;

    let severe_up_mid = input.k4_raw.saturating_mul(101).checked_div(100).unwrap_or(input.k4_raw);

    let legs = vec![
        tail_ladder_leg(
            AdvancedLegKind::Down,
            "extreme_downside",
            Some(input.k1_raw),
            None,
            None,
            severe_down_mid,
            input.down_tail_ask_raw,
            input.spot_raw,
            input.dead_zone_bps,
            input.gamma_bps,
        )?,
        tail_ladder_leg(
            AdvancedLegKind::Range,
            "moderate_downside",
            None,
            Some(input.k1_raw),
            Some(input.k2_raw),
            down_range_mid,
            input.lower_range_ask_raw,
            input.spot_raw,
            input.dead_zone_bps,
            input.gamma_bps,
        )?,
        tail_ladder_leg(
            AdvancedLegKind::Range,
            "moderate_upside",
            None,
            Some(input.k3_raw),
            Some(input.k4_raw),
            up_range_mid,
            input.upper_range_ask_raw,
            input.spot_raw,
            input.dead_zone_bps,
            input.gamma_bps,
        )?,
        tail_ladder_leg(
            AdvancedLegKind::Up,
            "extreme_upside",
            Some(input.k4_raw),
            None,
            None,
            severe_up_mid,
            input.up_tail_ask_raw,
            input.spot_raw,
            input.dead_zone_bps,
            input.gamma_bps,
        )?,
    ];

    let mut result =
        allocate_weighted_budget(AdvancedStrategyKind::ConvexTailLadder, input.budget_raw, legs)?;

    result.warnings.push(
        "Convex Tail Ladder is a terminal-expiry payoff. It does not pay for realized intraperiod volatility or touches before expiry."
            .to_string(),
    );

    Ok(result)
}

pub fn compile_expiry_move_note(
    input: ExpiryMoveNoteInput,
) -> Result<AdvancedCompileResult, AdvancedStrategyError> {
    if input.spot_raw == 0 {
        return Err(AdvancedStrategyError::InvalidInput(
            "spot_raw must be greater than zero".to_string(),
        ));
    }

    let severe_down_mid = input.k1_raw.saturating_mul(99).checked_div(100).unwrap_or(input.k1_raw);

    let down_range_mid = midpoint(input.k1_raw, input.k2_raw)?;
    let up_range_mid = midpoint(input.k3_raw, input.k4_raw)?;

    let severe_up_mid = input.k4_raw.saturating_mul(101).checked_div(100).unwrap_or(input.k4_raw);

    let legs = vec![
        expiry_move_leg(
            AdvancedLegKind::Down,
            "large_downside_move",
            Some(input.k1_raw),
            None,
            None,
            severe_down_mid,
            input.down_tail_ask_raw,
            input.spot_raw,
        )?,
        expiry_move_leg(
            AdvancedLegKind::Range,
            "moderate_downside_move",
            None,
            Some(input.k1_raw),
            Some(input.k2_raw),
            down_range_mid,
            input.lower_range_ask_raw,
            input.spot_raw,
        )?,
        expiry_move_leg(
            AdvancedLegKind::Range,
            "moderate_upside_move",
            None,
            Some(input.k3_raw),
            Some(input.k4_raw),
            up_range_mid,
            input.upper_range_ask_raw,
            input.spot_raw,
        )?,
        expiry_move_leg(
            AdvancedLegKind::Up,
            "large_upside_move",
            Some(input.k4_raw),
            None,
            None,
            severe_up_mid,
            input.up_tail_ask_raw,
            input.spot_raw,
        )?,
    ];

    let mut result =
        allocate_weighted_budget(AdvancedStrategyKind::ExpiryMoveNote, input.budget_raw, legs)?;

    result.warnings.push(
        "Expiry Move Note is a terminal-settlement product. It does not pay for realized volatility, intraperiod touches, or path-dependent moves."
            .to_string(),
    );

    Ok(result)
}

pub fn compile_range_conviction(
    input: RangeConvictionInput,
) -> Result<AdvancedCompileResult, AdvancedStrategyError> {
    if input.lower_raw >= input.upper_raw {
        return Err(AdvancedStrategyError::InvalidInput(
            "range conviction requires lower < upper".to_string(),
        ));
    }

    let midpoint_raw = midpoint(input.lower_raw, input.upper_raw)?;

    let leg = AdvancedLegInput {
        kind: AdvancedLegKind::Range,
        role: "central_range_conviction",
        strike_raw: None,
        lower_raw: Some(input.lower_raw),
        upper_raw: Some(input.upper_raw),
        midpoint_raw,
        ask_price_raw: input.range_ask_raw,
        base_weight_e6: 1_000_000,
        max_quantity: None,
    };

    let mut result = allocate_weighted_budget(
        AdvancedStrategyKind::RangeConviction,
        input.budget_raw,
        vec![leg],
    )?;

    result.warnings.push(
        "Range Conviction pays only if BTC settles inside the selected terminal range. It does not pay for staying inside the range during the period."
            .to_string(),
    );

    result.warnings.push(
        "This is a concentrated one-range payoff. It can expire worthless if BTC settles outside the corridor."
            .to_string(),
    );

    Ok(result)
}

pub fn compile_moonshot_upside(
    input: MoonshotUpsideInput,
) -> Result<AdvancedCompileResult, AdvancedStrategyError> {
    if input.spot_raw == 0 {
        return Err(AdvancedStrategyError::InvalidInput(
            "spot_raw must be greater than zero".to_string(),
        ));
    }

    if input.k3_raw >= input.k4_raw {
        return Err(AdvancedStrategyError::InvalidInput("moonshot requires k3 < k4".to_string()));
    }

    let range_weight_bps = input.range_weight_bps.min(10_000);
    let tail_weight_bps = 10_000u16.saturating_sub(range_weight_bps);
    let upper_range_mid = midpoint(input.k3_raw, input.k4_raw)?;

    let up_tail_mid = input.k4_raw.saturating_mul(101).checked_div(100).unwrap_or(input.k4_raw);

    let mut upper_range = tail_ladder_leg(
        AdvancedLegKind::Range,
        "upside_breakout_zone",
        None,
        Some(input.k3_raw),
        Some(input.k4_raw),
        upper_range_mid,
        input.upper_range_ask_raw,
        input.spot_raw,
        0,
        10_000,
    )?;

    upper_range.base_weight_e6 = scale_weight_bps(upper_range.base_weight_e6, range_weight_bps)?;

    let mut up_tail = tail_ladder_leg(
        AdvancedLegKind::Up,
        "moonshot_tail",
        Some(input.k4_raw),
        None,
        None,
        up_tail_mid,
        input.up_tail_ask_raw,
        input.spot_raw,
        0,
        input.tail_gamma_bps,
    )?;

    up_tail.base_weight_e6 = scale_weight_bps(up_tail.base_weight_e6, tail_weight_bps)?;

    let mut result = allocate_weighted_budget(
        AdvancedStrategyKind::MoonshotUpside,
        input.budget_raw,
        vec![upper_range, up_tail],
    )?;

    result.warnings.push(
        "Moonshot Upside is upside-only. It can expire worthless if BTC settles below the upper range."
            .to_string(),
    );

    result
        .warnings
        .push("Moonshot Upside is terminal-expiry exposure, not a touch option.".to_string());

    Ok(result)
}

pub fn compile_downside_convexity(
    input: DownsideConvexityInput,
) -> Result<AdvancedCompileResult, AdvancedStrategyError> {
    if input.spot_raw == 0 {
        return Err(AdvancedStrategyError::InvalidInput(
            "spot_raw must be greater than zero".to_string(),
        ));
    }

    if input.k1_raw >= input.k2_raw {
        return Err(AdvancedStrategyError::InvalidInput(
            "downside convexity requires k1 < k2".to_string(),
        ));
    }

    let range_weight_bps = input.range_weight_bps.min(10_000);
    let tail_weight_bps = 10_000u16.saturating_sub(range_weight_bps);

    let lower_range_mid = midpoint(input.k1_raw, input.k2_raw)?;

    let down_tail_mid = input.k1_raw.saturating_mul(99).checked_div(100).unwrap_or(input.k1_raw);

    let mut lower_range = tail_ladder_leg(
        AdvancedLegKind::Range,
        "downside_breakdown_zone",
        None,
        Some(input.k1_raw),
        Some(input.k2_raw),
        lower_range_mid,
        input.lower_range_ask_raw,
        input.spot_raw,
        0,
        10_000,
    )?;

    lower_range.base_weight_e6 = scale_weight_bps(lower_range.base_weight_e6, range_weight_bps)?;

    let mut down_tail = tail_ladder_leg(
        AdvancedLegKind::Down,
        "crash_tail",
        Some(input.k1_raw),
        None,
        None,
        down_tail_mid,
        input.down_tail_ask_raw,
        input.spot_raw,
        0,
        input.tail_gamma_bps,
    )?;

    down_tail.base_weight_e6 = scale_weight_bps(down_tail.base_weight_e6, tail_weight_bps)?;

    let mut result = allocate_weighted_budget(
        AdvancedStrategyKind::DownsideConvexity,
        input.budget_raw,
        vec![lower_range, down_tail],
    )?;

    result.warnings.push(
        "Downside Convexity is downside-only. It can expire worthless if BTC settles above the lower breakdown range."
            .to_string(),
    );

    result
        .warnings
        .push("Downside Convexity is terminal-expiry exposure, not a touch option.".to_string());

    Ok(result)
}

pub fn compile_upside_step_ladder(
    input: UpsideStepLadderInput,
) -> Result<AdvancedCompileResult, AdvancedStrategyError> {
    if input.spot_raw == 0 {
        return Err(AdvancedStrategyError::InvalidInput(
            "spot_raw must be greater than zero".to_string(),
        ));
    }

    if !(input.center_raw < input.k3_raw && input.k3_raw < input.k4_raw) {
        return Err(AdvancedStrategyError::InvalidInput(
            "upside step ladder requires center < k3 < k4".to_string(),
        ));
    }

    let near_weight_bps = input.near_range_weight_bps.min(10_000);
    let upper_weight_bps =
        input.upper_range_weight_bps.min(10_000u16.saturating_sub(near_weight_bps));
    let tail_weight_bps =
        10_000u16.saturating_sub(near_weight_bps).saturating_sub(upper_weight_bps);

    let near_mid = midpoint(input.center_raw, input.k3_raw)?;
    let upper_mid = midpoint(input.k3_raw, input.k4_raw)?;
    let tail_mid = input.k4_raw.saturating_mul(101).checked_div(100).unwrap_or(input.k4_raw);

    let mut near_range = tail_ladder_leg(
        AdvancedLegKind::Range,
        "near_upside_step",
        None,
        Some(input.center_raw),
        Some(input.k3_raw),
        near_mid,
        input.near_up_range_ask_raw,
        input.spot_raw,
        0,
        10_000,
    )?;
    near_range.base_weight_e6 = scale_weight_bps(near_range.base_weight_e6, near_weight_bps)?;

    let mut upper_range = tail_ladder_leg(
        AdvancedLegKind::Range,
        "upper_upside_step",
        None,
        Some(input.k3_raw),
        Some(input.k4_raw),
        upper_mid,
        input.upper_range_ask_raw,
        input.spot_raw,
        0,
        12_000,
    )?;
    upper_range.base_weight_e6 = scale_weight_bps(upper_range.base_weight_e6, upper_weight_bps)?;

    let mut up_tail = tail_ladder_leg(
        AdvancedLegKind::Up,
        "upside_continuation_tail",
        Some(input.k4_raw),
        None,
        None,
        tail_mid,
        input.up_tail_ask_raw,
        input.spot_raw,
        0,
        input.tail_gamma_bps,
    )?;
    up_tail.base_weight_e6 = scale_weight_bps(up_tail.base_weight_e6, tail_weight_bps)?;

    let mut result = allocate_weighted_budget(
        AdvancedStrategyKind::UpsideStepLadder,
        input.budget_raw,
        vec![near_range, upper_range, up_tail],
    )?;

    result.warnings.push(
        "Upside Step Ladder is upside-biased. It can expire worthless if BTC settles below the first upside step."
            .to_string(),
    );
    result
        .warnings
        .push("Upside Step Ladder is terminal-expiry exposure, not a touch option.".to_string());

    Ok(result)
}

pub fn compile_downside_step_ladder(
    input: DownsideStepLadderInput,
) -> Result<AdvancedCompileResult, AdvancedStrategyError> {
    if input.spot_raw == 0 {
        return Err(AdvancedStrategyError::InvalidInput(
            "spot_raw must be greater than zero".to_string(),
        ));
    }

    if !(input.k1_raw < input.k2_raw && input.k2_raw < input.center_raw) {
        return Err(AdvancedStrategyError::InvalidInput(
            "downside step ladder requires k1 < k2 < center".to_string(),
        ));
    }

    let near_weight_bps = input.near_range_weight_bps.min(10_000);
    let lower_weight_bps =
        input.lower_range_weight_bps.min(10_000u16.saturating_sub(near_weight_bps));
    let tail_weight_bps =
        10_000u16.saturating_sub(near_weight_bps).saturating_sub(lower_weight_bps);

    let near_mid = midpoint(input.k2_raw, input.center_raw)?;
    let lower_mid = midpoint(input.k1_raw, input.k2_raw)?;
    let tail_mid = input.k1_raw.saturating_mul(99).checked_div(100).unwrap_or(input.k1_raw);

    let mut near_range = tail_ladder_leg(
        AdvancedLegKind::Range,
        "near_downside_step",
        None,
        Some(input.k2_raw),
        Some(input.center_raw),
        near_mid,
        input.near_down_range_ask_raw,
        input.spot_raw,
        0,
        10_000,
    )?;
    near_range.base_weight_e6 = scale_weight_bps(near_range.base_weight_e6, near_weight_bps)?;

    let mut lower_range = tail_ladder_leg(
        AdvancedLegKind::Range,
        "lower_downside_step",
        None,
        Some(input.k1_raw),
        Some(input.k2_raw),
        lower_mid,
        input.lower_range_ask_raw,
        input.spot_raw,
        0,
        12_000,
    )?;
    lower_range.base_weight_e6 = scale_weight_bps(lower_range.base_weight_e6, lower_weight_bps)?;

    let mut down_tail = tail_ladder_leg(
        AdvancedLegKind::Down,
        "downside_continuation_tail",
        Some(input.k1_raw),
        None,
        None,
        tail_mid,
        input.down_tail_ask_raw,
        input.spot_raw,
        0,
        input.tail_gamma_bps,
    )?;
    down_tail.base_weight_e6 = scale_weight_bps(down_tail.base_weight_e6, tail_weight_bps)?;

    let mut result = allocate_weighted_budget(
        AdvancedStrategyKind::DownsideStepLadder,
        input.budget_raw,
        vec![near_range, lower_range, down_tail],
    )?;

    result.warnings.push(
        "Downside Step Ladder is downside-biased. It can expire worthless if BTC settles above the first downside step."
            .to_string(),
    );
    result
        .warnings
        .push("Downside Step Ladder is terminal-expiry exposure, not a touch option.".to_string());

    Ok(result)
}

pub fn compile_center_band_condor(
    input: CenterBandCondorInput,
) -> Result<AdvancedCompileResult, AdvancedStrategyError> {
    if !(input.k1_raw < input.k2_raw
        && input.k2_raw < input.center_raw
        && input.center_raw < input.k3_raw
        && input.k3_raw < input.k4_raw)
    {
        return Err(AdvancedStrategyError::InvalidInput(
            "center band condor requires k1 < k2 < center < k3 < k4".to_string(),
        ));
    }

    let center_weight_bps = input.center_weight_bps.min(10_000);
    let wing_weight_bps = 10_000u16.saturating_sub(center_weight_bps);
    let each_center_bps = center_weight_bps / 2;
    let each_wing_bps = wing_weight_bps / 2;

    let lower_wing_mid = midpoint(input.k1_raw, input.k2_raw)?;
    let lower_center_mid = midpoint(input.k2_raw, input.center_raw)?;
    let upper_center_mid = midpoint(input.center_raw, input.k3_raw)?;
    let upper_wing_mid = midpoint(input.k3_raw, input.k4_raw)?;

    let lower_wing = AdvancedLegInput {
        kind: AdvancedLegKind::Range,
        role: "lower_outside_wing",
        strike_raw: None,
        lower_raw: Some(input.k1_raw),
        upper_raw: Some(input.k2_raw),
        midpoint_raw: lower_wing_mid,
        ask_price_raw: input.lower_wing_ask_raw,
        base_weight_e6: scale_weight_bps(1_000_000, each_wing_bps)?,
        max_quantity: None,
    };

    let lower_center = AdvancedLegInput {
        kind: AdvancedLegKind::Range,
        role: "lower_center_band",
        strike_raw: None,
        lower_raw: Some(input.k2_raw),
        upper_raw: Some(input.center_raw),
        midpoint_raw: lower_center_mid,
        ask_price_raw: input.lower_center_ask_raw,
        base_weight_e6: scale_weight_bps(1_000_000, each_center_bps)?,
        max_quantity: None,
    };

    let upper_center = AdvancedLegInput {
        kind: AdvancedLegKind::Range,
        role: "upper_center_band",
        strike_raw: None,
        lower_raw: Some(input.center_raw),
        upper_raw: Some(input.k3_raw),
        midpoint_raw: upper_center_mid,
        ask_price_raw: input.upper_center_ask_raw,
        base_weight_e6: scale_weight_bps(1_000_000, each_center_bps)?,
        max_quantity: None,
    };

    let upper_wing = AdvancedLegInput {
        kind: AdvancedLegKind::Range,
        role: "upper_outside_wing",
        strike_raw: None,
        lower_raw: Some(input.k3_raw),
        upper_raw: Some(input.k4_raw),
        midpoint_raw: upper_wing_mid,
        ask_price_raw: input.upper_wing_ask_raw,
        base_weight_e6: scale_weight_bps(1_000_000, each_wing_bps)?,
        max_quantity: None,
    };

    let mut result = allocate_weighted_budget(
        AdvancedStrategyKind::CenterBandCondor,
        input.budget_raw,
        vec![lower_wing, lower_center, upper_center, upper_wing],
    )?;

    result.warnings.push(
        "Center Band Condor is terminal-settlement only. It does not pay for staying inside the corridor before expiry."
            .to_string(),
    );
    result.warnings.push(
        "This structure concentrates payout near the center band and keeps smaller outside-wing exposure."
            .to_string(),
    );

    Ok(result)
}

fn expiry_move_leg(
    kind: AdvancedLegKind,
    role: &'static str,
    strike_raw: Option<u64>,
    lower_raw: Option<u64>,
    upper_raw: Option<u64>,
    midpoint_raw: u64,
    ask_price_raw: u64,
    spot_raw: u64,
) -> Result<AdvancedLegInput, AdvancedStrategyError> {
    let move_bps = abs_return_bps(midpoint_raw, spot_raw)?;

    let weight_e6 = (move_bps as u128)
        .checked_mul(1_000_000)
        .ok_or(AdvancedStrategyError::Overflow)?
        .min(u64::MAX as u128) as u64;

    Ok(AdvancedLegInput {
        kind,
        role,
        strike_raw,
        lower_raw,
        upper_raw,
        midpoint_raw,
        ask_price_raw,
        base_weight_e6: weight_e6,
        max_quantity: None,
    })
}

fn scale_weight_bps(weight: u64, bps: u16) -> Result<u64, AdvancedStrategyError> {
    let scaled =
        (weight as u128).checked_mul(bps as u128).ok_or(AdvancedStrategyError::Overflow)? / 10_000;

    u64_checked(scaled)
}

fn crash_bucket_leg(
    kind: AdvancedLegKind,
    role: &'static str,
    strike_raw: Option<u64>,
    lower_raw: Option<u64>,
    upper_raw: Option<u64>,
    midpoint_raw: u64,
    ask_price_raw: u64,
    spot_raw: u64,
    exposure_raw: u64,
    over_hedge_cap_bps: u16,
    gamma_bps: u16,
) -> Result<AdvancedLegInput, AdvancedStrategyError> {
    let portfolio_loss = portfolio_loss_raw(exposure_raw, spot_raw, midpoint_raw)?;

    let weight_e6 = power_weight_e6(portfolio_loss, gamma_bps)?;

    let max_quantity = if portfolio_loss == 0 {
        Some(0)
    } else {
        let capped = (portfolio_loss as u128)
            .checked_mul(over_hedge_cap_bps as u128)
            .ok_or(AdvancedStrategyError::Overflow)?
            / 10_000;
        Some(u64_checked(capped)?)
    };

    Ok(AdvancedLegInput {
        kind,
        role,
        strike_raw,
        lower_raw,
        upper_raw,
        midpoint_raw,
        ask_price_raw,
        base_weight_e6: weight_e6,
        max_quantity,
    })
}

fn tail_ladder_leg(
    kind: AdvancedLegKind,
    role: &'static str,
    strike_raw: Option<u64>,
    lower_raw: Option<u64>,
    upper_raw: Option<u64>,
    midpoint_raw: u64,
    ask_price_raw: u64,
    spot_raw: u64,
    dead_zone_bps: u16,
    gamma_bps: u16,
) -> Result<AdvancedLegInput, AdvancedStrategyError> {
    let move_bps = abs_return_bps(midpoint_raw, spot_raw)?;
    let excess_bps = move_bps.saturating_sub(dead_zone_bps as u64);

    let weight_e6 = power_weight_e6(excess_bps, gamma_bps)?;

    Ok(AdvancedLegInput {
        kind,
        role,
        strike_raw,
        lower_raw,
        upper_raw,
        midpoint_raw,
        ask_price_raw,
        base_weight_e6: weight_e6,
        max_quantity: None,
    })
}

fn portfolio_loss_raw(
    exposure_raw: u64,
    spot_raw: u64,
    midpoint_raw: u64,
) -> Result<u64, AdvancedStrategyError> {
    if midpoint_raw >= spot_raw {
        return Ok(0);
    }

    let price_drop = spot_raw - midpoint_raw;

    let loss = (exposure_raw as u128)
        .checked_mul(price_drop as u128)
        .ok_or(AdvancedStrategyError::Overflow)?
        / spot_raw as u128;

    u64_checked(loss)
}

fn abs_return_bps(midpoint_raw: u64, spot_raw: u64) -> Result<u64, AdvancedStrategyError> {
    if spot_raw == 0 {
        return Err(AdvancedStrategyError::InvalidInput(
            "spot_raw must be greater than zero".to_string(),
        ));
    }

    let diff = midpoint_raw.abs_diff(spot_raw);

    let bps = (diff as u128).checked_mul(10_000).ok_or(AdvancedStrategyError::Overflow)?
        / spot_raw as u128;

    u64_checked(bps)
}

fn power_weight_e6(value: u64, gamma_bps: u16) -> Result<u64, AdvancedStrategyError> {
    if value == 0 {
        return Ok(0);
    }

    // Integer-friendly MVP:
    // gamma <= 1.0  -> linear
    // gamma > 1.0   -> convex but bounded: value * value / scale
    // This avoids floating point in core deterministic allocation.
    let weight = if gamma_bps <= 10_000 {
        (value as u128).checked_mul(1_000_000).ok_or(AdvancedStrategyError::Overflow)?
    } else {
        (value as u128)
            .checked_mul(value as u128)
            .ok_or(AdvancedStrategyError::Overflow)?
            .checked_mul(1_000_000)
            .ok_or(AdvancedStrategyError::Overflow)?
            / 10_000u128.max(value as u128)
    };

    let bounded = weight.min(u64::MAX as u128);

    Ok(bounded as u64)
}

fn midpoint(a: u64, b: u64) -> Result<u64, AdvancedStrategyError> {
    let sum = (a as u128).checked_add(b as u128).ok_or(AdvancedStrategyError::Overflow)?;
    u64_checked(sum / 2)
}

fn ceil_div(numerator: u128, denominator: u128) -> Result<u128, AdvancedStrategyError> {
    if denominator == 0 {
        return Err(AdvancedStrategyError::Overflow);
    }

    Ok(numerator.checked_add(denominator - 1).ok_or(AdvancedStrategyError::Overflow)? / denominator)
}

fn u64_checked(value: u128) -> Result<u64, AdvancedStrategyError> {
    u64::try_from(value).map_err(|_| AdvancedStrategyError::Overflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crash_shield_caps_overhedge_and_may_leave_budget_unused() {
        let input = PortfolioCrashShieldInput {
            spot_raw: 100_000_000_000_000,
            exposure_raw: 5_000_000_000,
            budget_raw: 200_000_000,
            over_hedge_cap_bps: 12_000,
            gamma_bps: 10_000,
            down_tail_strike_raw: 90_000_000_000_000,
            lower_range_upper_raw: 95_000_000_000_000,
            mild_range_upper_raw: Some(98_000_000_000_000),
            down_tail_ask_raw: 45_000_000,
            lower_range_ask_raw: 95_000_000,
            mild_range_ask_raw: Some(120_000_000),
        };

        let result = compile_portfolio_crash_shield(input).unwrap();

        assert_eq!(result.strategy, AdvancedStrategyKind::PortfolioCrashShield);
        assert_eq!(result.legs.len(), 3);
        assert!(result.used_budget_raw <= input.budget_raw);
        assert!(result.unused_budget_raw > 0);
        assert!(result.legs[0].quantity <= result.legs[0].max_quantity.unwrap());
    }

    #[test]
    fn convex_tail_ladder_allocates_four_legs() {
        let input = ConvexTailLadderInput {
            spot_raw: 100_000_000_000_000,
            budget_raw: 100_000_000,
            dead_zone_bps: 200,
            gamma_bps: 15_000,
            k1_raw: 95_000_000_000_000,
            k2_raw: 98_000_000_000_000,
            k3_raw: 102_000_000_000_000,
            k4_raw: 105_000_000_000_000,
            down_tail_ask_raw: 80_000_000,
            lower_range_ask_raw: 130_000_000,
            upper_range_ask_raw: 120_000_000,
            up_tail_ask_raw: 70_000_000,
        };

        let result = compile_convex_tail_ladder(input).unwrap();

        assert_eq!(result.strategy, AdvancedStrategyKind::ConvexTailLadder);
        assert_eq!(result.legs.len(), 4);
        assert!(result.used_budget_raw <= input.budget_raw);
        assert!(result.legs.iter().all(|leg| leg.quantity > 0));
    }

    #[test]
    fn weighted_allocator_rejects_zero_budget() {
        let err = allocate_weighted_budget(AdvancedStrategyKind::ConvexTailLadder, 0, vec![])
            .unwrap_err();

        assert!(matches!(err, AdvancedStrategyError::ZeroBudget));
    }

    #[test]
    fn expiry_move_note_allocates_by_terminal_distance() {
        let input = ExpiryMoveNoteInput {
            spot_raw: 100_000_000_000_000,
            budget_raw: 100_000_000,
            k1_raw: 95_000_000_000_000,
            k2_raw: 98_000_000_000_000,
            k3_raw: 102_000_000_000_000,
            k4_raw: 105_000_000_000_000,
            down_tail_ask_raw: 80_000_000,
            lower_range_ask_raw: 130_000_000,
            upper_range_ask_raw: 120_000_000,
            up_tail_ask_raw: 70_000_000,
        };

        let result = compile_expiry_move_note(input).unwrap();

        assert_eq!(result.strategy, AdvancedStrategyKind::ExpiryMoveNote);
        assert_eq!(result.legs.len(), 4);
        assert!(result.used_budget_raw <= input.budget_raw);
        assert!(result.legs.iter().all(|leg| leg.quantity > 0));

        let down_tail = result.legs.iter().find(|leg| leg.role == "large_downside_move").unwrap();

        let lower_range =
            result.legs.iter().find(|leg| leg.role == "moderate_downside_move").unwrap();

        assert!(down_tail.weight_e6 > lower_range.weight_e6);
    }

    #[test]
    fn smart_candidate_score_prefers_hit_rate_when_requested() {
        let crash = SmartCandidateMetrics {
            premium_raw: 100,
            max_payout_raw: 1_000,
            expected_payout_raw: 110,
            hit_probability_bps: 2_000,
            worst_case_improvement_raw: 900,
            complexity_penalty_bps: 100,
        };

        let expiry_move = SmartCandidateMetrics {
            premium_raw: 100,
            max_payout_raw: 550,
            expected_payout_raw: 150,
            hit_probability_bps: 4_500,
            worst_case_improvement_raw: 500,
            complexity_penalty_bps: 100,
        };

        let crash_score = score_smart_candidate(crash, SmartBudgetStyle::HigherHitRate).unwrap();
        let expiry_score =
            score_smart_candidate(expiry_move, SmartBudgetStyle::HigherHitRate).unwrap();

        assert!(expiry_score.hit_probability_score_e6 > crash_score.hit_probability_score_e6);
    }

    #[test]
    fn smart_candidate_score_tail_heavy_values_max_payout() {
        let tail = SmartCandidateMetrics {
            premium_raw: 100,
            max_payout_raw: 1_200,
            expected_payout_raw: 80,
            hit_probability_bps: 1_500,
            worst_case_improvement_raw: 900,
            complexity_penalty_bps: 100,
        };

        let range = SmartCandidateMetrics {
            premium_raw: 100,
            max_payout_raw: 300,
            expected_payout_raw: 160,
            hit_probability_bps: 5_000,
            worst_case_improvement_raw: 100,
            complexity_penalty_bps: 100,
        };

        let tail_score = score_smart_candidate(tail, SmartBudgetStyle::TailHeavy).unwrap();
        let range_score = score_smart_candidate(range, SmartBudgetStyle::TailHeavy).unwrap();

        assert!(tail_score.score_e6 > range_score.score_e6);
    }

    #[test]
    fn moonshot_upside_allocates_range_and_tail() {
        let input = MoonshotUpsideInput {
            spot_raw: 100_000_000_000_000,
            budget_raw: 100_000_000,
            k3_raw: 102_000_000_000_000,
            k4_raw: 105_000_000_000_000,
            upper_range_ask_raw: 120_000_000,
            up_tail_ask_raw: 70_000_000,
            range_weight_bps: 6_000,
            tail_gamma_bps: 15_000,
        };

        let result = compile_moonshot_upside(input).unwrap();

        assert_eq!(result.strategy, AdvancedStrategyKind::MoonshotUpside);
        assert_eq!(result.legs.len(), 2);
        assert!(result.used_budget_raw <= input.budget_raw);
        assert!(result.legs.iter().any(|leg| leg.role == "upside_breakout_zone"));
        assert!(result.legs.iter().any(|leg| leg.role == "moonshot_tail"));
    }

    #[test]
    fn range_conviction_allocates_single_range() {
        let input = RangeConvictionInput {
            budget_raw: 50_000_000,
            lower_raw: 98_000_000_000_000,
            upper_raw: 102_000_000_000_000,
            range_ask_raw: 125_000_000,
        };

        let result = compile_range_conviction(input).unwrap();

        assert_eq!(result.strategy, AdvancedStrategyKind::RangeConviction);
        assert_eq!(result.legs.len(), 1);
        assert_eq!(result.legs[0].role, "central_range_conviction");
        assert_eq!(result.legs[0].kind, AdvancedLegKind::Range);
        assert!(result.used_budget_raw <= input.budget_raw);
        assert!(result.legs[0].quantity > 0);
    }

    #[test]
    fn downside_convexity_allocates_range_and_tail() {
        let input = DownsideConvexityInput {
            spot_raw: 100_000_000_000_000,
            budget_raw: 100_000_000,
            k1_raw: 95_000_000_000_000,
            k2_raw: 98_000_000_000_000,
            down_tail_ask_raw: 70_000_000,
            lower_range_ask_raw: 120_000_000,
            range_weight_bps: 6_000,
            tail_gamma_bps: 15_000,
        };

        let result = compile_downside_convexity(input).unwrap();

        assert_eq!(result.strategy, AdvancedStrategyKind::DownsideConvexity);
        assert_eq!(result.legs.len(), 2);
        assert!(result.used_budget_raw <= input.budget_raw);

        assert!(result.legs.iter().any(|leg| leg.role == "downside_breakdown_zone"));

        assert!(result.legs.iter().any(|leg| leg.role == "crash_tail"));
    }

    #[test]
    fn upside_step_ladder_allocates_three_upside_steps() {
        let input = UpsideStepLadderInput {
            spot_raw: 100_000_000_000_000,
            budget_raw: 100_000_000,
            center_raw: 100_000_000_000_000,
            k3_raw: 102_000_000_000_000,
            k4_raw: 105_000_000_000_000,
            near_up_range_ask_raw: 180_000_000,
            upper_range_ask_raw: 120_000_000,
            up_tail_ask_raw: 70_000_000,
            near_range_weight_bps: 4_000,
            upper_range_weight_bps: 3_500,
            tail_gamma_bps: 15_000,
        };

        let result = compile_upside_step_ladder(input).unwrap();

        assert_eq!(result.strategy, AdvancedStrategyKind::UpsideStepLadder);
        assert_eq!(result.legs.len(), 3);
        assert!(result.used_budget_raw <= input.budget_raw);
        assert!(result.legs.iter().any(|leg| leg.role == "near_upside_step"));
        assert!(result.legs.iter().any(|leg| leg.role == "upper_upside_step"));
        assert!(result.legs.iter().any(|leg| leg.role == "upside_continuation_tail"));
    }

    #[test]
    fn downside_step_ladder_allocates_three_downside_steps() {
        let input = DownsideStepLadderInput {
            spot_raw: 100_000_000_000_000,
            budget_raw: 100_000_000,
            k1_raw: 95_000_000_000_000,
            k2_raw: 98_000_000_000_000,
            center_raw: 100_000_000_000_000,
            down_tail_ask_raw: 70_000_000,
            lower_range_ask_raw: 120_000_000,
            near_down_range_ask_raw: 180_000_000,
            near_range_weight_bps: 4_000,
            lower_range_weight_bps: 3_500,
            tail_gamma_bps: 15_000,
        };

        let result = compile_downside_step_ladder(input).unwrap();

        assert_eq!(result.strategy, AdvancedStrategyKind::DownsideStepLadder);
        assert_eq!(result.legs.len(), 3);
        assert!(result.used_budget_raw <= input.budget_raw);
        assert!(result.legs.iter().any(|leg| leg.role == "near_downside_step"));
        assert!(result.legs.iter().any(|leg| leg.role == "lower_downside_step"));
        assert!(result.legs.iter().any(|leg| leg.role == "downside_continuation_tail"));
    }

    #[test]
    fn center_band_condor_allocates_four_range_legs() {
        let input = CenterBandCondorInput {
            budget_raw: 100_000_000,
            k1_raw: 95_000_000_000_000,
            k2_raw: 98_000_000_000_000,
            center_raw: 100_000_000_000_000,
            k3_raw: 102_000_000_000_000,
            k4_raw: 105_000_000_000_000,
            lower_wing_ask_raw: 90_000_000,
            lower_center_ask_raw: 180_000_000,
            upper_center_ask_raw: 170_000_000,
            upper_wing_ask_raw: 95_000_000,
            center_weight_bps: 8_000,
        };

        let result = compile_center_band_condor(input).unwrap();

        assert_eq!(result.strategy, AdvancedStrategyKind::CenterBandCondor);
        assert_eq!(result.legs.len(), 4);
        assert!(result.used_budget_raw <= input.budget_raw);
        assert!(result.legs.iter().any(|leg| leg.role == "lower_center_band"));
        assert!(result.legs.iter().any(|leg| leg.role == "upper_center_band"));
        assert!(result.legs.iter().all(|leg| leg.kind == AdvancedLegKind::Range));
    }
}
