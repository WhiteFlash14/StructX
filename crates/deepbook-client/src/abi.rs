use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    let Some(module) = modules.as_object().and_then(|map| map.get(expected.module)) else {
        return AbiFunctionCheck {
            module: expected.module.to_string(),
            function: expected.function.to_string(),
            status: AbiCheckStatus::Fail,
            visibility: None,
            expected_parameter_count: expected.parameter_count,
            actual_parameter_count: None,
            expected_return_count: expected.return_count,
            actual_return_count: None,
            parameters: vec![],
            returns: vec![],
            source_note: expected.source_note.to_string(),
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
            expected_parameter_count: expected.parameter_count,
            actual_parameter_count: None,
            expected_return_count: expected.return_count,
            actual_return_count: None,
            parameters: vec![],
            returns: vec![],
            source_note: expected.source_note.to_string(),
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

    let parameter_count_ok = parameters.len() == expected.parameter_count;
    let return_count_ok = returns.len() == expected.return_count;

    let visibility_ok =
        visibility.as_deref().map(|value| value.eq_ignore_ascii_case("public")).unwrap_or(true);

    let mut failures = Vec::new();

    if !parameter_count_ok {
        failures.push(format!(
            "parameter count mismatch: expected {}, got {}",
            expected.parameter_count,
            parameters.len()
        ));
    }

    if !return_count_ok {
        failures.push(format!(
            "return count mismatch: expected {}, got {}",
            expected.return_count,
            returns.len()
        ));
    }

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
        expected_parameter_count: expected.parameter_count,
        actual_parameter_count: Some(parameters.len()),
        expected_return_count: expected.return_count,
        actual_return_count: Some(returns.len()),
        parameters,
        returns,
        source_note: expected.source_note.to_string(),
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
    fn verifies_expected_predict_abi_shape() {
        let modules = serde_json::json!({
            "predict": {
                "exposedFunctions": {
                    "get_trade_amounts": {
                        "visibility": "Public",
                        "parameters": [1, 2, 3, 4, 5],
                        "return": [1, 2]
                    },
                    "get_range_trade_amounts": {
                        "visibility": "Public",
                        "parameters": [1, 2, 3, 4, 5],
                        "return": [1, 2]
                    }
                }
            },
            "market_key": {
                "exposedFunctions": {
                    "up": {
                        "visibility": "Public",
                        "parameters": [1, 2, 3],
                        "return": [1]
                    },
                    "down": {
                        "visibility": "Public",
                        "parameters": [1, 2, 3],
                        "return": [1]
                    }
                }
            },
            "range_key": {
                "exposedFunctions": {
                    "new": {
                        "visibility": "Public",
                        "parameters": [1, 2, 3, 4],
                        "return": [1]
                    }
                }
            }
        });

        let report = verify_predict_abi("0xpackage", &modules);

        assert!(report.is_pass());
        assert_eq!(report.checks.len(), 5);
    }

    #[test]
    fn detects_missing_abi_function() {
        let modules = serde_json::json!({
            "predict": {
                "exposedFunctions": {}
            }
        });

        let report = verify_predict_abi("0xpackage", &modules);

        assert!(!report.is_pass());
        assert!(report
            .checks
            .iter()
            .any(|check| check.function == "get_trade_amounts"
                && check.status == AbiCheckStatus::Fail));
    }

    #[test]
    fn detects_parameter_count_mismatch() {
        let modules = serde_json::json!({
            "predict": {
                "exposedFunctions": {
                    "get_trade_amounts": {
                        "visibility": "Public",
                        "parameters": [1, 2],
                        "return": [1, 2]
                    }
                }
            },
            "market_key": {
                "exposedFunctions": {
                    "up": {"visibility": "Public", "parameters": [1,2,3], "return": [1]},
                    "down": {"visibility": "Public", "parameters": [1,2,3], "return": [1]}
                }
            },
            "range_key": {
                "exposedFunctions": {
                    "new": {"visibility": "Public", "parameters": [1,2,3,4], "return": [1]}
                }
            }
        });

        let report = verify_predict_abi("0xpackage", &modules);

        let check = report
            .checks
            .iter()
            .find(|check| check.function == "get_trade_amounts")
            .expect("check exists");

        assert_eq!(check.status, AbiCheckStatus::Fail);
        assert_eq!(check.actual_parameter_count, Some(2));
    }
}
