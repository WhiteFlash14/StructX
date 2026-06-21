"use client";

import {
  bigIntSafe,
  formatDusdcDisplayString,
  formatPriceCompact,
} from "@/lib/format";
import type { PayoffRow, StrikesBundle } from "@/types/structx";

type Props = {
  rows: PayoffRow[];
  strikes: StrikesBundle;
};

export function PayoffTable({ rows, strikes }: Props) {
  const scenarioLabels: Record<string, string> = {
    "BTC settles <= K1": `BTC settles at or below ${formatPriceCompact(strikes.k1)}`,
    "K1 < BTC settles <= K2": `${formatPriceCompact(strikes.k1)} to ${formatPriceCompact(strikes.k2)}`,
    "K2 < BTC settles < K3": `${formatPriceCompact(strikes.k2)} to ${formatPriceCompact(strikes.k3)}`,
    "K3 <= BTC settles < K4": `${formatPriceCompact(strikes.k3)} to ${formatPriceCompact(strikes.k4)}`,
    "BTC settles >= K4": `BTC settles at or above ${formatPriceCompact(strikes.k4)}`,
  };

  return (
    <section className="panel">
      <div className="panel-header">
        <p className="eyebrow">Payoff</p>
        <h2>Scenario payoff</h2>
        <p className="muted">Net PnL is gross payout minus premium paid.</p>
      </div>

      <div className="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Scenario</th>
              <th>Gross payout</th>
              <th>Net PnL</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => {
              const net = bigIntSafe(row.netPnlRaw);
              const cls =
                net === null ? "" : net > 0n ? "pnl-up" : net < 0n ? "pnl-down" : "";
              return (
                <tr key={row.condition}>
                  <td>{scenarioLabels[row.condition] ?? row.condition}</td>
                  <td className="mono">{formatDusdcDisplayString(row.grossPayoutDisplay)}</td>
                  <td className={`mono ${cls}`}>{formatDusdcDisplayString(row.netPnlDisplay)}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </section>
  );
}
