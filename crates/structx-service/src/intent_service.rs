use anyhow::{anyhow, Context};
use serde::{Deserialize, Serialize};

use crate::intent::{
    Direction, ExpiryPreferenceOverride, IntentConfidence, IntentPlan, RangeIntent, RiskStyle,
    StrategyTemplateId, UserIntentRequest,
};
use crate::market_catalog::{
    CatalogMarketSnapshot, ExpiryPreference, MarketCategory, MarketKind, MarketSearchQuery,
};
use crate::market_store::MarketStore;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentPlanningResponse {
    pub intent_plan: IntentPlan,
    pub candidate_markets: Vec<CatalogMarketSnapshot>,
    pub selected_market: Option<CatalogMarketSnapshot>,
    pub needs_clarification: bool,
    pub clarification_question: Option<String>,
}

pub async fn plan_from_intent<S: MarketStore + ?Sized>(
    store: &S,
    request: UserIntentRequest,
) -> anyhow::Result<IntentPlanningResponse> {
    let intent_plan = parse_intent_deterministic(request)?;

    if intent_plan.needs_clarification {
        return Ok(IntentPlanningResponse {
            candidate_markets: vec![],
            selected_market: None,
            needs_clarification: true,
            clarification_question: intent_plan.clarification_question.clone(),
            intent_plan,
        });
    }

    let candidate_markets = search_candidate_markets(store, &intent_plan).await?;

    let selected_market = choose_selected_market(&candidate_markets, &intent_plan);
    let needs_clarification = selected_market.is_none();

    let clarification_question = if needs_clarification {
        if candidate_markets.is_empty() {
            Some(format!(
                "I couldn't find an active DeepBook Predict market in the local catalog matching '{}'. The catalog may need a refresh, or there may be no live match right now.",
                intent_plan.market_query
            ))
        } else {
            Some(
                "I found candidate markets, but none of them look openable through StructX right now. Review the market list or refresh the catalog."
                    .to_string(),
            )
        }
    } else {
        None
    };

    Ok(IntentPlanningResponse {
        intent_plan,
        candidate_markets,
        selected_market,
        needs_clarification,
        clarification_question,
    })
}

pub fn parse_intent_deterministic(request: UserIntentRequest) -> anyhow::Result<IntentPlan> {
    let raw_prompt = request.prompt.trim().to_string();
    if raw_prompt.is_empty() {
        return Err(anyhow!("prompt cannot be empty"));
    }

    let normalized = normalize(&raw_prompt);
    let mut assumptions = Vec::new();
    let mut warnings = Vec::new();

    let market_query = infer_market_query(&normalized).unwrap_or_else(|| {
        warnings.push(
            "Could not confidently infer market; defaulting query to full prompt.".to_string(),
        );
        raw_prompt.clone()
    });

    let category_hint = infer_category_hint(&normalized, &market_query);
    let market_kind_hint = infer_market_kind_hint(&normalized, &market_query);
    let direction = infer_direction(&normalized);
    let range = infer_range(&normalized);
    let strategy_template = infer_strategy_template(&normalized, &direction, &range);
    let budget = request
        .budget
        .or_else(|| infer_budget_from_prompt(&normalized));
    let quote_asset = request
        .quote_asset
        .filter(|asset| !asset.trim().is_empty())
        .unwrap_or_else(|| "DUSDC".to_string());
    let risk_style = request
        .risk_style
        .unwrap_or_else(|| infer_risk_style(&normalized));
    let expiry_preference = infer_expiry_preference(&normalized);

    let mut needs_clarification = false;
    let mut clarification_question = None;

    if budget.is_none() {
        needs_clarification = true;
        set_clarification_once(
            &mut clarification_question,
            "How much dUSDC do you want to spend?",
        );
    }

    if market_query.trim().is_empty() {
        needs_clarification = true;
        set_clarification_once(
            &mut clarification_question,
            "Which market or asset do you want to trade?",
        );
    }

    if strategy_template == StrategyTemplateId::CustomPiecewise {
        assumptions.push(
            "Custom payoff intent detected; exact payoff buckets may need Advanced Mode confirmation."
                .to_string(),
        );
    }

    if normalized.contains("touch") || normalized.contains("hit ") || normalized.contains("reaches")
    {
        warnings.push(
            "DeepBook Predict settles at expiry; this is not a touch/barrier product unless the protocol market explicitly settles that way."
                .to_string(),
        );
    }

    let confidence = infer_confidence(
        &market_query,
        budget,
        &strategy_template,
        needs_clarification,
        &warnings,
    );

    Ok(IntentPlan {
        raw_prompt,
        market_query,
        category_hint,
        market_kind_hint,
        strategy_template,
        direction: Some(direction),
        range,
        budget,
        quote_asset,
        risk_style,
        expiry_preference,
        confidence,
        needs_clarification,
        clarification_question,
        assumptions,
        warnings,
    })
}

fn choose_selected_market(
    candidates: &[CatalogMarketSnapshot],
    intent: &IntentPlan,
) -> Option<CatalogMarketSnapshot> {
    let first_openable = candidates.iter().find(|market| is_structx_openable(market));

    match intent.confidence {
        IntentConfidence::High | IntentConfidence::Medium => first_openable.cloned(),
        IntentConfidence::Low | IntentConfidence::None => None,
    }
}

async fn search_candidate_markets<S: MarketStore + ?Sized>(
    store: &S,
    intent_plan: &IntentPlan,
) -> anyhow::Result<Vec<CatalogMarketSnapshot>> {
    let base_query = MarketSearchQuery {
        text: intent_plan.market_query.clone(),
        category_hint: intent_plan.category_hint.clone(),
        market_kind_hint: intent_plan.market_kind_hint.clone(),
        require_active: true,
        quote_asset: Some(intent_plan.quote_asset.clone()),
        expiry_preference: Some(map_expiry_preference(&intent_plan.expiry_preference)),
    };

    let first_pass = store
        .search_markets(base_query.clone())
        .await
        .context("failed to search market catalog from intent")?;
    if !first_pass.is_empty() {
        return Ok(first_pass);
    }

    if intent_plan.market_kind_hint.is_some() {
        let relaxed_kind = store
            .search_markets(MarketSearchQuery {
                market_kind_hint: None,
                ..base_query.clone()
            })
            .await
            .context("failed to retry market search without market kind hint")?;
        if !relaxed_kind.is_empty() {
            return Ok(relaxed_kind);
        }
    }

    if intent_plan.category_hint.is_some() {
        let relaxed_category = store
            .search_markets(MarketSearchQuery {
                category_hint: None,
                market_kind_hint: None,
                ..base_query
            })
            .await
            .context("failed to retry market search without category hint")?;
        if !relaxed_category.is_empty() {
            return Ok(relaxed_category);
        }
    }

    Ok(vec![])
}

fn is_structx_openable(market: &CatalogMarketSnapshot) -> bool {
    if market.metadata.as_str() == Some("Usable") {
        return true;
    }

    market
        .metadata
        .get("structx_status")
        .map(|status| {
            status.as_str() == Some("Usable")
                || status
                    .as_object()
                    .map(|obj| obj.contains_key("UsableWithWarnings"))
                    .unwrap_or(false)
        })
        .unwrap_or(false)
}

fn set_clarification_once(slot: &mut Option<String>, message: &str) {
    if slot.is_none() {
        *slot = Some(message.to_string());
    }
}

fn map_expiry_preference(input: &ExpiryPreferenceOverride) -> ExpiryPreference {
    match input {
        ExpiryPreferenceOverride::NearestActive => ExpiryPreference::NearestActive,
        ExpiryPreferenceOverride::ThisWeek => ExpiryPreference::NearestActive,
        ExpiryPreferenceOverride::Soonest => ExpiryPreference::Soonest,
        ExpiryPreferenceOverride::Latest => ExpiryPreference::Latest,
        ExpiryPreferenceOverride::Any => ExpiryPreference::Any,
    }
}

fn infer_market_query(text: &str) -> Option<String> {
    if contains_any(text, &["btc", "bitcoin"]) {
        return Some("BTC".to_string());
    }
    if contains_any(text, &["eth", "ethereum"]) {
        return Some("ETH".to_string());
    }
    if contains_any(text, &["sui"]) {
        return Some("SUI".to_string());
    }
    if contains_any(text, &["sol", "solana"]) {
        return Some("SOL".to_string());
    }
    None
}

fn infer_category_hint(text: &str, market_query: &str) -> Option<MarketCategory> {
    let combined = format!("{} {}", text, market_query).to_ascii_lowercase();
    if contains_any(
        &combined,
        &[
            "btc", "bitcoin", "eth", "ethereum", "sui", "crypto", "solana",
        ],
    ) {
        return Some(MarketCategory::Crypto);
    }
    if contains_any(&combined, &["election", "president", "senate", "politic"]) {
        return Some(MarketCategory::Politics);
    }
    if contains_any(&combined, &["nba", "nfl", "cricket", "football", "sport"]) {
        return Some(MarketCategory::Sports);
    }
    if contains_any(&combined, &["cpi", "inflation", "fed", "macro"]) {
        return Some(MarketCategory::Macro);
    }
    None
}

fn infer_market_kind_hint(text: &str, market_query: &str) -> Option<MarketKind> {
    let combined = format!("{} {}", text, market_query).to_ascii_lowercase();
    if contains_any(
        &combined,
        &["btc", "bitcoin", "eth", "ethereum", "sui", "sol", "price"],
    ) {
        return Some(MarketKind::ScalarPrice);
    }
    if contains_any(
        &combined,
        &["above", "below", "between", "range", "score", "vote share"],
    ) {
        return Some(MarketKind::ScalarEvent);
    }
    None
}

fn infer_direction(text: &str) -> Direction {
    if contains_any(
        text,
        &[
            "either side",
            "big move",
            "breakout",
            "volatile",
            "volatility",
        ],
    ) {
        return Direction::EitherSide;
    }
    if contains_any(text, &["between", "range", "sideways", "inside"]) {
        return Direction::InsideRange;
    }
    if contains_any(
        text,
        &["pump", "moon", "up", "above", "higher", "bullish", "rally"],
    ) {
        return Direction::Up;
    }
    if contains_any(
        text,
        &[
            "dump", "crash", "down", "below", "lower", "bearish", "protect", "hedge",
        ],
    ) {
        return Direction::Down;
    }
    Direction::Unknown
}

fn infer_strategy_template(
    text: &str,
    direction: &Direction,
    range: &Option<RangeIntent>,
) -> StrategyTemplateId {
    if contains_any(
        text,
        &[
            "choose for me",
            "best thing",
            "find best",
            "smart",
            "optimize",
        ],
    ) {
        return StrategyTemplateId::SmartBudget;
    }
    if contains_any(
        text,
        &["custom payoff", "piecewise", "payoff shape", "buckets"],
    ) {
        return StrategyTemplateId::CustomPiecewise;
    }
    if range.is_some() || matches!(direction, Direction::InsideRange) {
        return StrategyTemplateId::RangeInside;
    }
    if matches!(direction, Direction::EitherSide) {
        return StrategyTemplateId::BreakoutOutside;
    }
    if contains_any(text, &["protect", "hedge", "insurance"]) {
        return StrategyTemplateId::OneSidedTail;
    }
    if contains_any(text, &["moon", "rocket", "pump hard", "max upside"]) {
        return StrategyTemplateId::UpsideRocket;
    }

    match direction {
        Direction::Up => StrategyTemplateId::DirectionalAbove,
        Direction::Down => StrategyTemplateId::DirectionalBelow,
        Direction::EitherSide => StrategyTemplateId::BreakoutOutside,
        Direction::InsideRange => StrategyTemplateId::RangeInside,
        Direction::Unknown => StrategyTemplateId::SmartBudget,
    }
}

fn infer_range(text: &str) -> Option<RangeIntent> {
    if !(text.contains("between") || text.contains("range") || text.contains("inside")) {
        return None;
    }

    let nums = extract_numeric_tokens(text);
    if nums.len() >= 2 {
        let mut a = nums[0];
        let mut b = nums[1];
        if a > b {
            std::mem::swap(&mut a, &mut b);
        }
        return Some(RangeIntent {
            lower: Some(a),
            upper: Some(b),
        });
    }

    Some(RangeIntent {
        lower: None,
        upper: None,
    })
}

fn infer_budget_from_prompt(text: &str) -> Option<u64> {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    for (idx, token) in tokens.iter().enumerate() {
        let cleaned = token
            .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.')
            .to_ascii_lowercase();

        let Some(value) = parse_human_number(&cleaned) else {
            continue;
        };

        let next = tokens
            .get(idx + 1)
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();
        let prev = if idx > 0 {
            tokens
                .get(idx - 1)
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_default()
        } else {
            String::new()
        };

        let looks_like_budget = next.contains("dusdc")
            || next.contains("usdc")
            || prev.contains("spend")
            || prev.contains("budget")
            || prev.contains("with")
            || token.starts_with('$');

        if looks_like_budget && value > 0.0 {
            return Some((value * 1_000_000_000.0).round() as u64);
        }
    }
    None
}

fn infer_risk_style(text: &str) -> RiskStyle {
    if contains_any(text, &["safe", "conservative", "low risk"]) {
        return RiskStyle::Conservative;
    }
    if contains_any(text, &["aggressive", "degen", "max payout", "high risk"]) {
        return RiskStyle::Aggressive;
    }
    if contains_any(text, &["tail", "tail heavy", "big payout"]) {
        return RiskStyle::TailHeavy;
    }
    if contains_any(text, &["higher hit", "more likely", "higher chance"]) {
        return RiskStyle::HigherHitRate;
    }
    RiskStyle::Balanced
}

fn infer_expiry_preference(text: &str) -> ExpiryPreferenceOverride {
    if contains_any(text, &["this week", "weekly", "week"]) {
        return ExpiryPreferenceOverride::ThisWeek;
    }
    if contains_any(text, &["soonest", "nearest", "next expiry"]) {
        return ExpiryPreferenceOverride::NearestActive;
    }
    if contains_any(text, &["latest", "longest"]) {
        return ExpiryPreferenceOverride::Latest;
    }
    ExpiryPreferenceOverride::NearestActive
}

fn infer_confidence(
    market_query: &str,
    budget: Option<u64>,
    strategy_template: &StrategyTemplateId,
    needs_clarification: bool,
    warnings: &[String],
) -> IntentConfidence {
    if needs_clarification {
        return IntentConfidence::Low;
    }
    if market_query.trim().is_empty() || budget.is_none() {
        return IntentConfidence::Low;
    }
    if !warnings.is_empty() {
        return IntentConfidence::Medium;
    }
    if matches!(strategy_template, StrategyTemplateId::SmartBudget) {
        return IntentConfidence::Medium;
    }
    IntentConfidence::High
}

fn extract_numeric_tokens(text: &str) -> Vec<f64> {
    text.split_whitespace()
        .filter_map(|token| {
            let cleaned = token
                .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.' && c != '$')
                .to_ascii_lowercase();
            parse_human_number(&cleaned)
        })
        .collect()
}

fn parse_human_number(input: &str) -> Option<f64> {
    let mut s = input.trim().trim_start_matches('$').to_string();
    if s.is_empty() {
        return None;
    }

    let multiplier = if s.ends_with('k') {
        s.pop();
        1_000.0
    } else if s.ends_with('m') {
        s.pop();
        1_000_000.0
    } else {
        1.0
    };

    let value = s.replace(',', "").parse::<f64>().ok()?;
    if value.is_finite() && value >= 0.0 {
        Some(value * multiplier)
    } else {
        None
    }
}

fn normalize(input: &str) -> String {
    input
        .trim()
        .to_ascii_lowercase()
        .replace(['-', '_', '/'], " ")
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_btc_pump_with_budget() {
        let plan = parse_intent_deterministic(UserIntentRequest {
            user_address: None,
            prompt: "I think BTC will pump this week with 100 dUSDC".to_string(),
            budget: None,
            quote_asset: None,
            risk_style: None,
        })
        .unwrap();

        assert_eq!(plan.market_query, "BTC");
        assert_eq!(plan.strategy_template, StrategyTemplateId::DirectionalAbove);
        assert_eq!(plan.budget, Some(100_000_000_000));
        assert_eq!(plan.quote_asset, "DUSDC");
        assert!(!plan.needs_clarification);
    }

    #[test]
    fn parses_downside_protection() {
        let plan = parse_intent_deterministic(UserIntentRequest {
            user_address: None,
            prompt: "Protect me if bitcoin crashes with 50 dusdc".to_string(),
            budget: None,
            quote_asset: None,
            risk_style: None,
        })
        .unwrap();

        assert_eq!(plan.market_query, "BTC");
        assert_eq!(plan.strategy_template, StrategyTemplateId::OneSidedTail);
        assert_eq!(plan.direction, Some(Direction::Down));
    }

    #[test]
    fn asks_for_budget_if_missing() {
        let plan = parse_intent_deterministic(UserIntentRequest {
            user_address: None,
            prompt: "BTC breakout this week".to_string(),
            budget: None,
            quote_asset: None,
            risk_style: None,
        })
        .unwrap();

        assert!(plan.needs_clarification);
        assert!(plan.clarification_question.unwrap().contains("dUSDC"));
    }

    #[test]
    fn parses_range_intent() {
        let plan = parse_intent_deterministic(UserIntentRequest {
            user_address: None,
            prompt: "BTC between 100k and 110k with 25 dusdc".to_string(),
            budget: None,
            quote_asset: None,
            risk_style: None,
        })
        .unwrap();

        assert_eq!(plan.strategy_template, StrategyTemplateId::RangeInside);
        assert_eq!(plan.range.unwrap().lower, Some(100_000.0));
    }
}
