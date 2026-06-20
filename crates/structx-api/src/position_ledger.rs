use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::storage;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PositionRecord {
    #[serde(rename = "positionId")]
    pub position_id: String,
    pub status: PositionStatus,
    pub strategy: Option<String>,
    #[serde(rename = "sourceDigest")]
    pub source_digest: String,
    #[serde(rename = "openedAtUnix")]
    pub opened_at_unix: i64,
    #[serde(rename = "oracleId")]
    pub oracle_id: String,
    #[serde(rename = "expiryMs")]
    pub expiry_ms: String,
    pub kind: LegKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
    #[serde(rename = "strikeRaw", skip_serializing_if = "Option::is_none")]
    pub strike_raw: Option<String>,
    #[serde(rename = "lowerRaw", skip_serializing_if = "Option::is_none")]
    pub lower_raw: Option<String>,
    #[serde(rename = "upperRaw", skip_serializing_if = "Option::is_none")]
    pub upper_raw: Option<String>,
    #[serde(rename = "originalQuantityRaw")]
    pub original_quantity_raw: String,
    #[serde(rename = "remainingQuantityRaw")]
    pub remaining_quantity_raw: String,
    #[serde(rename = "premiumPaidRaw")]
    pub premium_paid_raw: String,
    #[serde(rename = "realizedPayoutRaw")]
    pub realized_payout_raw: String,
    #[serde(rename = "realizedPnlRaw")]
    pub realized_pnl_raw: String,
    #[serde(rename = "lastPreviewPayoutRaw")]
    pub last_preview_payout_raw: String,
    #[serde(rename = "lastPreviewPnlRaw")]
    pub last_preview_pnl_raw: String,
    #[serde(rename = "lastPreviewAtUnix")]
    pub last_preview_at_unix: i64,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PositionStatus {
    Open,
    Closed,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegKind {
    #[serde(rename = "DOWN")]
    Down,
    #[serde(rename = "UP")]
    Up,
    #[serde(rename = "RANGE")]
    Range,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PositionLedger {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    pub owner: String,
    #[serde(rename = "managerId")]
    pub manager_id: String,
    pub positions: Vec<PositionRecord>,
    #[serde(rename = "auditDigests", default)]
    pub audit_digests: Vec<String>,
    #[serde(rename = "redeemDigests", default)]
    pub redeem_digests: Vec<String>,
    #[serde(rename = "createdAtUnix")]
    pub created_at_unix: i64,
    #[serde(rename = "updatedAtUnix")]
    pub updated_at_unix: i64,
}

#[derive(Debug, Clone)]
pub struct MintedLeg {
    pub kind: LegKind,
    pub direction: Option<String>,
    pub oracle_id: String,
    pub expiry_ms: String,
    pub strike_raw: Option<String>,
    pub lower_raw: Option<String>,
    pub upper_raw: Option<String>,
    pub quantity_raw: u128,
    pub cost_raw: u128,
    pub role: Option<String>,
    pub strategy: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RedeemedLeg {
    pub kind: LegKind,
    pub oracle_id: String,
    pub expiry_ms: String,
    pub strike_raw: Option<String>,
    pub lower_raw: Option<String>,
    pub upper_raw: Option<String>,
    pub quantity_raw: u128,
    pub payout_raw: u128,
}

#[derive(Serialize, Debug, Clone, Default)]
pub struct PositionsSummary {
    #[serde(rename = "openCount")]
    pub open_count: usize,
    #[serde(rename = "closedCount")]
    pub closed_count: usize,
    #[serde(rename = "totalPremiumPaidRaw")]
    pub total_premium_paid_raw: String,
    #[serde(rename = "totalEstimatedRedeemRaw")]
    pub total_estimated_redeem_raw: String,
    #[serde(rename = "totalUnrealizedPnlRaw")]
    pub total_unrealized_pnl_raw: String,
    #[serde(rename = "totalRealizedPnlRaw")]
    pub total_realized_pnl_raw: String,
    #[serde(rename = "earliestExpiryMs", skip_serializing_if = "Option::is_none")]
    pub earliest_expiry_ms: Option<String>,
}

impl PositionLedger {
    pub fn empty(owner: &str, manager_id: &str) -> Self {
        let now = storage::unix_now();
        Self {
            schema_version: SCHEMA_VERSION,
            owner: owner.to_lowercase(),
            manager_id: manager_id.to_lowercase(),
            positions: Vec::new(),
            audit_digests: Vec::new(),
            redeem_digests: Vec::new(),
            created_at_unix: now,
            updated_at_unix: now,
        }
    }

    /// Load ledger from disk; returns an empty ledger when no file exists yet.
    /// Surface corrupt JSON as Err so the caller can choose policy (warn vs
    /// reject).
    pub fn load(owner: &str, manager_id: &str) -> std::io::Result<Self> {
        let path = storage::positions_path(owner, manager_id);
        match storage::read_json::<PositionLedger>(&path)? {
            Some(ledger) => Ok(ledger),
            None => Ok(Self::empty(owner, manager_id)),
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = storage::positions_path(&self.owner, &self.manager_id);
        storage::atomic_write_json(&path, self)
    }

    /// Deterministic key derived from (owner, manager, oracle, expiry, kind,
    /// strike or lower/upper). The same physical leg always hashes to the
    /// same id, so re-applying a mint event from the same digest is idempotent
    /// at the merge level (we still need digest-dedup for safety, though).
    pub fn position_id(owner: &str, manager: &str, leg: &MintedLeg) -> String {
        let mut hasher = Sha256::new();
        hasher.update(owner.to_lowercase().as_bytes());
        hasher.update(b"|");
        hasher.update(manager.to_lowercase().as_bytes());
        hasher.update(b"|");
        hasher.update(leg.oracle_id.to_lowercase().as_bytes());
        hasher.update(b"|");
        hasher.update(leg.expiry_ms.as_bytes());
        hasher.update(b"|");
        match leg.kind {
            LegKind::Down => hasher.update(b"DOWN"),
            LegKind::Up => hasher.update(b"UP"),
            LegKind::Range => hasher.update(b"RANGE"),
        }
        hasher.update(b"|");
        match (&leg.strike_raw, &leg.lower_raw, &leg.upper_raw) {
            (Some(s), _, _) => hasher.update(s.as_bytes()),
            (None, Some(l), Some(u)) => {
                hasher.update(l.as_bytes());
                hasher.update(b"~");
                hasher.update(u.as_bytes());
            }
            _ => hasher.update(b""),
        }
        let digest = hasher.finalize();
        let mut out = String::with_capacity(64);
        for byte in digest {
            out.push_str(&format!("{byte:02x}"));
        }
        format!("pos_{out}")
    }

    /// Apply a mint event. Same-key mints merge (sum quantity + premium).
    pub fn apply_mint(&mut self, leg: &MintedLeg, source_digest: &str, opened_at_unix: i64) {
        let position_id = Self::position_id(&self.owner, &self.manager_id, leg);
        if let Some(existing) = self
            .positions
            .iter_mut()
            .find(|p| p.position_id == position_id)
        {
            let q = existing
                .original_quantity_raw
                .parse::<u128>()
                .unwrap_or(0)
                .saturating_add(leg.quantity_raw);
            existing.original_quantity_raw = q.to_string();

            let remaining = existing
                .remaining_quantity_raw
                .parse::<u128>()
                .unwrap_or(0)
                .saturating_add(leg.quantity_raw);
            existing.remaining_quantity_raw = remaining.to_string();

            let premium = existing
                .premium_paid_raw
                .parse::<u128>()
                .unwrap_or(0)
                .saturating_add(leg.cost_raw);
            existing.premium_paid_raw = premium.to_string();

            existing.status = if remaining == 0 {
                PositionStatus::Closed
            } else {
                PositionStatus::Open
            };
        } else {
            self.positions.push(PositionRecord {
                position_id,
                status: PositionStatus::Open,
                strategy: leg.strategy.clone(),
                source_digest: source_digest.to_string(),
                opened_at_unix,
                oracle_id: leg.oracle_id.clone(),
                expiry_ms: leg.expiry_ms.clone(),
                kind: leg.kind,
                direction: leg.direction.clone(),
                strike_raw: leg.strike_raw.clone(),
                lower_raw: leg.lower_raw.clone(),
                upper_raw: leg.upper_raw.clone(),
                original_quantity_raw: leg.quantity_raw.to_string(),
                remaining_quantity_raw: leg.quantity_raw.to_string(),
                premium_paid_raw: leg.cost_raw.to_string(),
                realized_payout_raw: "0".to_string(),
                realized_pnl_raw: "0".to_string(),
                last_preview_payout_raw: "0".to_string(),
                last_preview_pnl_raw: "0".to_string(),
                last_preview_at_unix: 0,
                metadata: leg
                    .role
                    .as_ref()
                    .map(|role| serde_json::json!({ "role": role, "strategyLabel": leg.strategy }))
                    .unwrap_or(serde_json::Value::Null),
            });
        }
        if !self.audit_digests.iter().any(|d| d == source_digest) {
            self.audit_digests.push(source_digest.to_string());
        }
        self.updated_at_unix = storage::unix_now();
    }

    /// Apply a redeem event: decrement remaining, increment realized payout,
    /// and bump realized PnL using a pro-rata premium basis.
    pub fn apply_redeem(&mut self, leg: &RedeemedLeg, source_digest: &str) {
        // Find by key fields (same lookup as mint, minus role/strategy).
        let position_id = {
            let proxy = MintedLeg {
                kind: leg.kind,
                direction: None,
                oracle_id: leg.oracle_id.clone(),
                expiry_ms: leg.expiry_ms.clone(),
                strike_raw: leg.strike_raw.clone(),
                lower_raw: leg.lower_raw.clone(),
                upper_raw: leg.upper_raw.clone(),
                quantity_raw: 0,
                cost_raw: 0,
                role: None,
                strategy: None,
            };
            Self::position_id(&self.owner, &self.manager_id, &proxy)
        };

        if let Some(existing) = self
            .positions
            .iter_mut()
            .find(|p| p.position_id == position_id)
        {
            let original = existing.original_quantity_raw.parse::<u128>().unwrap_or(0);
            let premium = existing.premium_paid_raw.parse::<u128>().unwrap_or(0);
            let remaining = existing.remaining_quantity_raw.parse::<u128>().unwrap_or(0);
            let redeem_qty = leg.quantity_raw.min(remaining);
            let new_remaining = remaining.saturating_sub(redeem_qty);

            // Pro-rata premium basis for the redeemed slice.
            let premium_basis_redeemed = if original > 0 {
                premium
                    .saturating_mul(redeem_qty)
                    .checked_div(original)
                    .unwrap_or(0)
            } else {
                0
            };

            let realized_payout = existing
                .realized_payout_raw
                .parse::<u128>()
                .unwrap_or(0)
                .saturating_add(leg.payout_raw);
            let realized_pnl_delta = (leg.payout_raw as i128) - (premium_basis_redeemed as i128);
            let realized_pnl_prev = existing.realized_pnl_raw.parse::<i128>().unwrap_or(0);
            let realized_pnl = realized_pnl_prev.saturating_add(realized_pnl_delta);

            existing.remaining_quantity_raw = new_remaining.to_string();
            existing.realized_payout_raw = realized_payout.to_string();
            existing.realized_pnl_raw = realized_pnl.to_string();
            existing.status = if new_remaining == 0 {
                PositionStatus::Closed
            } else {
                PositionStatus::Open
            };
            // A redeem invalidates the prior preview — quote refresh required.
            existing.last_preview_payout_raw = "0".to_string();
            existing.last_preview_pnl_raw = "0".to_string();
            existing.last_preview_at_unix = 0;
        }

        if !self.redeem_digests.iter().any(|d| d == source_digest) {
            self.redeem_digests.push(source_digest.to_string());
        }
        self.updated_at_unix = storage::unix_now();
    }

    pub fn apply_preview(
        &mut self,
        position_id: &str,
        payout_raw: u128,
        pnl_raw: i128,
        previewed_at_unix: i64,
    ) {
        if let Some(existing) = self
            .positions
            .iter_mut()
            .find(|p| p.position_id == position_id)
        {
            existing.last_preview_payout_raw = payout_raw.to_string();
            existing.last_preview_pnl_raw = pnl_raw.to_string();
            existing.last_preview_at_unix = previewed_at_unix;
            self.updated_at_unix = storage::unix_now();
        }
    }

    /// Aggregate snapshot for the dashboard. Sums in u128 so it never wraps
    /// at reasonable testnet volumes.
    pub fn summary(&self) -> PositionsSummary {
        let mut open = 0usize;
        let mut closed = 0usize;
        let mut total_premium: u128 = 0;
        let mut total_est_redeem: u128 = 0;
        let mut total_unrealized: i128 = 0;
        let mut total_realized: i128 = 0;
        let mut earliest: Option<u128> = None;

        for p in &self.positions {
            match p.status {
                PositionStatus::Open => open += 1,
                PositionStatus::Closed => closed += 1,
            }
            total_premium = total_premium.saturating_add(p.premium_paid_raw.parse().unwrap_or(0));
            total_est_redeem =
                total_est_redeem.saturating_add(p.last_preview_payout_raw.parse().unwrap_or(0));
            total_unrealized =
                total_unrealized.saturating_add(p.last_preview_pnl_raw.parse().unwrap_or(0));
            total_realized = total_realized.saturating_add(p.realized_pnl_raw.parse().unwrap_or(0));

            if matches!(p.status, PositionStatus::Open) {
                if let Ok(exp) = p.expiry_ms.parse::<u128>() {
                    earliest = match earliest {
                        Some(prev) if prev <= exp => Some(prev),
                        _ => Some(exp),
                    };
                }
            }
        }

        PositionsSummary {
            open_count: open,
            closed_count: closed,
            total_premium_paid_raw: total_premium.to_string(),
            total_estimated_redeem_raw: total_est_redeem.to_string(),
            total_unrealized_pnl_raw: total_unrealized.to_string(),
            total_realized_pnl_raw: total_realized.to_string(),
            earliest_expiry_ms: earliest.map(|v| v.to_string()),
        }
    }
}

/// Pure premium-basis helper used by the redeem-preview endpoint (later
/// slice). Exposed and tested here because the math has subtle off-by-ones.
pub fn premium_basis_for_slice(
    premium_paid_raw: u128,
    original_quantity_raw: u128,
    redeem_quantity_raw: u128,
) -> u128 {
    if original_quantity_raw == 0 {
        return 0;
    }
    premium_paid_raw
        .saturating_mul(redeem_quantity_raw)
        .checked_div(original_quantity_raw)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_mint(kind: LegKind, qty: u128, cost: u128) -> MintedLeg {
        MintedLeg {
            kind,
            direction: Some(match kind {
                LegKind::Down => "down".to_string(),
                LegKind::Up => "up".to_string(),
                LegKind::Range => "range".to_string(),
            }),
            oracle_id: "0xORACLE".to_string(),
            expiry_ms: "1781289000000".to_string(),
            strike_raw: matches!(kind, LegKind::Range)
                .then(|| "0".to_string())
                .or_else(|| Some("64475000000000".to_string())),
            lower_raw: matches!(kind, LegKind::Range).then(|| "63000000000000".to_string()),
            upper_raw: matches!(kind, LegKind::Range).then(|| "65000000000000".to_string()),
            quantity_raw: qty,
            cost_raw: cost,
            role: Some("moonshot_tail".to_string()),
            strategy: Some("MOONSHOT_UPSIDE".to_string()),
        }
    }

    #[test]
    fn empty_ledger_has_correct_shape() {
        let l = PositionLedger::empty("0xOWNER", "0xMANAGER");
        assert_eq!(l.schema_version, SCHEMA_VERSION);
        assert_eq!(l.owner, "0xowner");
        assert_eq!(l.manager_id, "0xmanager");
        assert!(l.positions.is_empty());
        let s = l.summary();
        assert_eq!(s.open_count, 0);
        assert_eq!(s.closed_count, 0);
    }

    #[test]
    fn apply_mint_inserts_new_position() {
        let mut l = PositionLedger::empty("0xOWNER", "0xMANAGER");
        l.apply_mint(&sample_mint(LegKind::Up, 1000, 500), "0xDIGEST1", 1);
        assert_eq!(l.positions.len(), 1);
        assert_eq!(l.positions[0].original_quantity_raw, "1000");
        assert_eq!(l.positions[0].remaining_quantity_raw, "1000");
        assert_eq!(l.positions[0].premium_paid_raw, "500");
        assert_eq!(l.positions[0].status, PositionStatus::Open);
        assert_eq!(l.audit_digests, vec!["0xDIGEST1"]);
    }

    #[test]
    fn apply_mint_same_key_merges_quantity_and_premium() {
        let mut l = PositionLedger::empty("0xOWNER", "0xMANAGER");
        l.apply_mint(&sample_mint(LegKind::Up, 1000, 500), "0xDIGEST1", 1);
        l.apply_mint(&sample_mint(LegKind::Up, 700, 350), "0xDIGEST2", 2);
        assert_eq!(l.positions.len(), 1, "same key, must merge");
        assert_eq!(l.positions[0].original_quantity_raw, "1700");
        assert_eq!(l.positions[0].remaining_quantity_raw, "1700");
        assert_eq!(l.positions[0].premium_paid_raw, "850");
        assert_eq!(l.audit_digests.len(), 2);
    }

    #[test]
    fn apply_mint_range_inserts_distinct_position() {
        let mut l = PositionLedger::empty("0xOWNER", "0xMANAGER");
        l.apply_mint(&sample_mint(LegKind::Up, 1000, 500), "0xDIGEST", 1);
        l.apply_mint(&sample_mint(LegKind::Range, 2000, 800), "0xDIGEST", 1);
        assert_eq!(l.positions.len(), 2);
    }

    #[test]
    fn apply_redeem_partial_pro_rata_basis() {
        let mut l = PositionLedger::empty("0xOWNER", "0xMANAGER");
        l.apply_mint(&sample_mint(LegKind::Up, 1000, 600), "0xDIGEST_OPEN", 1);

        // Redeem half. Premium basis for the half = 300. If payout > 300, pnl > 0.
        l.apply_redeem(
            &RedeemedLeg {
                kind: LegKind::Up,
                oracle_id: "0xORACLE".to_string(),
                expiry_ms: "1781289000000".to_string(),
                strike_raw: Some("64475000000000".to_string()),
                lower_raw: None,
                upper_raw: None,
                quantity_raw: 500,
                payout_raw: 800,
            },
            "0xDIGEST_REDEEM",
        );
        assert_eq!(l.positions[0].remaining_quantity_raw, "500");
        assert_eq!(l.positions[0].realized_payout_raw, "800");
        // pnl_delta = 800 - 300 = 500
        assert_eq!(l.positions[0].realized_pnl_raw, "500");
        assert_eq!(l.positions[0].status, PositionStatus::Open);
    }

    #[test]
    fn apply_redeem_full_closes() {
        let mut l = PositionLedger::empty("0xOWNER", "0xMANAGER");
        l.apply_mint(&sample_mint(LegKind::Up, 1000, 600), "0xDIGEST_OPEN", 1);
        l.apply_redeem(
            &RedeemedLeg {
                kind: LegKind::Up,
                oracle_id: "0xORACLE".to_string(),
                expiry_ms: "1781289000000".to_string(),
                strike_raw: Some("64475000000000".to_string()),
                lower_raw: None,
                upper_raw: None,
                quantity_raw: 1000,
                payout_raw: 300,
            },
            "0xDIGEST_REDEEM",
        );
        assert_eq!(l.positions[0].remaining_quantity_raw, "0");
        assert_eq!(l.positions[0].realized_payout_raw, "300");
        // pnl_delta = 300 - 600 = -300
        assert_eq!(l.positions[0].realized_pnl_raw, "-300");
        assert_eq!(l.positions[0].status, PositionStatus::Closed);
    }

    #[test]
    fn redeem_more_than_remaining_is_capped() {
        let mut l = PositionLedger::empty("0xOWNER", "0xMANAGER");
        l.apply_mint(&sample_mint(LegKind::Up, 1000, 600), "0xDIGEST_OPEN", 1);
        l.apply_redeem(
            &RedeemedLeg {
                kind: LegKind::Up,
                oracle_id: "0xORACLE".to_string(),
                expiry_ms: "1781289000000".to_string(),
                strike_raw: Some("64475000000000".to_string()),
                lower_raw: None,
                upper_raw: None,
                quantity_raw: 5000, // > remaining
                payout_raw: 100,
            },
            "0xDIGEST_REDEEM",
        );
        assert_eq!(l.positions[0].remaining_quantity_raw, "0");
        assert_eq!(l.positions[0].status, PositionStatus::Closed);
    }

    #[test]
    fn apply_preview_updates_latest_unrealized_snapshot() {
        let mut l = PositionLedger::empty("0xOWNER", "0xMANAGER");
        l.apply_mint(&sample_mint(LegKind::Up, 1000, 600), "0xDIGEST_OPEN", 1);
        let position_id = l.positions[0].position_id.clone();

        l.apply_preview(&position_id, 700, 100, 123);

        assert_eq!(l.positions[0].last_preview_payout_raw, "700");
        assert_eq!(l.positions[0].last_preview_pnl_raw, "100");
        assert_eq!(l.positions[0].last_preview_at_unix, 123);
    }

    #[test]
    fn premium_basis_math_zero_quantity() {
        assert_eq!(premium_basis_for_slice(100, 0, 10), 0);
    }

    #[test]
    fn premium_basis_math_half() {
        assert_eq!(premium_basis_for_slice(600, 1000, 500), 300);
    }

    #[test]
    fn premium_basis_math_full() {
        assert_eq!(premium_basis_for_slice(600, 1000, 1000), 600);
    }

    #[test]
    fn summary_aggregates_open_and_closed() {
        let mut l = PositionLedger::empty("0xOWNER", "0xMANAGER");
        l.apply_mint(&sample_mint(LegKind::Up, 1000, 600), "0xDIGEST1", 1);
        l.apply_mint(&sample_mint(LegKind::Range, 2000, 400), "0xDIGEST2", 2);
        l.apply_redeem(
            &RedeemedLeg {
                kind: LegKind::Up,
                oracle_id: "0xORACLE".to_string(),
                expiry_ms: "1781289000000".to_string(),
                strike_raw: Some("64475000000000".to_string()),
                lower_raw: None,
                upper_raw: None,
                quantity_raw: 1000,
                payout_raw: 700,
            },
            "0xDIGEST_REDEEM",
        );
        let s = l.summary();
        assert_eq!(s.open_count, 1, "range still open");
        assert_eq!(s.closed_count, 1, "up fully redeemed");
        assert_eq!(s.total_premium_paid_raw, "1000");
        assert_eq!(s.total_realized_pnl_raw, "100");
    }
}
