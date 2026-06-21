"use client";

import { copyToClipboard, formatDate, shortAddress } from "@/lib/format";
import type { CompileResponse } from "@/types/structx";

type Props = {
  compiled: CompileResponse;
  onCopy?: (label: string, value: string) => void;
};

export function PreviewSummary({ compiled, onCopy }: Props) {
  const handleCopy = async (label: string, value: string) => {
    await copyToClipboard(value);
    onCopy?.(label, value);
  };

  return (
    <section className="panel">
      <div className="panel-header">
        <p className="eyebrow">Preview</p>
        <h2>Breakout Protection</h2>
        <p className="muted">Compiled plan summary.</p>
      </div>

      <div className="stats-grid">
        <Stat label="Premium required" value={compiled.premiumRequiredDisplay} />
        <Stat label="Max loss" value={compiled.maxLossDisplay} />
        <Stat label="Max gross payout" value={compiled.maxGrossPayoutDisplay} />
        <Stat label="Max net payout" value={compiled.maxNetPayoutDisplay} />
      </div>

      <div className="meta-grid">
        <MetaCopy
          label="Oracle"
          value={shortAddress(compiled.oracleId)}
          copyValue={compiled.oracleId}
          onCopy={() => handleCopy("oracle id", compiled.oracleId)}
        />
        <Meta label="Expiry" value={formatDate(compiled.expiry)} />
        <Meta label="Spot" value={compiled.spot} />
        <Meta
          label="Strikes K1 / K2 / K3 / K4"
          value={`${compiled.strikes.k1} / ${compiled.strikes.k2} / ${compiled.strikes.k3} / ${compiled.strikes.k4}`}
        />
        <Meta label="Budget" value={compiled.budgetDisplay} />
        <Meta label="Style" value={compiled.style} />
      </div>
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

function Meta({ label, value }: { label: string; value: string }) {
  return (
    <div className="meta-item">
      <label>{label}</label>
      <span>{value}</span>
    </div>
  );
}

function MetaCopy({
  label,
  value,
  copyValue,
  onCopy,
}: {
  label: string;
  value: string;
  copyValue: string;
  onCopy: () => void;
}) {
  return (
    <div className="meta-item">
      <label>{label}</label>
      <span className="meta-copy">
        <code title={copyValue}>{value}</code>
        <button type="button" className="mini-button" onClick={onCopy}>
          Copy
        </button>
      </span>
    </div>
  );
}
