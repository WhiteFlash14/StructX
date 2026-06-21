"use client";

import { useMemo } from "react";

import { formatDusdcDisplayString, formatPriceCompact } from "@/lib/format";
import type { CompileResponse } from "@/types/structx";

type Props = {
  compiled: CompileResponse;
};

// Render a simple stylized step chart over K1..K4 showing whether each scenario
// is in-the-money for the user (green) or in the loss band (red).
export function PayoffVisualization({ compiled }: Props) {
  const rows = compiled.payoffTable;
  const strikeLabels = [
    formatPriceCompact(compiled.strikes.k1),
    formatPriceCompact(compiled.strikes.k2),
    formatPriceCompact(compiled.strikes.k3),
    formatPriceCompact(compiled.strikes.k4),
  ];

  const buckets = useMemo(() => {
    const labels: string[] = [
      `BTC < ${strikeLabels[0]}`,
      `${strikeLabels[0]} to ${strikeLabels[1]}`,
      `${strikeLabels[1]} to ${strikeLabels[2]}`,
      `${strikeLabels[2]} to ${strikeLabels[3]}`,
      `BTC > ${strikeLabels[3]}`,
    ];
    return labels.map((label, idx) => {
      const row = rows[idx];
      const net = safeBigInt(row?.netPnlRaw);
      const positive = net !== null && net > 0n;
      const flat = net !== null && net === 0n;
      return {
        label,
        netDisplay: formatDusdcDisplayString(row?.netPnlDisplay),
        grossDisplay: formatDusdcDisplayString(row?.grossPayoutDisplay),
        tone: positive ? "positive" : flat ? "neutral" : "negative",
      };
    });
  }, [rows, strikeLabels]);

  return (
    <section className="panel">
      <div className="panel-header">
        <p className="eyebrow">Payoff shape</p>
        <h2>Where this strategy pays</h2>
        <p className="muted">
          Green bands win, red bands lose. Loss is capped at the premium paid.
        </p>
      </div>

      <div className="payoff-vis">
        <div className="payoff-row">
          {buckets.map((b, idx) => (
            <div key={idx} className={`payoff-cell tone-${b.tone}`}>
              <p className="payoff-label">{b.label}</p>
              <p className="payoff-net">{b.netDisplay}</p>
              <p className="payoff-gross">payout {b.grossDisplay}</p>
            </div>
          ))}
        </div>
        <div className="payoff-axis">
          <span>Unavailable</span>
          {strikeLabels.map((s, idx) => (
            <span key={idx} className="mono">
              {s}
            </span>
          ))}
          <span>+</span>
        </div>
      </div>
    </section>
  );
}

function safeBigInt(value: string | undefined | null): bigint | null {
  if (value === undefined || value === null) return null;
  try {
    return BigInt(value);
  } catch {
    return null;
  }
}
