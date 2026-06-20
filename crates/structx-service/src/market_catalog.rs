use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MarketCatalogSource {
    PredictServer,
    DiskCache,
    Mixed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum MarketKind {
    ScalarPrice,
    ScalarEvent,
    BinaryEvent,
    CategoricalEvent,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum MarketCategory {
    Crypto,
    Finance,
    Sports,
    Politics,
    Macro,
    Weather,
    Other,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum MarketStatus {
    Inactive,
    Active,
    PendingSettlement,
    Settled,
    ExpiredUnknown,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExpiryPreference {
    NearestActive,
    Soonest,
    Latest,
    Any,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MarketCatalog {
    pub schema_version: u32,
    pub markets: Vec<CatalogMarketSnapshot>,
    pub last_refreshed_at_ms: u64,
    pub source: MarketCatalogSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CatalogMarketSnapshot {
    pub market_id: String,
    pub oracle_id: String,
    pub underlying: String,
    pub display_name: String,
    pub category: MarketCategory,
    pub market_kind: MarketKind,
    pub expiry_ms: u64,
    pub status: MarketStatus,
    pub spot: Option<f64>,
    pub settlement_price: Option<f64>,
    pub valid_strikes: Vec<u64>,
    pub min_strike: Option<u64>,
    pub max_strike: Option<u64>,
    pub quote_assets: Vec<String>,
    pub preferred_quote_asset: String,
    pub latest_price_updated_at_ms: Option<u64>,
    pub svi_updated_at_ms: Option<u64>,
    pub fetched_at_ms: u64,
    pub tags: Vec<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MarketSearchQuery {
    pub text: String,
    pub category_hint: Option<MarketCategory>,
    pub market_kind_hint: Option<MarketKind>,
    pub require_active: bool,
    pub quote_asset: Option<String>,
    pub expiry_preference: Option<ExpiryPreference>,
}

impl MarketCatalog {
    pub const SCHEMA_VERSION: u32 = 1;

    pub fn new(markets: Vec<CatalogMarketSnapshot>, now_ms: u64) -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            markets,
            last_refreshed_at_ms: now_ms,
            source: MarketCatalogSource::PredictServer,
        }
    }

    pub fn is_stale(&self, now_ms: u64, max_staleness_ms: u64) -> bool {
        now_ms.saturating_sub(self.last_refreshed_at_ms) > max_staleness_ms
    }
}

impl CatalogMarketSnapshot {
    pub fn is_active(&self) -> bool {
        self.status == MarketStatus::Active
    }

    pub fn status_at(&self, now_ms: u64) -> MarketStatus {
        if self.expiry_ms == 0 {
            return self.status.clone();
        }

        match self.status {
            MarketStatus::Active if self.expiry_ms <= now_ms => MarketStatus::ExpiredUnknown,
            MarketStatus::ExpiredUnknown if self.expiry_ms > now_ms => MarketStatus::Active,
            _ => self.status.clone(),
        }
    }

    pub fn is_active_at(&self, now_ms: u64) -> bool {
        self.status_at(now_ms) == MarketStatus::Active
    }

    pub fn with_status_at(mut self, now_ms: u64) -> Self {
        self.status = self.status_at(now_ms);
        self
    }

    pub fn supports_quote_asset(&self, quote_asset: &str) -> bool {
        self.quote_assets.iter().any(|asset| asset.eq_ignore_ascii_case(quote_asset))
    }

    pub fn searchable_text(&self) -> String {
        let mut parts = vec![
            self.market_id.clone(),
            self.oracle_id.clone(),
            self.underlying.clone(),
            self.display_name.clone(),
            self.preferred_quote_asset.clone(),
        ];
        parts.extend(self.tags.clone());
        parts.join(" ").to_ascii_lowercase().replace(['-', '_', '/'], " ")
    }
}

pub fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}
