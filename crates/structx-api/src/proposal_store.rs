use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context};
use serde::{Deserialize, Serialize};
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredExecutionProposal {
    pub proposal_id: String,
    pub created_at_ms: u64,
    pub expires_at_ms: u64,
    pub proposal: structx_service::ExecutionProposal,
}

#[derive(Debug, Clone)]
pub struct DiskProposalStore {
    root_dir: PathBuf,
}

impl DiskProposalStore {
    pub fn new(root_dir: impl Into<PathBuf>) -> Self {
        Self { root_dir: root_dir.into() }
    }

    pub fn default_state_dir() -> Self {
        Self::new("artifacts/structx_state/proposals")
    }

    fn proposal_path(&self, proposal_id: &str) -> PathBuf {
        self.root_dir.join(format!("{proposal_id}.json"))
    }

    pub async fn save(
        &self,
        proposal: structx_service::ExecutionProposal,
    ) -> anyhow::Result<StoredExecutionProposal> {
        self.ensure_dirs().await?;

        let now = now_ms();
        let expires_at_ms = proposal
            .quote_metadata
            .quoted_at_ms
            .saturating_add(proposal.quote_metadata.max_quote_age_ms);

        let stored = StoredExecutionProposal {
            proposal_id: proposal.proposal_id.clone(),
            created_at_ms: now,
            expires_at_ms,
            proposal,
        };

        self.atomic_write_json(&self.proposal_path(&stored.proposal_id), &stored).await?;

        Ok(stored)
    }

    pub async fn load(&self, proposal_id: &str) -> anyhow::Result<Option<StoredExecutionProposal>> {
        let path = self.proposal_path(proposal_id);
        if !path.exists() {
            return Ok(None);
        }

        let bytes = fs::read(&path)
            .await
            .with_context(|| format!("failed to read proposal {}", path.display()))?;

        let stored: StoredExecutionProposal = serde_json::from_slice(&bytes)
            .with_context(|| format!("failed to decode proposal {}", path.display()))?;

        Ok(Some(stored))
    }

    pub async fn require_fresh(
        &self,
        proposal_id: &str,
        now_ms: u64,
    ) -> anyhow::Result<StoredExecutionProposal> {
        let stored = self
            .load(proposal_id)
            .await?
            .ok_or_else(|| anyhow!("proposal not found: {proposal_id}"))?;

        if now_ms >= stored.expires_at_ms {
            return Err(anyhow!(
                "proposal quote expired: proposal_id={}, expired_at_ms={}, now_ms={}",
                proposal_id,
                stored.expires_at_ms,
                now_ms
            ));
        }

        Ok(stored)
    }

    async fn ensure_dirs(&self) -> anyhow::Result<()> {
        fs::create_dir_all(&self.root_dir).await.with_context(|| {
            format!("failed to create proposal dir {}", self.root_dir.display())
        })?;
        Ok(())
    }

    async fn atomic_write_json<T: serde::Serialize>(
        &self,
        path: &Path,
        value: &T,
    ) -> anyhow::Result<()> {
        let parent =
            path.parent().ok_or_else(|| anyhow!("path has no parent: {}", path.display()))?;
        fs::create_dir_all(parent).await?;

        let tmp_path = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(value)?;

        fs::write(&tmp_path, bytes)
            .await
            .with_context(|| format!("failed to write temp proposal {}", tmp_path.display()))?;

        let file = std::fs::OpenOptions::new()
            .read(true)
            .open(&tmp_path)
            .with_context(|| format!("failed to open temp proposal {}", tmp_path.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to fsync temp proposal {}", tmp_path.display()))?;

        fs::rename(&tmp_path, path)
            .await
            .with_context(|| format!("failed to rename proposal {}", path.display()))?;

        Ok(())
    }
}

pub fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_proposal() -> structx_service::ExecutionProposal {
        structx_service::ExecutionProposal {
            proposal_id: "proposal_test".to_string(),
            user_address: None,
            raw_prompt: "btc up with 10 dusdc".to_string(),
            selected_market: structx_service::CatalogMarketSnapshot {
                market_id: "btc-market".to_string(),
                oracle_id: "0xoracle".to_string(),
                underlying: "BTC".to_string(),
                display_name: "BTC Test".to_string(),
                category: structx_service::MarketCategory::Crypto,
                market_kind: structx_service::MarketKind::ScalarPrice,
                expiry_ms: 1_900_000_000_000,
                status: structx_service::MarketStatus::Active,
                spot: Some(100_000.0),
                settlement_price: None,
                valid_strikes: vec![90_000, 100_000, 110_000],
                min_strike: Some(90_000),
                max_strike: Some(110_000),
                quote_assets: vec!["DUSDC".to_string()],
                preferred_quote_asset: "DUSDC".to_string(),
                latest_price_updated_at_ms: None,
                svi_updated_at_ms: None,
                fetched_at_ms: now_ms(),
                tags: vec!["btc".to_string()],
                metadata: serde_json::json!({}),
            },
            reason_for_selection: "test".to_string(),
            strategy_template: structx_service::StrategyTemplateId::DirectionalAbove,
            backend_strategy_id: "moonshot_upside".to_string(),
            legs: vec![],
            total_premium: 10_000_000,
            max_loss: 10_000_000,
            max_payout: 100_000_000,
            payoff_table: vec![],
            net_pnl_table: vec![],
            quote_metadata: structx_service::ProposalQuoteMetadata {
                quote_batch_id: "quote_test".to_string(),
                quoted_at_ms: now_ms(),
                max_quote_age_ms: 30_000,
                source: "test".to_string(),
                oracle_id: "0xoracle".to_string(),
                market_fetched_at_ms: now_ms(),
            },
            assumptions: vec![],
            warnings: vec![],
            requires_user_signature: true,
            raw_compiled_strategy: serde_json::json!({ "ok": true }),
        }
    }

    #[tokio::test]
    async fn saves_and_loads_proposal() {
        let temp = tempfile::tempdir().unwrap();
        let store = DiskProposalStore::new(temp.path());

        let proposal = sample_proposal();
        let proposal_id = proposal.proposal_id.clone();

        store.save(proposal).await.unwrap();

        let loaded = store.load(&proposal_id).await.unwrap().unwrap();
        assert_eq!(loaded.proposal_id, proposal_id);
        assert_eq!(loaded.proposal.total_premium, 10_000_000);
    }

    #[tokio::test]
    async fn rejects_expired_proposal_at_boundary() {
        let temp = tempfile::tempdir().unwrap();
        let store = DiskProposalStore::new(temp.path());

        let mut proposal = sample_proposal();
        proposal.quote_metadata.quoted_at_ms = 1;
        proposal.quote_metadata.max_quote_age_ms = 99;

        let proposal_id = proposal.proposal_id.clone();
        store.save(proposal).await.unwrap();

        let err = store.require_fresh(&proposal_id, 100).await.unwrap_err();
        assert!(err.to_string().contains("expired"));
    }
}
