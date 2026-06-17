use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy)]
pub struct ExpectedAbiFunction {
    pub module: &'static str,
    pub function: &'static str,
    pub expected_parameters: &'static [&'static str],
    pub expected_returns: &'static [&'static str],
    pub source_note: &'static str,
    pub source_url: &'static str,
}

#[cfg(test)]
const PREDICT_PACKAGE: &str = "0xf5ea2b3749c65d6e56507cc35388719aadb28f9cab873696a2f8687f5c785138";

const PREDICT_REF: &str =
    "{\"Reference\":{\"Struct\":{\"address\":\"0xf5ea2b3749c65d6e56507cc35388719aadb28f9cab873696a2f8687f5c785138\",\"module\":\"predict\",\"name\":\"Predict\",\"typeArguments\":[]}}}";

const ORACLE_SVI_REF: &str =
    "{\"Reference\":{\"Struct\":{\"address\":\"0xf5ea2b3749c65d6e56507cc35388719aadb28f9cab873696a2f8687f5c785138\",\"module\":\"oracle\",\"name\":\"OracleSVI\",\"typeArguments\":[]}}}";

const MARKET_KEY: &str =
    "{\"Struct\":{\"address\":\"0xf5ea2b3749c65d6e56507cc35388719aadb28f9cab873696a2f8687f5c785138\",\"module\":\"market_key\",\"name\":\"MarketKey\",\"typeArguments\":[]}}";

const RANGE_KEY: &str =
    "{\"Struct\":{\"address\":\"0xf5ea2b3749c65d6e56507cc35388719aadb28f9cab873696a2f8687f5c785138\",\"module\":\"range_key\",\"name\":\"RangeKey\",\"typeArguments\":[]}}";

const OBJECT_ID: &str =
    "{\"Struct\":{\"address\":\"0x2\",\"module\":\"object\",\"name\":\"ID\",\"typeArguments\":[]}}";

const CLOCK_REF: &str =
    "{\"Reference\":{\"Struct\":{\"address\":\"0x2\",\"module\":\"clock\",\"name\":\"Clock\",\"typeArguments\":[]}}}";

const TX_CONTEXT_MUT_REF: &str =
    "{\"MutableReference\":{\"Struct\":{\"address\":\"0x2\",\"module\":\"tx_context\",\"name\":\"TxContext\",\"typeArguments\":[]}}}";

pub const REQUIRED_PREDICT_ABI: &[ExpectedAbiFunction] = &[
    ExpectedAbiFunction {
        module: "predict",
        function: "get_trade_amounts",
        expected_parameters: &[PREDICT_REF, ORACLE_SVI_REF, MARKET_KEY, "U64", CLOCK_REF],
        expected_returns: &["U64", "U64"],
        source_note: "predict.move + official Predict docs",
        source_url: "https://raw.githubusercontent.com/MystenLabs/deepbookv3/predict-testnet-4-16/packages/predict/sources/predict.move",
    },
    ExpectedAbiFunction {
        module: "predict",
        function: "get_range_trade_amounts",
        expected_parameters: &[PREDICT_REF, ORACLE_SVI_REF, RANGE_KEY, "U64", CLOCK_REF],
        expected_returns: &["U64", "U64"],
        source_note: "predict.move + official Predict docs",
        source_url: "https://raw.githubusercontent.com/MystenLabs/deepbookv3/predict-testnet-4-16/packages/predict/sources/predict.move",
    },
    
    ExpectedAbiFunction {
        module: "predict",
        function: "create_manager",
        expected_parameters: &[TX_CONTEXT_MUT_REF],
        expected_returns: &[OBJECT_ID],
        source_note: "predict.move + official Predict docs",
        source_url: "https://raw.githubusercontent.com/MystenLabs/deepbookv3/predict-testnet-4-16/packages/predict/sources/predict.move",
    },
ExpectedAbiFunction {
        module: "market_key",
        function: "up",
        expected_parameters: &[OBJECT_ID, "U64", "U64"],
        expected_returns: &[MARKET_KEY],
        source_note: "official Market Keys docs + live ABI",
        source_url: "https://docs.sui.io/onchain-finance/deepbook-predict/contract-information/market-keys",
    },
    ExpectedAbiFunction {
        module: "market_key",
        function: "down",
        expected_parameters: &[OBJECT_ID, "U64", "U64"],
        expected_returns: &[MARKET_KEY],
        source_note: "official Market Keys docs + live ABI",
        source_url: "https://docs.sui.io/onchain-finance/deepbook-predict/contract-information/market-keys",
    },
    ExpectedAbiFunction {
        module: "range_key",
        function: "new",
        expected_parameters: &[OBJECT_ID, "U64", "U64", "U64"],
        expected_returns: &[RANGE_KEY],
        source_note: "official Market Keys docs + live ABI",
        source_url: "https://docs.sui.io/onchain-finance/deepbook-predict/contract-information/market-keys",
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

    pub expected_parameters: Vec<String>,
    pub expected_returns: Vec<String>,

    pub source_note: String,
    pub source_url: String,
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

pub fn verify_predict_abi(package_id: impl Into<String>, modules: &Value) -> AbiVerificationReport {
    let package_id = package_id.into();
    let module_count = modules.as_object().map(|map| map.len()).unwrap_or(0);

    let checks = REQUIRED_PREDICT_ABI
        .iter()
        .map(|expected| verify_function(modules, expected))
        .collect::<Vec<_>>();

    AbiVerificationReport { package_id, module_count, checks }
}

fn verify_function(modules: &Value, expected: &ExpectedAbiFunction) -> AbiFunctionCheck {
    let expected_parameters =
        expected.expected_parameters.iter().map(|value| (*value).to_string()).collect::<Vec<_>>();

    let expected_returns =
        expected.expected_returns.iter().map(|value| (*value).to_string()).collect::<Vec<_>>();

    let Some(module) = modules.as_object().and_then(|map| map.get(expected.module)) else {
        return AbiFunctionCheck {
            module: expected.module.to_string(),
            function: expected.function.to_string(),
            status: AbiCheckStatus::Fail,
            visibility: None,
            expected_parameter_count: expected.expected_parameters.len(),
            actual_parameter_count: None,
            expected_return_count: expected.expected_returns.len(),
            actual_return_count: None,
            parameters: vec![],
            returns: vec![],
            expected_parameters,
            expected_returns,
            source_note: expected.source_note.to_string(),
            source_url: expected.source_url.to_string(),
            message: Some(format!("missing module `{}`", expected.module)),
        };
    };

    let Some(function) = module
        .get("exposedFunctions")
        .and_then(Value::as_object)
        .and_then(|functions| functions.get(expected.function))
    else {
        return AbiFunctionCheck {
            module: expected.module.to_string(),
            function: expected.function.to_string(),
            status: AbiCheckStatus::Fail,
            visibility: None,
            expected_parameter_count: expected.expected_parameters.len(),
            actual_parameter_count: None,
            expected_return_count: expected.expected_returns.len(),
            actual_return_count: None,
            parameters: vec![],
            returns: vec![],
            expected_parameters,
            expected_returns,
            source_note: expected.source_note.to_string(),
            source_url: expected.source_url.to_string(),
            message: Some(format!("missing function `{}::{}`", expected.module, expected.function)),
        };
    };

    let parameters = function
        .get("parameters")
        .and_then(Value::as_array)
        .map(|items| items.iter().map(type_to_string).collect::<Vec<_>>())
        .unwrap_or_default();

    let returns = function
        .get("return")
        .or_else(|| function.get("returns"))
        .and_then(Value::as_array)
        .map(|items| items.iter().map(type_to_string).collect::<Vec<_>>())
        .unwrap_or_default();

    let visibility = function.get("visibility").and_then(Value::as_str).map(ToString::to_string);

    let mut failures = Vec::new();

    if parameters.len() != expected.expected_parameters.len() {
        failures.push(format!(
            "parameter count mismatch: expected {}, got {}",
            expected.expected_parameters.len(),
            parameters.len()
        ));
    }

    if returns.len() != expected.expected_returns.len() {
        failures.push(format!(
            "return count mismatch: expected {}, got {}",
            expected.expected_returns.len(),
            returns.len()
        ));
    }

    for (idx, (actual, expected_type)) in
        parameters.iter().zip(expected.expected_parameters.iter()).enumerate()
    {
        if actual != expected_type {
            failures.push(format!(
                "parameter[{idx}] type mismatch: expected {expected_type}, got {actual}"
            ));
        }
    }

    for (idx, (actual, expected_type)) in
        returns.iter().zip(expected.expected_returns.iter()).enumerate()
    {
        if actual != expected_type {
            failures.push(format!(
                "return[{idx}] type mismatch: expected {expected_type}, got {actual}"
            ));
        }
    }

    let visibility_ok =
        visibility.as_deref().map(|value| value.eq_ignore_ascii_case("public")).unwrap_or(true);

    if !visibility_ok {
        failures.push(format!(
            "visibility mismatch: expected public, got {}",
            visibility.as_deref().unwrap_or("unknown")
        ));
    }

    AbiFunctionCheck {
        module: expected.module.to_string(),
        function: expected.function.to_string(),
        status: if failures.is_empty() { AbiCheckStatus::Pass } else { AbiCheckStatus::Fail },
        visibility,
        expected_parameter_count: expected.expected_parameters.len(),
        actual_parameter_count: Some(parameters.len()),
        expected_return_count: expected.expected_returns.len(),
        actual_return_count: Some(returns.len()),
        parameters,
        returns,
        expected_parameters,
        expected_returns,
        source_note: expected.source_note.to_string(),
        source_url: expected.source_url.to_string(),
        message: if failures.is_empty() { None } else { Some(failures.join("; ")) },
    }
}

fn type_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => serde_json::to_string(other).unwrap_or_else(|_| "<unprintable>".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifies_exact_expected_predict_abi_shape() {
        let modules = serde_json::json!({
            "predict": {
                "exposedFunctions": {
                    "get_trade_amounts": {
                        "visibility": "Public",
                        "parameters": [
                            {"Reference":{"Struct":{"address":PREDICT_PACKAGE,"module":"predict","name":"Predict","typeArguments":[]}}},
                            {"Reference":{"Struct":{"address":PREDICT_PACKAGE,"module":"oracle","name":"OracleSVI","typeArguments":[]}}},
                            {"Struct":{"address":PREDICT_PACKAGE,"module":"market_key","name":"MarketKey","typeArguments":[]}},
                            "U64",
                            {"Reference":{"Struct":{"address":"0x2","module":"clock","name":"Clock","typeArguments":[]}}}
                        ],
                        "return": ["U64", "U64"]
                    },
                    "get_range_trade_amounts": {
                        "visibility": "Public",
                        "parameters": [
                            {"Reference":{"Struct":{"address":PREDICT_PACKAGE,"module":"predict","name":"Predict","typeArguments":[]}}},
                            {"Reference":{"Struct":{"address":PREDICT_PACKAGE,"module":"oracle","name":"OracleSVI","typeArguments":[]}}},
                            {"Struct":{"address":PREDICT_PACKAGE,"module":"range_key","name":"RangeKey","typeArguments":[]}},
                            "U64",
                            {"Reference":{"Struct":{"address":"0x2","module":"clock","name":"Clock","typeArguments":[]}}}
                        ],
                        "return": ["U64", "U64"]
                    }
                }
            },
            "market_key": {
                "exposedFunctions": {
                    "up": {
                        "visibility": "Public",
                        "parameters": [
                            {"Struct":{"address":"0x2","module":"object","name":"ID","typeArguments":[]}},
                            "U64",
                            "U64"
                        ],
                        "return": [
                            {"Struct":{"address":PREDICT_PACKAGE,"module":"market_key","name":"MarketKey","typeArguments":[]}}
                        ]
                    },
                    "down": {
                        "visibility": "Public",
                        "parameters": [
                            {"Struct":{"address":"0x2","module":"object","name":"ID","typeArguments":[]}},
                            "U64",
                            "U64"
                        ],
                        "return": [
                            {"Struct":{"address":PREDICT_PACKAGE,"module":"market_key","name":"MarketKey","typeArguments":[]}}
                        ]
                    }
                }
            },
            "range_key": {
                "exposedFunctions": {
                    "new": {
                        "visibility": "Public",
                        "parameters": [
                            {"Struct":{"address":"0x2","module":"object","name":"ID","typeArguments":[]}},
                            "U64",
                            "U64",
                            "U64"
                        ],
                        "return": [
                            {"Struct":{"address":PREDICT_PACKAGE,"module":"range_key","name":"RangeKey","typeArguments":[]}}
                        ]
                    }
                }
            }
        });

        let report = verify_predict_abi(PREDICT_PACKAGE, &modules);

        assert!(report.is_pass());
        assert_eq!(report.checks.len(), 6);
    }

    #[test]
    fn detects_exact_type_mismatch() {
        let modules = serde_json::json!({
            "predict": {
                "exposedFunctions": {
                    "get_trade_amounts": {
                        "visibility": "Public",
                        "parameters": [
                            {"Reference":{"Struct":{"address":PREDICT_PACKAGE,"module":"predict","name":"Predict","typeArguments":[]}}},
                            {"Reference":{"Struct":{"address":PREDICT_PACKAGE,"module":"oracle","name":"OracleSVI","typeArguments":[]}}},
                            "U64",
                            "U64",
                            {"Reference":{"Struct":{"address":"0x2","module":"clock","name":"Clock","typeArguments":[]}}}
                        ],
                        "return": ["U64", "U64"]
                    }
                }
            },
            "market_key": {
                "exposedFunctions": {
                    "up": {"visibility":"Public","parameters":[{"Struct":{"address":"0x2","module":"object","name":"ID","typeArguments":[]}},"U64","U64"],"return":[{"Struct":{"address":PREDICT_PACKAGE,"module":"market_key","name":"MarketKey","typeArguments":[]}}]},
                    "down": {"visibility":"Public","parameters":[{"Struct":{"address":"0x2","module":"object","name":"ID","typeArguments":[]}},"U64","U64"],"return":[{"Struct":{"address":PREDICT_PACKAGE,"module":"market_key","name":"MarketKey","typeArguments":[]}}]}
                }
            },
            "range_key": {
                "exposedFunctions": {
                    "new": {"visibility":"Public","parameters":[{"Struct":{"address":"0x2","module":"object","name":"ID","typeArguments":[]}},"U64","U64","U64"],"return":[{"Struct":{"address":PREDICT_PACKAGE,"module":"range_key","name":"RangeKey","typeArguments":[]}}]}
                }
            }
        });

        let report = verify_predict_abi(PREDICT_PACKAGE, &modules);

        let check = report
            .checks
            .iter()
            .find(|check| check.module == "predict" && check.function == "get_trade_amounts")
            .expect("check exists");

        assert_eq!(check.status, AbiCheckStatus::Fail);
        assert!(check.message.as_deref().unwrap_or("").contains("parameter[2] type mismatch"));
    }
}
