use serde::{Deserialize, Serialize};

use crate::intent_audit::{DiskIntentAuditStore, IntentExecutionAudit, IntentExecutionStatus};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentPositionSummary {
    pub source: String,
    pub proposal_id: String,
    pub audit_id: String,
    pub tx_digest: String,
    pub user_address: Option<String>,
    pub manager_id: Option<String>,
    pub market_id: String,
    pub oracle_id: String,
    pub underlying: String,
    pub raw_prompt: String,
    pub strategy_template: String,
    pub backend_strategy_id: String,
    pub total_premium: u64,
    pub max_loss: u64,
    pub max_payout: u64,
    pub status: IntentPositionStatus,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IntentPositionStatus {
    PendingConfirmation,
    OpenPendingLedgerSync,
    Failed,
    Unknown,
}

pub async fn list_intent_positions(
    user_address: Option<String>,
    max: usize,
) -> anyhow::Result<Vec<IntentPositionSummary>> {
    let store = DiskIntentAuditStore::default_state_dir();
    let audits = store.list_recent(max).await?;

    let normalized_user = user_address.map(|s| s.to_ascii_lowercase());

    let positions = audits
        .into_iter()
        .filter(|audit| {
            let Some(ref target) = normalized_user else {
                return true;
            };

            audit
                .user_address
                .as_ref()
                .map(|addr| addr.to_ascii_lowercase() == *target)
                .unwrap_or(false)
        })
        .map(intent_position_from_audit)
        .collect();

    Ok(positions)
}

pub fn intent_position_from_audit(audit: IntentExecutionAudit) -> IntentPositionSummary {
    let status = match audit.status {
        IntentExecutionStatus::Submitted => IntentPositionStatus::PendingConfirmation,
        IntentExecutionStatus::Confirmed => IntentPositionStatus::OpenPendingLedgerSync,
        IntentExecutionStatus::Failed => IntentPositionStatus::Failed,
        IntentExecutionStatus::Unknown => IntentPositionStatus::Unknown,
    };

    IntentPositionSummary {
        source: "intent_audit_overlay".to_string(),
        proposal_id: audit.proposal_id,
        audit_id: audit.audit_id,
        tx_digest: audit.tx_digest,
        user_address: audit.user_address,
        manager_id: audit.manager_id,
        market_id: audit.market_id,
        oracle_id: audit.oracle_id,
        underlying: audit.underlying,
        raw_prompt: audit.proposal.raw_prompt,
        strategy_template: audit.strategy_template,
        backend_strategy_id: audit.backend_strategy_id,
        total_premium: audit.total_premium,
        max_loss: audit.max_loss,
        max_payout: audit.max_payout,
        status,
        created_at_ms: audit.created_at_ms,
        updated_at_ms: audit.updated_at_ms,
        warnings: audit.warnings,
    }
}
