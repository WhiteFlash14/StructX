use std::time::Duration;

use reqwest::Url;
use serde_json::{json, Value};

use crate::error::{DeepBookClientError, Result};

#[derive(Clone)]
pub struct SuiRpcClient {
    http: reqwest::Client,
    rpc_url: String,
}

impl SuiRpcClient {
    pub fn new(rpc_url: impl Into<String>, request_timeout: Duration) -> Result<Self> {
        let rpc_url = rpc_url.into();

        Url::parse(&rpc_url).map_err(|err| DeepBookClientError::InvalidUrl(err.to_string()))?;

        let http = reqwest::Client::builder()
            .user_agent("structx-sui-rpc-client/0.1")
            .timeout(request_timeout)
            .build()?;

        Ok(Self { http, rpc_url })
    }

    pub async fn get_normalized_move_modules_by_package(&self, package_id: &str) -> Result<Value> {
        self.call("sui_getNormalizedMoveModulesByPackage", json!([package_id])).await
    }

    pub async fn get_object(&self, object_id: &str) -> Result<Value> {
        self.call(
            "sui_getObject",
            json!([
                object_id,
                {
                    "showType": true,
                    "showOwner": true,
                    "showPreviousTransaction": false,
                    "showDisplay": false,
                    "showContent": false,
                    "showBcs": false,
                    "showStorageRebate": false
                }
            ]),
        )
        .await
    }

    pub async fn dev_inspect_transaction_kind(
        &self,
        sender: &str,
        tx_kind_b64: &str,
    ) -> Result<Value> {
        self.call("sui_devInspectTransactionBlock", json!([sender, tx_kind_b64])).await
    }

    async fn call(&self, method: &str, params: Value) -> Result<Value> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });

        let response = self.http.post(&self.rpc_url).json(&body).send().await?;
        let status = response.status();

        if !status.is_success() {
            let body = response.text().await.unwrap_or_else(|_| String::new());
            return Err(DeepBookClientError::HttpStatus { status, body });
        }

        let payload = response.json::<Value>().await.map_err(DeepBookClientError::Request)?;

        if let Some(error) = payload.get("error") {
            return Err(DeepBookClientError::UnexpectedShape {
                endpoint: method.to_string(),
                message: format!("RPC error: {error}"),
            });
        }

        payload.get("result").cloned().ok_or_else(|| DeepBookClientError::UnexpectedShape {
            endpoint: method.to_string(),
            message: "missing RPC result field".to_string(),
        })
    }
}
