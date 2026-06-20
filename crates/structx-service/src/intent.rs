use serde::{Deserialize, Serialize};

use crate::market_catalog::{MarketCategory, MarketKind};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StrategyTemplateId {
    DirectionalAbove,
    DirectionalBelow,
    RangeInside,
    BreakoutOutside,
    OneSidedTail,
    UpsideRocket,
    CustomPiecewise,
    SmartBudget,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Up,
    Down,
    EitherSide,
    InsideRange,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RiskStyle {
    Conservative,
    #[default]
    Balanced,
    Aggressive,
    TailHeavy,
    HigherHitRate,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IntentConfidence {
    High,
    Medium,
    Low,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExpiryPreferenceOverride {
    #[default]
    NearestActive,
    ThisWeek,
    Soonest,
    Latest,
    Any,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RangeIntent {
    pub lower: Option<f64>,
    pub upper: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserIntentRequest {
    pub user_address: Option<String>,
    pub prompt: String,
    pub budget: Option<u64>,
    pub quote_asset: Option<String>,
    pub risk_style: Option<RiskStyle>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentPlan {
    pub raw_prompt: String,
    pub market_query: String,
    pub category_hint: Option<MarketCategory>,
    pub market_kind_hint: Option<MarketKind>,
    pub strategy_template: StrategyTemplateId,
    pub direction: Option<Direction>,
    pub range: Option<RangeIntent>,
    pub budget: Option<u64>,
    pub quote_asset: String,
    pub risk_style: RiskStyle,
    pub expiry_preference: ExpiryPreferenceOverride,
    pub confidence: IntentConfidence,
    pub needs_clarification: bool,
    pub clarification_question: Option<String>,
    pub assumptions: Vec<String>,
    pub warnings: Vec<String>,
}