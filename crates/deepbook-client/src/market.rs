use chrono::{DateTime, Duration, TimeZone, Utc};
use serde::{Deserialize, Serialize};

use crate::models::{AskBounds, LatestPrice, LatestSvi, OracleListItem, OracleState};

#[derive(Debug, Clone, Copy)]
pub struct FreshnessConfig {
    pub max_price_age: Duration,
    pub max_svi_age: Duration,
    pub min_time_to_expiry: Duration,
    pub require_price_timestamp: bool,
    pub require_svi_timestamp: bool,
}

impl Default for FreshnessConfig {
    fn default() -> Self {
        Self {
            max_price_age: Duration::seconds(60),
            max_svi_age: Duration::seconds(60),
            min_time_to_expiry: Duration::minutes(5),

            // Testnet public-server responses can expose latest values without
            // timestamp fields in the shape our parser recognizes. For market
            // discovery, missing timestamps should warn, not reject.
            require_price_timestamp: false,
            require_svi_timestamp: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarketRejectionReason {
    NonBtc,
    NotActiveOrLive,
    MissingLatestPrice,
    StalePrice,
    MissingLatestSvi,
    StaleSvi,
    MissingExpiry,
    ExpiryTooClose,
    MissingMinStrike,
    MissingTickSize,
    VaultSummaryUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarketWarning {
    MissingLatestPriceTimestamp,
    MissingLatestSviTimestamp,
    AskBoundsUnavailable,
}

#[derive(Debug, Clone, Serialize)]
pub enum StructxMarketStatus {
    Usable,
    UsableWithWarnings(Vec<MarketWarning>),
    Rejected { reasons: Vec<MarketRejectionReason>, warnings: Vec<MarketWarning> },
}

impl StructxMarketStatus {
    #[must_use]
    pub fn is_usable(&self) -> bool {
        matches!(self, Self::Usable | Self::UsableWithWarnings(_))
    }

    #[must_use]
    pub fn warnings(&self) -> &[MarketWarning] {
        match self {
            Self::Usable => &[],
            Self::UsableWithWarnings(warnings) => warnings,
            Self::Rejected { warnings, .. } => warnings,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MarketSnapshot {
    pub list_item: OracleListItem,
    pub state: Option<OracleState>,
    pub latest_price: Option<LatestPrice>,
    pub latest_svi: Option<LatestSvi>,
    pub ask_bounds: Option<AskBounds>,
    pub structx_status: StructxMarketStatus,
}

impl MarketSnapshot {
    #[must_use]
    pub fn evaluate(
        list_item: OracleListItem,
        state: Option<OracleState>,
        latest_price: Option<LatestPrice>,
        latest_svi: Option<LatestSvi>,
        ask_bounds: Option<AskBounds>,
        vault_summary_available: bool,
        now: DateTime<Utc>,
        config: FreshnessConfig,
    ) -> Self {
        let evaluation = evaluate_market(
            &list_item,
            state.as_ref(),
            latest_price.as_ref(),
            latest_svi.as_ref(),
            ask_bounds.as_ref(),
            vault_summary_available,
            now,
            config,
        );

        let structx_status = if evaluation.rejections.is_empty() && evaluation.warnings.is_empty() {
            StructxMarketStatus::Usable
        } else if evaluation.rejections.is_empty() {
            StructxMarketStatus::UsableWithWarnings(evaluation.warnings)
        } else {
            StructxMarketStatus::Rejected {
                reasons: evaluation.rejections,
                warnings: evaluation.warnings,
            }
        };

        Self { list_item, state, latest_price, latest_svi, ask_bounds, structx_status }
    }

    #[must_use]
    pub fn oracle_id(&self) -> Option<&str> {
        self.state
            .as_ref()
            .and_then(|s| s.oracle_id.as_deref())
            .or(self.list_item.oracle_id.as_deref())
    }

    #[must_use]
    pub fn underlying(&self) -> Option<&str> {
        self.state
            .as_ref()
            .and_then(|s| s.underlying_asset.as_deref())
            .or(self.list_item.underlying_asset.as_deref())
    }

    #[must_use]
    pub fn status(&self) -> Option<&str> {
        self.state.as_ref().and_then(|s| s.status.as_deref()).or(self.list_item.status.as_deref())
    }

    #[must_use]
    pub fn expiry_ms(&self) -> Option<i64> {
        self.state.as_ref().and_then(|s| s.expiry_ms).or(self.list_item.expiry_ms)
    }

    #[must_use]
    pub fn expiry_datetime(&self) -> Option<DateTime<Utc>> {
        self.expiry_ms().and_then(|ms| Utc.timestamp_millis_opt(ms).single())
    }

    #[must_use]
    pub fn min_strike(&self) -> Option<u64> {
        self.state.as_ref().and_then(|s| s.min_strike)
    }

    #[must_use]
    pub fn tick_size(&self) -> Option<u64> {
        self.state.as_ref().and_then(|s| s.tick_size)
    }

    #[must_use]
    pub fn price_age_seconds(&self, now: DateTime<Utc>) -> Option<i64> {
        let ts = self.latest_price.as_ref()?.timestamp_datetime()?;
        Some((now - ts).num_seconds())
    }

    #[must_use]
    pub fn svi_age_seconds(&self, now: DateTime<Utc>) -> Option<i64> {
        let ts = self.latest_svi.as_ref()?.timestamp_datetime()?;
        Some((now - ts).num_seconds())
    }
}

#[derive(Debug, Default)]
struct MarketEvaluation {
    rejections: Vec<MarketRejectionReason>,
    warnings: Vec<MarketWarning>,
}

fn evaluate_market(
    list_item: &OracleListItem,
    state: Option<&OracleState>,
    latest_price: Option<&LatestPrice>,
    latest_svi: Option<&LatestSvi>,
    ask_bounds: Option<&AskBounds>,
    vault_summary_available: bool,
    now: DateTime<Utc>,
    config: FreshnessConfig,
) -> MarketEvaluation {
    let mut evaluation = MarketEvaluation::default();

    let underlying =
        state.and_then(|s| s.underlying_asset.as_deref()).or(list_item.underlying_asset.as_deref());

    if !underlying.map(|value| value.eq_ignore_ascii_case("BTC")).unwrap_or(false) {
        evaluation.rejections.push(MarketRejectionReason::NonBtc);
    }

    let active =
        state.map(OracleState::is_active_or_live).unwrap_or_else(|| list_item.is_active_or_live());

    if !active {
        evaluation.rejections.push(MarketRejectionReason::NotActiveOrLive);
    }

    match latest_price {
        Some(price) => match price.timestamp_datetime() {
            Some(ts) => {
                if now.signed_duration_since(ts) > config.max_price_age {
                    evaluation.rejections.push(MarketRejectionReason::StalePrice);
                }
            }
            None if config.require_price_timestamp => {
                evaluation.rejections.push(MarketRejectionReason::StalePrice);
            }
            None => {
                evaluation.warnings.push(MarketWarning::MissingLatestPriceTimestamp);
            }
        },
        None => evaluation.rejections.push(MarketRejectionReason::MissingLatestPrice),
    }

    match latest_svi {
        Some(svi) => match svi.timestamp_datetime() {
            Some(ts) => {
                if now.signed_duration_since(ts) > config.max_svi_age {
                    evaluation.rejections.push(MarketRejectionReason::StaleSvi);
                }
            }
            None if config.require_svi_timestamp => {
                evaluation.rejections.push(MarketRejectionReason::StaleSvi);
            }
            None => {
                evaluation.warnings.push(MarketWarning::MissingLatestSviTimestamp);
            }
        },
        None => evaluation.rejections.push(MarketRejectionReason::MissingLatestSvi),
    }

    let expiry_ms = state.and_then(|s| s.expiry_ms).or(list_item.expiry_ms);

    match expiry_ms.and_then(|ms| Utc.timestamp_millis_opt(ms).single()) {
        Some(expiry) => {
            if expiry.signed_duration_since(now) < config.min_time_to_expiry {
                evaluation.rejections.push(MarketRejectionReason::ExpiryTooClose);
            }
        }
        None => evaluation.rejections.push(MarketRejectionReason::MissingExpiry),
    }

    if state.and_then(|s| s.min_strike).is_none() {
        evaluation.rejections.push(MarketRejectionReason::MissingMinStrike);
    }

    if state.and_then(|s| s.tick_size).is_none() {
        evaluation.rejections.push(MarketRejectionReason::MissingTickSize);
    }

    if !vault_summary_available {
        evaluation.rejections.push(MarketRejectionReason::VaultSummaryUnavailable);
    }

    if ask_bounds.is_none() {
        evaluation.warnings.push(MarketWarning::AskBoundsUnavailable);
    }

    evaluation
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{LatestPrice, LatestSvi};
    use serde_json::json;

    fn base_list_item(now: DateTime<Utc>) -> OracleListItem {
        OracleListItem {
            oracle_id: Some("0xabc".to_string()),
            underlying_asset: Some("BTC".to_string()),
            status: Some("active".to_string()),
            expiry_ms: Some((now + Duration::minutes(30)).timestamp_millis()),
            extra: Default::default(),
        }
    }

    fn base_state(now: DateTime<Utc>) -> OracleState {
        OracleState {
            oracle_id: Some("0xabc".to_string()),
            underlying_asset: Some("BTC".to_string()),
            status: Some("active".to_string()),
            expiry_ms: Some((now + Duration::minutes(30)).timestamp_millis()),
            min_strike: Some(50_000_000_000_000),
            max_strike: Some(150_000_000_000_000),
            tick_size: Some(1_000_000_000),
            raw: json!({}),
        }
    }

    fn latest_price_at(ts: DateTime<Utc>) -> LatestPrice {
        LatestPrice {
            timestamp_ms: Some(ts.timestamp_millis()),
            price: Some(63_000.0),
            raw: json!({}),
        }
    }

    fn latest_svi_at(ts: DateTime<Utc>) -> LatestSvi {
        LatestSvi {
            timestamp_ms: Some(ts.timestamp_millis()),
            spot: Some(63_000.0),
            forward: Some(63_100.0),
            raw: json!({}),
        }
    }

    #[test]
    fn market_freshness_filter_accepts_valid_market() {
        let now = Utc.timestamp_millis_opt(1_900_000_000_000).single().expect("valid timestamp");

        let snapshot = MarketSnapshot::evaluate(
            base_list_item(now),
            Some(base_state(now)),
            Some(latest_price_at(now - Duration::seconds(10))),
            Some(latest_svi_at(now - Duration::seconds(20))),
            Some(AskBounds { raw: json!({}) }),
            true,
            now,
            FreshnessConfig::default(),
        );

        assert!(snapshot.structx_status.is_usable());
    }

    #[test]
    fn stale_price_is_rejected() {
        let now = Utc.timestamp_millis_opt(1_900_000_000_000).single().expect("valid timestamp");

        let snapshot = MarketSnapshot::evaluate(
            base_list_item(now),
            Some(base_state(now)),
            Some(latest_price_at(now - Duration::seconds(120))),
            Some(latest_svi_at(now - Duration::seconds(20))),
            Some(AskBounds { raw: json!({}) }),
            true,
            now,
            FreshnessConfig::default(),
        );

        match snapshot.structx_status {
            StructxMarketStatus::Rejected { reasons, .. } => {
                assert!(reasons.contains(&MarketRejectionReason::StalePrice));
            }
            _ => panic!("stale market should be rejected"),
        }
    }

    #[test]
    fn stale_svi_is_rejected() {
        let now = Utc.timestamp_millis_opt(1_900_000_000_000).single().expect("valid timestamp");

        let snapshot = MarketSnapshot::evaluate(
            base_list_item(now),
            Some(base_state(now)),
            Some(latest_price_at(now - Duration::seconds(20))),
            Some(latest_svi_at(now - Duration::seconds(120))),
            Some(AskBounds { raw: json!({}) }),
            true,
            now,
            FreshnessConfig::default(),
        );

        match snapshot.structx_status {
            StructxMarketStatus::Rejected { reasons, .. } => {
                assert!(reasons.contains(&MarketRejectionReason::StaleSvi));
            }
            _ => panic!("stale market should be rejected"),
        }
    }

    #[test]
    fn missing_optional_fields_reject_without_panic() {
        let now = Utc.timestamp_millis_opt(1_900_000_000_000).single().expect("valid timestamp");

        let list_item = OracleListItem {
            oracle_id: Some("0xabc".to_string()),
            underlying_asset: Some("BTC".to_string()),
            status: Some("active".to_string()),
            expiry_ms: None,
            extra: Default::default(),
        };

        let snapshot = MarketSnapshot::evaluate(
            list_item,
            None,
            None,
            None,
            None,
            true,
            now,
            FreshnessConfig::default(),
        );

        match snapshot.structx_status {
            StructxMarketStatus::Rejected { reasons, .. } => {
                assert!(reasons.contains(&MarketRejectionReason::MissingLatestPrice));
                assert!(reasons.contains(&MarketRejectionReason::MissingLatestSvi));
                assert!(reasons.contains(&MarketRejectionReason::MissingExpiry));
                assert!(reasons.contains(&MarketRejectionReason::MissingMinStrike));
                assert!(reasons.contains(&MarketRejectionReason::MissingTickSize));
            }
            _ => panic!("incomplete market should be rejected"),
        }
    }
}
