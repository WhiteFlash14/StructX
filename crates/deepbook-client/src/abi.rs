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
