use serde::{Deserialize, Serialize};

use crate::intent::{IntentPlan, StrategyTemplateId};
use crate::market_catalog::CatalogMarketSnapshot;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuoteIntentPlanRequest {
    pub user_address: Option<String>,
    pub intent_plan: IntentPlan,
    pub selected_market_id: Option<String>,
    pub budget: Option<u64>,
    pub max_quote_age_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionProposal {
    pub proposal_id: String,
    pub user_address: Option<String>,
    pub raw_prompt: String,
    pub selected_market: CatalogMarketSnapshot,
    pub reason_for_selection: String,
    pub strategy_template: StrategyTemplateId,
    pub backend_strategy_id: String,
    pub legs: Vec<CompiledProposalLeg>,
    pub total_premium: u64,
    pub max_loss: u64,
    pub max_payout: u64,
    pub payoff_table: Vec<PayoffRow>,
    pub net_pnl_table: Vec<PayoffRow>,
    pub quote_metadata: ProposalQuoteMetadata,
    pub assumptions: Vec<String>,
    pub warnings: Vec<String>,
    pub requires_user_signature: bool,
    pub raw_compiled_strategy: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledProposalLeg {
    pub kind: String,
    pub oracle_id: String,
    pub expiry_ms: u64,
    pub strike: Option<u64>,
    pub lower: Option<u64>,
    pub upper: Option<u64>,
    pub quantity: u64,
    pub ask_price: Option<u64>,
    pub premium: Option<u64>,
    pub role: Option<String>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayoffRow {
    pub label: String,
    pub settlement_lower: Option<f64>,
    pub settlement_upper: Option<f64>,
    pub gross_payout: u64,
    pub net_pnl: i128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalQuoteMetadata {
    pub quote_batch_id: String,
    pub quoted_at_ms: u64,
    pub max_quote_age_ms: u64,
    pub source: String,
    pub oracle_id: String,
    pub market_fetched_at_ms: u64,
}