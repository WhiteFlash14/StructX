pub use advanced_strategies::{
    allocate_weighted_budget, compile_convex_tail_ladder, compile_downside_convexity,
    compile_expiry_move_note, compile_moonshot_upside, compile_portfolio_crash_shield,
    compile_range_conviction, score_smart_candidate, AdvancedCompileResult, AdvancedCompiledLeg,
    AdvancedLegInput, AdvancedLegKind, AdvancedStrategyError, AdvancedStrategyKind,
    ConvexTailLadderInput, DownsideConvexityInput, ExpiryMoveNoteInput, MoonshotUpsideInput,
    PortfolioCrashShieldInput, RangeConvictionInput, SmartBudgetStyle, SmartCandidateMetrics,
    SmartCandidateScore,
};
pub mod advanced_strategies;
pub use breakout_optimizer::{
    estimate_breakout_premium_raw, optimize_breakout_quantities, BreakoutAskInputs,
    BreakoutOptimizerError, BreakoutStyle, OptimizedBreakoutQuantities,
};
pub mod breakout_optimizer;
pub mod payoff;
pub mod price;
pub mod quote_guard;
pub mod quote_plan;
pub mod quote_preview;
pub mod quote_tx;
pub mod selector;
pub mod strike_grid;

pub use payoff::{
    compile_breakout, compile_bucket_payoff, compile_range_payout, BinaryDirection, CompiledPayoff,
    PayoffBucket, PayoffCompileError, PredictLeg,
};
pub use price::{DisplayPrice, PriceScale};
pub use quote_guard::{guard_quote_preview, GuardedQuotePreview, QuoteCostGuard, QuoteGuardError};
pub use quote_plan::{
    build_quote_plan, QuoteCall, QuoteFunction, QuotePlan, QuotePlanError, QuoteTarget,
};
pub use quote_preview::{format_quote_amount, QuoteAssetDisplay, QuotePreview, QuotePreviewLeg};
pub use quote_tx::{
    build_create_manager_tx_kind, build_manager_balance_tx_kind, build_manager_positions_tx_kind,
    build_mint_tx_kind, build_quote_tx_kind, build_redeem_tx_kind, ManagerPositionRead,
    MintObjectRefs, QuoteObjectRefs, QuoteTxBuildError, QuoteTxKind,
};
pub use selector::{
    select_best_market, select_candidate_markets, MarketSelectionError, SelectedMarket,
};
pub use strike_grid::{Strike, StrikeBucket, StrikeGrid, StrikeGridError};
