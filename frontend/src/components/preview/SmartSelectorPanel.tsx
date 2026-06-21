"use client";

import { formatDusdcDisplay, formatNumberTwoDecimals } from "@/lib/format";
import { strategyDisplayName } from "@/lib/strategyCatalog";
import type { SmartSelectorInfo } from "@/types/structx";

type Props = {
  info: SmartSelectorInfo;
};

export function SmartSelectorPanel({ info }: Props) {
  return (
    <section className="panel">
      <div className="panel-header">
        <p className="eyebrow">Smart Selector</p>
        <h2>Selected {strategyDisplayName(info.winner)}</h2>
        <p className="muted">
          StructX compared {info.candidateCount} executable candidates and chose
          the highest scoring strategy for this budget and style.
        </p>
      </div>

      <div className="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Strategy</th>
              <th>Score</th>
              <th>Premium</th>
              <th>Max payout</th>
              <th>Expected payout</th>
              <th>Hit prob</th>
            </tr>
          </thead>
          <tbody>
            {info.alternatives.map((candidate) => (
              <tr key={candidate.strategy}>
                <td>{strategyDisplayName(candidate.strategy)}</td>
                <td>{candidate.scoreE6}</td>
                <td>{formatDusdcDisplay(candidate.premiumRaw)}</td>
                <td>{formatDusdcDisplay(candidate.maxPayoutRaw)}</td>
                <td>{formatDusdcDisplay(candidate.expectedPayoutRaw)}</td>
                <td>
                  {formatNumberTwoDecimals(candidate.hitProbabilityBps / 100)}%
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}
