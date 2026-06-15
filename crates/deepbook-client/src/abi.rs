use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy)]
pub struct ExpectedAbiFunction {
    pub module: &'static str,
    pub function: &'static str,
    pub parameter_count: usize,
    pub return_count: usize,
    pub source_note: &'static str,
}

pub const REQUIRED_PREDICT_ABI: &[ExpectedAbiFunction] = &[
    ExpectedAbiFunction {
        module: "predict",
        function: "get_trade_amounts",
        parameter_count: 5,
        return_count: 2,
        source_note: "predict.move + official Predict docs",
    },
    ExpectedAbiFunction {
        module: "predict",
        function: "get_range_trade_amounts",
        parameter_count: 5,
        return_count: 2,
        source_note: "predict.move + official Predict docs",
    },
    ExpectedAbiFunction {
        module: "market_key",
        function: "up",
        parameter_count: 3,
        return_count: 1,
        source_note: "official Market Keys docs",
    },
    ExpectedAbiFunction {
        module: "market_key",
        function: "down",
        parameter_count: 3,
        return_count: 1,
        source_note: "official Market Keys docs",
    },
    ExpectedAbiFunction {
        module: "range_key",
        function: "new",
        parameter_count: 4,
        return_count: 1,
        source_note: "official Market Keys docs",
    },
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AbiCheckStatus {
    Pass,
    Fail,
}

impl std::fmt::Display for AbiCheckStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pass => write!(f, "pass"),
            Self::Fail => write!(f, "fail"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbiFunctionCheck {
    pub module: String,
    pub function: String,
    pub status: AbiCheckStatus,
    pub visibility: Option<String>,
    pub expected_parameter_count: usize,
    pub actual_parameter_count: Option<usize>,
    pub expected_return_count: usize,
    pub actual_return_count: Option<usize>,
    pub parameters: Vec<String>,
    pub returns: Vec<String>,
    pub source_note: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbiVerificationReport {
    pub package_id: String,
    pub module_count: usize,
    pub checks: Vec<AbiFunctionCheck>,
}

impl AbiVerificationReport {
    #[must_use]
    pub fn is_pass(&self) -> bool {
        self.checks.iter().all(|check| check.status == AbiCheckStatus::Pass)
    }
}
