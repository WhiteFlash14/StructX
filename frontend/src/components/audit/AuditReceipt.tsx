"use client";

import { useState } from "react";

import { CopyButton } from "@/components/common/CopyButton";
import { StatusPill } from "@/components/common/StatusPill";
import {
  formatCompactNumber,
  formatDusdcDisplayString,
  formatPriceCompact,
  shortAddress,
} from "@/lib/format";
import type { AuditResponse } from "@/types/structx";

type Props = {
  audit: AuditResponse;
  onCopied: (label: string) => void;
};

export function AuditReceipt({ audit, onCopied }: Props) {
  const [debugOpen, setDebugOpen] = useState(false);
  const verification = audit.positionVerification;
  const minted = audit.mintedLegs ?? [];
  const partial = verification?.status === "partial";
  const tone: "ok" | "warn" | "danger" = audit.ok
    ? partial
      ? "warn"
      : "ok"
    : "danger";

  return (
    <section className="panel receipt">
      <div className="receipt-head">
        <div>
          <p className="eyebrow">Post-trade audit</p>
          <h2>
            {audit.ok
              ? partial
                ? "Audit accepted with caution"
                : "Audit accepted"
              : "Audit failed"}
          </h2>
          <p className="muted">
            Execution status:{" "}
            <strong>{audit.executionStatus ?? "unknown"}</strong>
          </p>
        </div>
        <StatusPill
          label={tone === "ok" ? "Success" : tone === "warn" ? "Partial" : "Failed"}
          tone={tone}
          dot
        />
      </div>

      <div className="receipt-stats">
        <Stat
          label="Total premium paid"
          value={formatDusdcDisplayString(audit.totalCostDisplay)}
        />
        <Stat
          label="Manager balance after"
          value={formatDusdcDisplayString(audit.managerBalanceDisplay)}
        />
        <Stat
          label="Verified positions"
          value={
            verification
              ? `${verification.verifiedCount} / ${
                  verification.verifiedCount + verification.mismatchCount
                }`
              : "—"
          }
        />
      </div>

      <div className="receipt-rows">
        {audit.digest && (
          <div className="receipt-row">
            <span className="receipt-row-label">Transaction</span>
            <span className="receipt-row-value">
              <code title={audit.digest}>{shortAddress(audit.digest)}</code>
              <CopyButton
                value={audit.digest}
                label=""
                onCopied={() => onCopied("digest")}
                className="tight"
              />
              {audit.explorerUrl && (
                <a
                  href={audit.explorerUrl}
                  target="_blank"
                  rel="noreferrer noopener"
                  className="link-button"
                >
                  Open in explorer
                </a>
              )}
            </span>
          </div>
        )}
      </div>

      {minted.length > 0 && (
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Event</th>
                <th>Type</th>
                <th>Direction</th>
                <th>Strike / range</th>
                <th>Quantity</th>
                <th>Cost</th>
              </tr>
            </thead>
            <tbody>
              {minted.map((leg) => (
                <tr key={leg.index}>
                  <td>
                    <span className="kind-pill subtle">{leg.event}</span>
                  </td>
                  <td>{leg.kind}</td>
                  <td>{leg.direction ?? "—"}</td>
                  <td className="mono">
                    {leg.kind === "RANGE"
                      ? `${formatPriceCompact(leg.lower)} → ${formatPriceCompact(leg.upper)}`
                      : formatPriceCompact(leg.strike)}
                  </td>
                  <td className="mono">{formatCompactNumber(leg.quantityDisplay)}</td>
                  <td className="mono">{formatDusdcDisplayString(leg.costDisplay)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {partial && (
        <div className="warning-item severity-caution">
          <span className="warning-tag">Caution</span>
          <p>
            Position verification is partial. Range legs verified. Binary
            manager-key verification is a known issue under investigation.
          </p>
        </div>
      )}

      <button
        type="button"
        className="advanced-toggle"
        aria-expanded={debugOpen}
        onClick={() => setDebugOpen((v) => !v)}
      >
        {debugOpen ? "Hide" : "Show"} advanced debug details
      </button>
      {debugOpen && (
        <div className="debug-grid">
          {audit.debug?.stdout && (
            <div className="debug-block">
              <p className="debug-label">Audit CLI stdout</p>
              <pre className="debug-pre">{audit.debug.stdout}</pre>
            </div>
          )}
          {audit.debug?.stderr && (
            <div className="debug-block">
              <p className="debug-label">Audit CLI stderr</p>
              <pre className="debug-pre">{audit.debug.stderr}</pre>
            </div>
          )}
          {audit.artifactPath && (
            <div className="debug-block">
              <p className="debug-label">Artifact path</p>
              <pre className="debug-pre">{audit.artifactPath}</pre>
            </div>
          )}
        </div>
      )}
    </section>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="stat">
      <label>{label}</label>
      <strong>{value}</strong>
    </div>
  );
}
