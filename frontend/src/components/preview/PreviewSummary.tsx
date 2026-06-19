"use client";

import { CopyButton } from "@/components/common/CopyButton";
import {
  formatDate,
  formatDusdcDisplayString,
  formatPriceCompact,
  shortAddress,
} from "@/lib/format";
import type { CompileResponse } from "@/types/structx";

type Props = {
  compiled: CompileResponse;
  displayName: string;
  onCopied: (label: string) => void;
};

export function PreviewSummary({ compiled, displayName, onCopied }: Props) {
  return (
    <section className="panel">
      <div className="panel-header">
        <p className="eyebrow">Preview · {displayName}</p>
        <h2>Compiled plan</h2>
        <p className="muted">Snapshot of the strategy ticket your wallet will sign.</p>
      </div>

      <div className="stats-grid">
        <Stat
          label="Premium required"
          value={formatDusdcDisplayString(compiled.premiumRequiredDisplay)}
        />
        <Stat
          label="You can lose at most"
          value={formatDusdcDisplayString(compiled.maxLossDisplay)}
          tone="danger"
        />
        <Stat
          label="Potential gross payout"
          value={formatDusdcDisplayString(compiled.maxGrossPayoutDisplay)}
          tone="success"
        />
        <Stat
          label="Estimated max net PnL"
          value={formatDusdcDisplayString(compiled.maxNetPayoutDisplay)}
          tone="success"
        />
      </div>

      <div className="meta-grid">
        <div className="meta-item">
          <label>Oracle</label>
          <span className="meta-copy">
            <code title={compiled.oracleId}>{shortAddress(compiled.oracleId)}</code>
            <CopyButton
              value={compiled.oracleId}
              label=""
              onCopied={() => onCopied("oracle id")}
              className="tight"
            />
          </span>
        </div>
        <div className="meta-item">
          <label>Expiry</label>
          <span>{formatDate(compiled.expiry)}</span>
        </div>
        <div className="meta-item">
          <label>Spot</label>
          <span className="mono">{formatPriceCompact(compiled.spot)}</span>
        </div>
        <div className="meta-item">
          <label>Style</label>
          <span>{labelForStyle(compiled.style)}</span>
        </div>
        <div className="meta-item span-2">
          <label>Strike ladder</label>
          <span className="mono">
            {formatPriceCompact(compiled.strikes.k1)} /{" "}
            {formatPriceCompact(compiled.strikes.k2)} /{" "}
            {formatPriceCompact(compiled.strikes.k3)} /{" "}
            {formatPriceCompact(compiled.strikes.k4)}
          </span>
        </div>
      </div>
    </section>
  );
}

function Stat({
  label,
  value,
  tone,
}: {
  label: string;
  value: string;
  tone?: "danger" | "success";
}) {
  return (
    <div className={`stat ${tone ? `tone-${tone}` : ""}`}>
      <label>{label}</label>
      <strong>{value}</strong>
    </div>
  );
}

function labelForStyle(style: CompileResponse["style"]) {
  switch (style) {
    case "tail-heavy":
      return "Conservative";
    case "balanced":
      return "Balanced";
    case "higher-hit-rate":
      return "Aggressive";
    default:
      return style;
  }
}
