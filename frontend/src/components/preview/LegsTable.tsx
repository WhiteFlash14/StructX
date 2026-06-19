"use client";

import {
  formatCompactNumber,
  formatDusdcDisplayString,
  formatPriceCompact,
  ROLE_LABELS,
} from "@/lib/format";
import type { StrategyLeg } from "@/types/structx";

const KIND_BADGES: Record<string, { label: string; cls: string }> = {
  DOWN: { label: "Binary down", cls: "kind-pill down" },
  UP: { label: "Binary up", cls: "kind-pill up" },
  RANGE: { label: "Range", cls: "kind-pill range" },
};

function strikeText(leg: StrategyLeg) {
  if (leg.kind === "RANGE") {
    return `${formatPriceCompact(leg.lower)} → ${formatPriceCompact(leg.upper)}`;
  }
  return formatPriceCompact(leg.strike);
}

type Props = {
  legs: StrategyLeg[];
};

export function LegsTable({ legs }: Props) {
  return (
    <section className="panel">
      <div className="panel-header">
        <p className="eyebrow">Legs</p>
        <h2>Predict positions to mint</h2>
      </div>

      <div className="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Type</th>
              <th>Role</th>
              <th>Strike / range</th>
              <th>Quantity</th>
              <th>Premium</th>
            </tr>
          </thead>
          <tbody>
            {legs.map((leg, idx) => {
              const badge = KIND_BADGES[leg.kind] ?? { label: leg.kind, cls: "kind-pill" };
              return (
                <tr key={`${leg.kind}-${leg.role}-${idx}`}>
                  <td>
                    <span className={badge.cls}>{badge.label}</span>
                  </td>
                  <td>{ROLE_LABELS[leg.role] ?? leg.role}</td>
                  <td className="mono">{strikeText(leg)}</td>
                  <td className="mono">{formatCompactNumber(leg.quantityDisplay)}</td>
                  <td className="mono">{formatDusdcDisplayString(leg.premiumDisplay)}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </section>
  );
}
