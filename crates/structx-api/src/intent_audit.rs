use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context};
use serde::{Deserialize, Serialize};
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentExecutionAudit {
    pub schema_version: u32,
    pub audit_id: String,
    pub proposal_id: String,
    pub user_address: Option<String>,
    pub manager_id: Option<String>,
    pub tx_digest: String,
    pub status: IntentExecutionStatus,
    pub market_id: String,
    pub oracle_id: String,
    pub underlying: String,
    pub strategy_template: String,
    pub backend_strategy_id: String,
    pub total_premium: u64,
    pub max_loss: u64,
    pub max_payout: u64,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub warnings: Vec<String>,
    pub raw_execution_result: serde_json::Value,
    pub proposal: structx_service::ExecutionProposal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IntentExecutionStatus {
    Submitted,
    Confirmed,
    Failed,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct DiskIntentAuditStore {
    root_dir: PathBuf,
}

impl DiskIntentAuditStore {
    pub fn new(root_dir: impl Into<PathBuf>) -> Self {
        Self { root_dir: root_dir.into() }
    }

    pub fn default_state_dir() -> Self {
        Self::new("artifacts/structx_state/intent_audits")
    }

    fn audit_path(&self, audit_id: &str) -> PathBuf {
        self.root_dir.join(format!("{audit_id}.json"))
    }

    fn by_proposal_dir(&self) -> PathBuf {
        self.root_dir.join("by_proposal")
    }

    fn by_proposal_path(&self, proposal_id: &str) -> PathBuf {
        self.by_proposal_dir().join(format!("{proposal_id}.json"))
    }

    fn by_digest_dir(&self) -> PathBuf {
        self.root_dir.join("by_digest")
    }

    fn by_digest_path(&self, digest: &str) -> PathBuf {
        self.by_digest_dir().join(format!("{digest}.json"))
    }

    pub async fn save(&self, audit: &IntentExecutionAudit) -> anyhow::Result<()> {
        self.ensure_dirs().await?;
        self.atomic_write_json(&self.audit_path(&audit.audit_id), audit).await?;
        self.atomic_write_json(&self.by_proposal_path(&audit.proposal_id), audit).await?;
        self.atomic_write_json(&self.by_digest_path(&audit.tx_digest), audit).await?;
        Ok(())
    }

    pub async fn load_by_proposal(
        &self,
        proposal_id: &str,
    ) -> anyhow::Result<Option<IntentExecutionAudit>> {
        self.load_path(self.by_proposal_path(proposal_id)).await
    }

    pub async fn load_by_digest(
        &self,
        digest: &str,
    ) -> anyhow::Result<Option<IntentExecutionAudit>> {
        self.load_path(self.by_digest_path(digest)).await
    }

    pub async fn list_recent(&self, max: usize) -> anyhow::Result<Vec<IntentExecutionAudit>> {
        self.ensure_dirs().await?;

        let mut entries = fs::read_dir(&self.root_dir).await?;
        let mut audits = Vec::new();

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }

            if let Some(audit) = self.load_path(path).await? {
                audits.push(audit);
            }
        }

        audits.sort_by_key(|audit| std::cmp::Reverse(audit.created_at_ms));
        audits.truncate(max);
        Ok(audits)
    }

    async fn load_path(&self, path: PathBuf) -> anyhow::Result<Option<IntentExecutionAudit>> {
        if !path.exists() {
            return Ok(None);
        }

        let bytes = fs::read(&path)
            .await
            .with_context(|| format!("failed to read intent audit {}", path.display()))?;

        let audit: IntentExecutionAudit = serde_json::from_slice(&bytes)
            .with_context(|| format!("failed to decode intent audit {}", path.display()))?;

        Ok(Some(audit))
    }

    async fn ensure_dirs(&self) -> anyhow::Result<()> {
        fs::create_dir_all(&self.root_dir).await?;
        fs::create_dir_all(self.by_proposal_dir()).await?;
        fs::create_dir_all(self.by_digest_dir()).await?;
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
            .with_context(|| format!("failed to write temp audit {}", tmp_path.display()))?;

        let file = std::fs::OpenOptions::new()
            .read(true)
            .open(&tmp_path)
            .with_context(|| format!("failed to open temp audit {}", tmp_path.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to fsync temp audit {}", tmp_path.display()))?;

        fs::rename(&tmp_path, path)
            .await
            .with_context(|| format!("failed to rename audit {}", path.display()))?;

        Ok(())
    }
}

pub fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

pub fn make_audit_id(proposal_id: &str, tx_digest: &str) -> String {
    let short_digest = tx_digest.chars().take(16).collect::<String>();
    format!("intent_audit_{}_{}", proposal_id, short_digest)
}

pub fn infer_execution_status(raw: &serde_json::Value) -> IntentExecutionStatus {
    let status = raw
        .get("effects")
        .and_then(|effects| effects.get("status"))
        .and_then(|status| status.get("status"))
        .and_then(|status| status.as_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    match status.as_str() {
        "success" => IntentExecutionStatus::Confirmed,
        "failure" | "failed" => IntentExecutionStatus::Failed,
        _ => IntentExecutionStatus::Submitted,
    }
}

pub fn infer_manager_id_from_execution(raw: &serde_json::Value) -> Option<String> {
    let changes = raw.get("objectChanges")?.as_array()?;

    for change in changes {
        let object_type = change
            .get("objectType")
            .or_else(|| change.get("object_type"))
            .or_else(|| change.get("type"))
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        if !object_type.contains("PredictManager") && !object_type.contains("predict_manager") {
            continue;
        }

        if let Some(object_id) =
            change.get("objectId").or_else(|| change.get("object_id")).and_then(|v| v.as_str())
        {
            return Some(object_id.to_string());
        }
    }

    None
}
