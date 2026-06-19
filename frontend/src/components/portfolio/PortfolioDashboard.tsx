"use client";

import { useMemo } from "react";

import { CopyButton } from "@/components/common/CopyButton";
import { EmptyState } from "@/components/common/EmptyState";
import { StatusPill } from "@/components/common/StatusPill";
import {
  formatDate,
  formatDusdcDisplay,
  formatDusdcDisplayString,
  shortAddress,
} from "@/lib/format";
import type { ManagerBalanceResponse, PortfolioTradeRecord } from "@/types/structx";

type Props = {
  connectedAddress: string | null;
  managerId: string;
  managerBalance: ManagerBalanceResponse | null;
  managerBalanceLoading: boolean;
  history: PortfolioTradeRecord[];
  query: string;
  onShowStrategies: () => void;
  onCopied: (label: string) => void;
};

function sumRaw(records: PortfolioTradeRecord[], pick: (record: PortfolioTradeRecord) => string | undefined) {
  return records.reduce((acc, record) => {
    const raw = pick(record);
    if (!raw) return acc;
    try {
      return acc + BigInt(raw);
    } catch {
      return acc;
    }
  }, 0n);
}

function legLabel(record: PortfolioTradeRecord): string[] {
  if (!record.mintedLegs.length) return [`${record.legCount} legs`];
  return record.mintedLegs.slice(0, 4).map((leg) =>
    leg.kind === "RANGE"
      ? `${leg.kind} ${leg.lower ?? "?"}–${leg.upper ?? "?"}`
      : `${leg.kind} ${leg.strike ?? "?"}`,
  );
}

export function PortfolioDashboard({
  connectedAddress,
  managerId,
  managerBalance,
  managerBalanceLoading,
  history,
  query,
  onShowStrategies,
  onCopied,
}: Props) {
  const filteredHistory = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return history;
    return history.filter((record) =>
      [
        record.displayName,
        record.digest,
        record.managerId,
        record.strategy,
        record.requestedStrategy,
        ...(record.categories ?? []),
      ]
        .join(" ")
        .toLowerCase()
        .includes(q),
    );
  }, [history, query]);

  const premiumDeployedRaw = sumRaw(filteredHistory, (record) => record.premiumPaidRaw);
  const maxPayoutRaw = sumRaw(filteredHistory, (record) => record.maxGrossPayoutRaw);
  const bestCasePnlRaw = sumRaw(filteredHistory, (record) => record.maxNetPayoutRaw);
  const currentBookValueRaw = premiumDeployedRaw;

  if (!connectedAddress) {
    return (
      <section className="section">
        <div className="section-head">
          <h2>Positions & trades</h2>
        </div>
        <EmptyState
          title="Connect your wallet to view positions"
          body="Once you open strategies from StructX, this workspace will track your recent baskets, execution receipts, and manager cash."
        />
      </section>
    );
  }

  return (
    <section className="section">
      <div className="section-head portfolio-head">
        <div>
          <h2>Positions & trades</h2>
          <p className="muted portfolio-head-sub">
            Current value is shown at cost basis until live mark-to-market lands.
          </p>
        </div>
        <button type="button" className="ghost-button" onClick={onShowStrategies}>
          Back to strategy library
        </button>
      </div>

      <div className="portfolio-stat-grid">
        <div className="portfolio-stat-card">
          <span>Open baskets</span>
          <strong>{filteredHistory.length}</strong>
          <small>Recent StructX executions for this wallet</small>
        </div>
        <div className="portfolio-stat-card">
          <span>Premium deployed</span>
          <strong>{formatDusdcDisplay(premiumDeployedRaw.toString())}</strong>
          <small>Total cost basis across tracked positions</small>
        </div>
        <div className="portfolio-stat-card">
          <span>Current value</span>
          <strong>{formatDusdcDisplay(currentBookValueRaw.toString())}</strong>
          <small>Cost basis until live marks are available</small>
        </div>
        <div className="portfolio-stat-card">
          <span>Funding cash</span>
          <strong>
            {managerBalanceLoading
              ? "Checking…"
              : managerBalance?.ok
                ? formatDusdcDisplayString(managerBalance.balanceDisplay)
                : "Unavailable"}
          </strong>
          <small>{managerId ? "Linked funding manager" : "No funding manager linked"}</small>
        </div>
        <div className="portfolio-stat-card">
          <span>Max gross payout</span>
          <strong>{formatDusdcDisplay(maxPayoutRaw.toString())}</strong>
          <small>Best-case payout across tracked baskets</small>
        </div>
        <div className="portfolio-stat-card">
          <span>Best-case P/L</span>
          <strong className={bestCasePnlRaw > 0n ? "stat-pos" : bestCasePnlRaw < 0n ? "stat-neg" : ""}>
            {formatDusdcDisplay(bestCasePnlRaw.toString())}
          </strong>
          <small>Potential net upside if every basket maxes out</small>
        </div>
      </div>

      {filteredHistory.length === 0 ? (
        <EmptyState
          title={query ? "No positions match your search" : "No positions yet"}
          body={
            query
              ? "Try a different keyword, digest, or strategy name."
              : "Open a strategy from StructX and your recent baskets will appear here."
          }
          action={
            <button type="button" className="primary-button compact" onClick={onShowStrategies}>
              Browse strategies
            </button>
          }
        />
      ) : (
        <>
          <div className="portfolio-card-grid">
            {filteredHistory.map((record) => (
              <article className="portfolio-card" key={record.id}>
                <div className="portfolio-card-top">
                  <div>
                    <p className="portfolio-card-kicker">{formatDate(record.openedAt)}</p>
                    <h3>{record.displayName}</h3>
                    <p className="portfolio-card-sub">
                      {record.requestedStrategy && record.requestedStrategy !== record.strategy
                        ? "Opened via Smart Budget Selector"
                        : "Opened directly from StructX"}
                    </p>
                  </div>
                  <StatusPill
                    label={record.auditOk ? "Tracked" : "Needs review"}
                    tone={record.auditOk ? "ok" : "warn"}
                    dot={record.auditOk}
                  />
                </div>

                <div className="portfolio-pill-row">
                  {(record.categories ?? []).map((category) => (
                    <span key={category} className="portfolio-pill">
                      {category}
                    </span>
                  ))}
                  {record.expiry && (
                    <span className="portfolio-pill subtle">
                      Expiry {new Date(record.expiry).toLocaleDateString()}
                    </span>
                  )}
                </div>

                <div className="portfolio-card-metrics">
                  <div>
                    <span>Premium paid</span>
                    <strong>{formatDusdcDisplayString(record.premiumPaidDisplay)}</strong>
                  </div>
                  <div>
                    <span>Current value</span>
                    <strong>{formatDusdcDisplayString(record.premiumPaidDisplay)}</strong>
                  </div>
                  <div>
                    <span>Max payout</span>
                    <strong>{formatDusdcDisplayString(record.maxGrossPayoutDisplay)}</strong>
                  </div>
                  <div>
                    <span>Best-case P/L</span>
                    <strong className="metric-pos">
                      {formatDusdcDisplayString(record.maxNetPayoutDisplay)}
                    </strong>
                  </div>
                </div>

                <div className="portfolio-leg-row">
                  {legLabel(record).map((label) => (
                    <span key={label} className="portfolio-leg-pill">
                      {label}
                    </span>
                  ))}
                </div>

                <div className="portfolio-card-foot">
                  <div>
                    <span>Digest</span>
                    <strong>{shortAddress(record.digest)}</strong>
                  </div>
                  <div className="portfolio-card-actions">
                    <CopyButton
                      value={record.digest}
                      label="Copy digest"
                      onCopied={() => onCopied("transaction digest")}
                    />
                    {record.explorerUrl && (
                      <a
                        href={record.explorerUrl}
                        target="_blank"
                        rel="noreferrer noopener"
                        className="ghost-button"
                      >
                        Explorer
                      </a>
                    )}
                  </div>
                </div>
              </article>
            ))}
          </div>

          <div className="portfolio-trades-panel">
            <div className="section-head">
              <h2>Recent trades</h2>
              <p className="muted">
                {filteredHistory.length} {filteredHistory.length === 1 ? "trade" : "trades"}
              </p>
            </div>

            <div className="portfolio-trade-list">
              {filteredHistory.map((record) => (
                <div className="portfolio-trade-row" key={`${record.id}-row`}>
                  <div className="portfolio-trade-main">
                    <strong>{record.displayName}</strong>
                    <span>{formatDate(record.openedAt)}</span>
                  </div>
                  <div className="portfolio-trade-metric">
                    <span>Premium</span>
                    <strong>{formatDusdcDisplayString(record.premiumPaidDisplay)}</strong>
                  </div>
                  <div className="portfolio-trade-metric">
                    <span>Max payout</span>
                    <strong>{formatDusdcDisplayString(record.maxGrossPayoutDisplay)}</strong>
                  </div>
                  <div className="portfolio-trade-metric">
                    <span>Status</span>
                    <strong>{record.executionStatus ?? (record.auditOk ? "tracked" : "unknown")}</strong>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </>
      )}
    </section>
  );
}
