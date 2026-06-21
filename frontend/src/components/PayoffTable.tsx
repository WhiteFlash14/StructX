"use client";

import { bigIntSafe } from "@/lib/format";
import type { PayoffRow } from "@/types/structx";

type Props = {
  rows: PayoffRow[];
};

export function PayoffTable({ rows }: Props) {
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
                  <td>{row.condition}</td>
                  <td className="mono">{row.grossPayoutDisplay}</td>
                  <td className={`mono ${cls}`}>{row.netPnlDisplay}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </section>
  );
}
