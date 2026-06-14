use deepbook_client::{PREDICT_OBJECT_ID, PREDICT_PACKAGE_ID};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuoteTarget {
    pub package_id: String,
    pub predict_object_id: String,
    pub module: String,
}

impl Default for QuoteTarget {
    fn default() -> Self {
        Self {
            package_id: PREDICT_PACKAGE_ID.to_string(),
            predict_object_id: PREDICT_OBJECT_ID.to_string(),
            module: "predict".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuoteFunction {
    GetTradeAmounts,
    GetRangeTradeAmounts,
}

impl QuoteFunction {
    #[must_use]
    pub fn move_function_name(self) -> &'static str {
        match self {
            Self::GetTradeAmounts => "get_trade_amounts",
            Self::GetRangeTradeAmounts => "get_range_trade_amounts",
        }
    }
}

impl std::fmt::Display for QuoteFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.move_function_name())
    }
}
