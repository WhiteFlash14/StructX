use std::time::Duration as StdDuration;

use chrono::Utc;
use reqwest::Url;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::constants::{PREDICT_OBJECT_ID, PREDICT_SERVER_URL};
use crate::error::{DeepBookClientError, Result};
use crate::market::{FreshnessConfig, MarketEvaluationContext, MarketSnapshot};
use crate::models::{
    parse_oracle_list_from_value, parse_quote_assets_from_value, AskBounds, LatestPrice, LatestSvi,
    OracleListItem, OracleState, PredictState, QuoteAsset, ServerStatus, VaultSummary,
};

#[derive(Debug, Clone)]
pub struct DeepBookConfig {
    pub server_url: String,
    pub predict_id: String,
    pub request_timeout: StdDuration,
}

impl Default for DeepBookConfig {
    fn default() -> Self {
        Self {
            server_url: PREDICT_SERVER_URL.to_string(),
            predict_id: PREDICT_OBJECT_ID.to_string(),
            request_timeout: StdDuration::from_secs(15),
        }
    }
}

#[derive(Clone)]
pub struct DeepBookClient {
    http: reqwest::Client,
    config: DeepBookConfig,
}

impl DeepBookClient {
    pub fn new(config: DeepBookConfig) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent("structx-deepbook-client/0.1")
            .timeout(config.request_timeout)
            .build()?;

        Ok(Self { http, config })
    }

    #[must_use]
    pub fn config(&self) -> &DeepBookConfig {
        &self.config
    }

    pub fn endpoint_url(&self, path: &str) -> Result<Url> {
        let base = self.config.server_url.trim_end_matches('/');
        let path = path.trim_start_matches('/');
        let full = format!("{base}/{path}");

        Url::parse(&full).map_err(|err| DeepBookClientError::InvalidUrl(err.to_string()))
    }

    pub async fn status(&self) -> Result<ServerStatus> {
        self.get_json("/status").await
    }

    pub async fn predict_state(&self) -> Result<PredictState> {
        self.get_json(&format!("/predicts/{}/state", self.config.predict_id)).await
    }

    pub async fn quote_assets(&self) -> Result<Vec<QuoteAsset>> {
        let value =
            self.get_value(&format!("/predicts/{}/quote-assets", self.config.predict_id)).await?;

        parse_quote_assets_from_value(value).map_err(|source| DeepBookClientError::Decode {
            endpoint: "quote-assets".to_string(),
            source,
        })
    }

    pub async fn oracle_list(&self) -> Result<Vec<OracleListItem>> {
        let value =
            self.get_value(&format!("/predicts/{}/oracles", self.config.predict_id)).await?;

        parse_oracle_list_from_value(value).map_err(|source| DeepBookClientError::Decode {
            endpoint: "oracles".to_string(),
            source,
        })
    }

    pub async fn vault_summary(&self) -> Result<VaultSummary> {
        self.get_json(&format!("/predicts/{}/vault/summary", self.config.predict_id)).await
    }

    pub async fn oracle_state(&self, oracle_id: &str) -> Result<OracleState> {
        let value =
            self.get_value(&format!("/oracles/{}/state", encode_path_segment(oracle_id))).await?;

        Ok(OracleState::from_value(value))
    }

    pub async fn oracle_ask_bounds(&self, oracle_id: &str) -> Result<AskBounds> {
        let value = self
            .get_value(&format!("/oracles/{}/ask-bounds", encode_path_segment(oracle_id)))
            .await?;

        Ok(AskBounds::from_value(value))
    }

    pub async fn oracle_latest_price(&self, oracle_id: &str) -> Result<LatestPrice> {
        let value = self
            .get_value(&format!("/oracles/{}/prices/latest", encode_path_segment(oracle_id)))
            .await?;

        Ok(LatestPrice::from_value(value))
    }

    pub async fn oracle_latest_svi(&self, oracle_id: &str) -> Result<LatestSvi> {
        let value = self
            .get_value(&format!("/oracles/{}/svi/latest", encode_path_segment(oracle_id)))
            .await?;

        Ok(LatestSvi::from_value(value))
    }

    pub async fn load_structx_markets(
        &self,
        freshness: FreshnessConfig,
    ) -> Result<Vec<MarketSnapshot>> {
        self.load_markets_filtered(freshness, |oracle| {
            oracle.is_btc() && oracle.is_active_or_live()
        })
        .await
    }

    pub async fn load_market_directory(
        &self,
        freshness: FreshnessConfig,
    ) -> Result<Vec<MarketSnapshot>> {
        self.load_markets_filtered(freshness, OracleListItem::is_active_or_live).await
    }

    async fn load_markets_filtered<F>(
        &self,
        freshness: FreshnessConfig,
        mut predicate: F,
    ) -> Result<Vec<MarketSnapshot>>
    where
        F: FnMut(&OracleListItem) -> bool,
    {
        let vault_available = self.vault_summary().await.map(|v| v.is_present()).unwrap_or(false);

        let oracles = self.oracle_list().await?;
        let now = Utc::now();

        let context =
            MarketEvaluationContext { vault_summary_available: vault_available, now, freshness };

        let selected_oracles =
            oracles.into_iter().filter(|oracle| predicate(oracle)).collect::<Vec<_>>();

        let mut snapshots = Vec::with_capacity(selected_oracles.len());

        for oracle in selected_oracles {
            let Some(oracle_id) = oracle.oracle_id.clone() else {
                snapshots.push(MarketSnapshot::evaluate(oracle, None, None, None, None, context));
                continue;
            };

            let (state, latest_price, latest_svi, ask_bounds) = tokio::join!(
                self.oracle_state(&oracle_id),
                self.oracle_latest_price(&oracle_id),
                self.oracle_latest_svi(&oracle_id),
                self.oracle_ask_bounds(&oracle_id),
            );

            snapshots.push(MarketSnapshot::evaluate(
                oracle,
                state.ok(),
                latest_price.ok(),
                latest_svi.ok(),
                ask_bounds.ok(),
                context,
            ));
        }

        Ok(snapshots)
    }

    async fn get_json<T>(&self, path: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let value = self.get_value(path).await?;

        serde_json::from_value(value)
            .map_err(|source| DeepBookClientError::Decode { endpoint: path.to_string(), source })
    }

    async fn get_value(&self, path: &str) -> Result<Value> {
        let url = self.endpoint_url(path)?;
        let response = self.http.get(url.clone()).send().await?;
        let status = response.status();

        if !status.is_success() {
            let body = response.text().await.unwrap_or_else(|_| String::new());

            return Err(DeepBookClientError::HttpStatus { status, body });
        }

        response.json::<Value>().await.map_err(DeepBookClientError::Request)
    }
}

fn encode_path_segment(value: &str) -> String {
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructs_endpoint_urls_without_double_slashes() {
        let client = DeepBookClient::new(DeepBookConfig {
            server_url: "https://example.com/base/".to_string(),
            predict_id: "0xpredict".to_string(),
            request_timeout: StdDuration::from_secs(15),
        })
        .expect("client builds");

        let url = client.endpoint_url("/status").expect("url builds");

        assert_eq!(url.as_str(), "https://example.com/base/status");
    }

    #[test]
    fn constructs_predict_state_url() {
        let client = DeepBookClient::new(DeepBookConfig {
            server_url: "https://example.com".to_string(),
            predict_id: "0xpredict".to_string(),
            request_timeout: StdDuration::from_secs(15),
        })
        .expect("client builds");

        let url = client
            .endpoint_url(&format!("/predicts/{}/state", client.config().predict_id))
            .expect("url builds");

        assert_eq!(url.as_str(), "https://example.com/predicts/0xpredict/state");
    }
}
