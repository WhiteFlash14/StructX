"use client";

import { DebugDetails } from "@/components/DebugDetails";
import { copyToClipboard, shortAddress } from "@/lib/format";
import type { AuditResponse } from "@/types/structx";

type Props = {
  audit: AuditResponse;
  onCopy?: (label: string, value: string) => void;
};

export function AuditResult({ audit, onCopy }: Props) {
  const verification = audit.positionVerification;
  const minted = audit.mintedLegs ?? [];
  const partial = verification?.status === "partial";

  return (
    <section className="panel">
      <div className="panel-header">
        <p className="eyebrow">Post-trade audit</p>
        <h2>{audit.ok ? "Audit accepted" : "Audit failed"}</h2>
        {audit.executionStatus && (
          <p className="muted">
            Execution status: <strong>{audit.executionStatus}</strong>
          </p>
        )}
      </div>

      <div className="stats-grid">
        <Stat label="Total premium paid" value={audit.totalCostDisplay ?? "Unavailable"} />
        <Stat
          label="Manager balance after"
          value={audit.managerBalanceDisplay ?? "Unavailable"}
        />
        <Stat
          label="Verified positions"
          value={
            verification
              ? `${verification.verifiedCount} / ${
                  verification.verifiedCount + verification.mismatchCount
                }`
              : "Unavailable"
          }
        />
        <Stat
          label="Verification status"
          value={verification?.status ?? "Unavailable"}
        />
      </div>

      <div className="meta-grid">
        {audit.digest && (
          <DigestMeta
            label="Transaction digest"
            digest={audit.digest}
            explorerUrl={audit.explorerUrl}
            onCopy={onCopy}
          />
        )}
        {audit.managerId && (
          <div className="meta-item">
            <label>PredictManager</label>
            <span className="mono">{shortAddress(audit.managerId)}</span>
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
                  <td>{leg.direction ?? "Unavailable"}</td>
                  <td className="mono">
                    {leg.kind === "RANGE"
                      ? `${leg.lower ?? "Unavailable"} to ${leg.upper ?? "Unavailable"}`
                      : (leg.strike ?? "Unavailable")}
                  </td>
                  <td className="mono">{leg.quantityDisplay}</td>
                  <td className="mono">{leg.costDisplay}</td>
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

      <DebugDetails
        title="Advanced debug details"
        sections={[
          { label: "Audit CLI stdout", content: audit.debug?.stdout ?? audit.stdout },
          { label: "Audit CLI stderr", content: audit.debug?.stderr ?? audit.stderr },
          { label: "Artifact path", content: audit.artifactPath },
        ]}
      />
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

function DigestMeta({
  label,
  digest,
  explorerUrl,
  onCopy,
}: {
  label: string;
  digest: string;
  explorerUrl?: string;
  onCopy?: (label: string, value: string) => void;
}) {
  const handleCopy = async () => {
    await copyToClipboard(digest);
    onCopy?.("digest", digest);
  };
  return (
    <div className="meta-item">
      <label>{label}</label>
      <span className="meta-copy">
        <code title={digest}>{shortAddress(digest)}</code>
        <button type="button" className="mini-button" onClick={handleCopy}>
          Copy
        </button>
        {explorerUrl && (
          <a
            href={explorerUrl}
            target="_blank"
            rel="noreferrer noopener"
            className="mini-button link-button"
          >
            Explorer ↗
          </a>
        )}
      </span>
    </div>
  );
}
