"use client";

import { ROLE_LABELS } from "@/lib/format";
import type { StrategyLeg } from "@/types/structx";

const KIND_BADGES: Record<string, { label: string; cls: string }> = {
  DOWN: { label: "Extreme downside", cls: "kind-pill down" },
  UP: { label: "Extreme upside", cls: "kind-pill up" },
  RANGE: { label: "Range", cls: "kind-pill range" },
};

function legBadge(leg: StrategyLeg) {
  if (leg.kind === "RANGE") {
    if (leg.role === "moderate_downside") {
      return { label: "Moderate downside", cls: "kind-pill range" };
    }
    if (leg.role === "moderate_upside") {
      return { label: "Moderate upside", cls: "kind-pill range" };
    }
  }
  return KIND_BADGES[leg.kind] ?? { label: leg.kind, cls: "kind-pill" };
}

function strikeText(leg: StrategyLeg) {
  if (leg.kind === "RANGE") {
    return `${leg.lower ?? "Unavailable"} to ${leg.upper ?? "Unavailable"}`;
  }
  return leg.strike ?? "Unavailable";
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
              <th>Ask price raw</th>
              <th>Premium</th>
              <th>Max slippage cost</th>
            </tr>
          </thead>
          <tbody>
            {legs.map((leg, idx) => {
              const badge = legBadge(leg);
              return (
                <tr key={`${leg.kind}-${leg.role}-${idx}`}>
                  <td>
                    <span className={badge.cls}>{badge.label}</span>
                  </td>
                  <td>{ROLE_LABELS[leg.role] ?? leg.role}</td>
                  <td className="mono">{strikeText(leg)}</td>
                  <td className="mono">{leg.quantityDisplay}</td>
                  <td className="mono dim">{leg.askPriceRaw}</td>
                  <td className="mono">{leg.premiumDisplay}</td>
                  <td className="mono dim">{leg.maxCostRaw ?? "Unavailable"}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </section>
  );
}
