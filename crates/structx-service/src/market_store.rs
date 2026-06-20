use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use tokio::fs;

use crate::market_catalog::{
    now_ms, CatalogMarketSnapshot, ExpiryPreference, MarketCatalog, MarketSearchQuery,
};

#[async_trait]
pub trait MarketStore: Send + Sync {
    async fn save_catalog(&self, catalog: &MarketCatalog) -> anyhow::Result<()>;

    async fn load_latest_catalog(&self) -> anyhow::Result<Option<MarketCatalog>>;

    async fn get_market(&self, market_id: &str) -> anyhow::Result<Option<CatalogMarketSnapshot>>;

    async fn search_markets(
        &self,
        query: MarketSearchQuery,
    ) -> anyhow::Result<Vec<CatalogMarketSnapshot>>;
}

#[derive(Debug, Clone)]
pub struct DiskMarketStore {
    root_dir: PathBuf,
}

impl DiskMarketStore {
    pub fn new(root_dir: impl Into<PathBuf>) -> Self {
        Self {
            root_dir: root_dir.into(),
        }
    }

    pub fn default_state_dir() -> Self {
        Self::new("artifacts/structx_state/markets")
    }

    fn latest_path(&self) -> PathBuf {
        self.root_dir.join("latest.json")
    }

    fn snapshots_dir(&self) -> PathBuf {
        self.root_dir.join("snapshots")
    }

    fn snapshot_path(&self, refreshed_at_ms: u64) -> PathBuf {
        self.snapshots_dir()
            .join(format!("catalog_{refreshed_at_ms}.json"))
    }

    async fn ensure_dirs(&self) -> anyhow::Result<()> {
        fs::create_dir_all(&self.root_dir)
            .await
            .with_context(|| format!("failed to create market dir {}", self.root_dir.display()))?;
        fs::create_dir_all(self.snapshots_dir())
            .await
            .context("failed to create market snapshots dir")?;
        Ok(())
    }

    async fn atomic_write_json<T: serde::Serialize>(
        &self,
        path: &Path,
        value: &T,
    ) -> anyhow::Result<()> {
        let parent = path
            .parent()
            .ok_or_else(|| anyhow!("path has no parent: {}", path.display()))?;
        fs::create_dir_all(parent).await?;

        let tmp_path = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(value)?;
        fs::write(&tmp_path, bytes)
            .await
            .with_context(|| format!("failed to write temp file {}", tmp_path.display()))?;

        let file = std::fs::OpenOptions::new()
            .read(true)
            .open(&tmp_path)
            .with_context(|| format!("failed to open temp file {}", tmp_path.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to fsync temp file {}", tmp_path.display()))?;

        fs::rename(&tmp_path, path).await.with_context(|| {
            format!(
                "failed to rename {} -> {}",
                tmp_path.display(),
                path.display()
            )
        })?;
        Ok(())
    }
}

#[async_trait]
impl MarketStore for DiskMarketStore {
    async fn save_catalog(&self, catalog: &MarketCatalog) -> anyhow::Result<()> {
        self.ensure_dirs().await?;

        let latest_path = self.latest_path();
        let snapshot_path = self.snapshot_path(catalog.last_refreshed_at_ms);

        self.atomic_write_json(&latest_path, catalog).await?;
        self.atomic_write_json(&snapshot_path, catalog).await?;
        Ok(())
    }

    async fn load_latest_catalog(&self) -> anyhow::Result<Option<MarketCatalog>> {
        let path = self.latest_path();
        if !path.exists() {
            return Ok(None);
        }

        let bytes = fs::read(&path)
            .await
            .with_context(|| format!("failed to read market catalog {}", path.display()))?;
        let catalog: MarketCatalog = serde_json::from_slice(&bytes)
            .with_context(|| format!("failed to decode market catalog {}", path.display()))?;
        Ok(Some(catalog))
    }

    async fn get_market(&self, market_id: &str) -> anyhow::Result<Option<CatalogMarketSnapshot>> {
        let Some(catalog) = self.load_latest_catalog().await? else {
            return Ok(None);
        };
        let now_ms = now_ms();

        Ok(catalog
            .markets
            .into_iter()
            .find(|m| m.market_id == market_id || m.oracle_id == market_id)
            .map(|market| market.with_status_at(now_ms)))
    }

    async fn search_markets(
        &self,
        query: MarketSearchQuery,
    ) -> anyhow::Result<Vec<CatalogMarketSnapshot>> {
        let Some(catalog) = self.load_latest_catalog().await? else {
            return Ok(vec![]);
        };
        let now_ms = now_ms();

        let mut scored: Vec<(i64, CatalogMarketSnapshot)> = catalog
            .markets
            .into_iter()
            .map(|market| market.with_status_at(now_ms))
            .filter(|m| {
                if query.require_active && !m.is_active() {
                    return false;
                }
                if let Some(ref quote_asset) = query.quote_asset {
                    if !m.supports_quote_asset(quote_asset) {
                        return false;
                    }
                }
                if let Some(ref category) = query.category_hint {
                    if &m.category != category {
                        return false;
                    }
                }
                if let Some(ref kind) = query.market_kind_hint {
                    if &m.market_kind != kind {
                        return false;
                    }
                }
                true
            })
            .map(|m| {
                let score = score_market(&m, &query);
                (score, m)
            })
            .filter(|(score, _)| *score > 0)
            .collect();

        scored.sort_by(|a, b| {
            b.0.cmp(&a.0).then_with(|| {
                let a_expiry = a.1.expiry_ms;
                let b_expiry = b.1.expiry_ms;

                match query.expiry_preference {
                    Some(ExpiryPreference::Latest) => b_expiry.cmp(&a_expiry),
                    _ => a_expiry.cmp(&b_expiry),
                }
            })
        });

        Ok(scored.into_iter().map(|(_, market)| market).collect())
    }
}

fn score_market(market: &CatalogMarketSnapshot, query: &MarketSearchQuery) -> i64 {
    let q = normalize_query(&query.text);
    if q.is_empty() {
        return 1;
    }

    let searchable = market.searchable_text();
    let mut score = 0_i64;

    for alias in aliases_for(&q) {
        if market.underlying.eq_ignore_ascii_case(alias) {
            score += 100;
        }
        if searchable.contains(alias) {
            score += 40;
        }
    }

    if searchable.contains(&q) {
        score += 50;
    }

    for token in q.split_whitespace() {
        if token.len() >= 2 && searchable.contains(token) {
            score += 10;
        }
    }

    if market.is_active() {
        score += 10;
    }

    if let Some(ref quote_asset) = query.quote_asset {
        if market.supports_quote_asset(quote_asset) {
            score += 8;
        }
    }

    score
}

fn normalize_query(input: &str) -> String {
    input
        .trim()
        .to_ascii_lowercase()
        .replace(['-', '_', '/'], " ")
}

fn aliases_for(query: &str) -> Vec<&'static str> {
    match query.trim().to_ascii_lowercase().as_str() {
        "btc" | "bitcoin" => vec!["btc", "bitcoin"],
        "eth" | "ethereum" => vec!["eth", "ethereum"],
        "sui" => vec!["sui"],
        "sol" | "solana" => vec!["sol", "solana"],
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market_catalog::{now_ms, MarketCatalogSource, MarketCategory, MarketKind, MarketStatus};

    fn btc_market() -> CatalogMarketSnapshot {
        CatalogMarketSnapshot {
            market_id: "btc-test-market".to_string(),
            oracle_id: "0xoraclebtc".to_string(),
            underlying: "BTC".to_string(),
            display_name: "BTC Weekly Expiry".to_string(),
            category: MarketCategory::Crypto,
            market_kind: MarketKind::ScalarPrice,
            expiry_ms: 1_900_000_000_000,
            status: MarketStatus::Active,
            spot: Some(100_000.0),
            settlement_price: None,
            valid_strikes: vec![90_000_000_000_000, 100_000_000_000_000, 110_000_000_000_000],
            min_strike: Some(90_000_000_000_000),
            max_strike: Some(110_000_000_000_000),
            quote_assets: vec!["DUSDC".to_string()],
            preferred_quote_asset: "DUSDC".to_string(),
            latest_price_updated_at_ms: Some(1_800_000_000_000),
            svi_updated_at_ms: Some(1_800_000_000_000),
            fetched_at_ms: 1_800_000_000_000,
            tags: vec!["bitcoin".to_string(), "crypto".to_string()],
            metadata: serde_json::json!({}),
        }
    }

    #[tokio::test]
    async fn disk_store_saves_loads_and_searches_catalog() {
        let temp = tempfile::tempdir().unwrap();
        let store = DiskMarketStore::new(temp.path());

        let catalog = MarketCatalog {
            schema_version: MarketCatalog::SCHEMA_VERSION,
            markets: vec![btc_market()],
            last_refreshed_at_ms: now_ms(),
            source: MarketCatalogSource::PredictServer,
        };

        store.save_catalog(&catalog).await.unwrap();

        let loaded = store.load_latest_catalog().await.unwrap().unwrap();
        assert_eq!(loaded.markets.len(), 1);

        let result = store
            .search_markets(MarketSearchQuery {
                text: "bitcoin".to_string(),
                category_hint: Some(MarketCategory::Crypto),
                market_kind_hint: Some(MarketKind::ScalarPrice),
                require_active: true,
                quote_asset: Some("DUSDC".to_string()),
                expiry_preference: Some(ExpiryPreference::NearestActive),
            })
            .await
            .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].underlying, "BTC");
    }

    #[tokio::test]
    async fn search_treats_stale_active_market_as_expired() {
        let temp = tempfile::tempdir().unwrap();
        let store = DiskMarketStore::new(temp.path());
        let mut market = btc_market();
        market.expiry_ms = now_ms().saturating_sub(1_000);

        let catalog = MarketCatalog {
            schema_version: MarketCatalog::SCHEMA_VERSION,
            markets: vec![market],
            last_refreshed_at_ms: now_ms(),
            source: MarketCatalogSource::PredictServer,
        };

        store.save_catalog(&catalog).await.unwrap();

        let result = store
            .search_markets(MarketSearchQuery {
                text: "bitcoin".to_string(),
                category_hint: Some(MarketCategory::Crypto),
                market_kind_hint: Some(MarketKind::ScalarPrice),
                require_active: true,
                quote_asset: Some("DUSDC".to_string()),
                expiry_preference: Some(ExpiryPreference::NearestActive),
            })
            .await
            .unwrap();

        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn get_market_returns_effective_expired_status() {
        let temp = tempfile::tempdir().unwrap();
        let store = DiskMarketStore::new(temp.path());
        let mut market = btc_market();
        market.expiry_ms = now_ms().saturating_sub(1_000);
        let market_id = market.market_id.clone();

        let catalog = MarketCatalog {
            schema_version: MarketCatalog::SCHEMA_VERSION,
            markets: vec![market],
            last_refreshed_at_ms: now_ms(),
            source: MarketCatalogSource::PredictServer,
        };

        store.save_catalog(&catalog).await.unwrap();

        let loaded = store.get_market(&market_id).await.unwrap().unwrap();

        assert_eq!(loaded.status, MarketStatus::ExpiredUnknown);
    }
}