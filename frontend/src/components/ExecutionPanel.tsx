"use client";

import { StatusStepper } from "@/components/StatusStepper";
import type { ExecutionStage, StageStatus } from "@/types/structx";

type Props = {
  stages: Record<ExecutionStage, StageStatus>;
  walletConnected: boolean;
  testnet: boolean;
  hasManager: boolean;
  managerBalanceVerified: boolean;
  managerBalanceText: string | null;
  compiled: boolean;
  premiumOk: boolean;
  budgetOk: boolean;
  canDryRun: boolean;
  canOpen: boolean;
  dryRunning: boolean;
  opening: boolean;
  onDryRun: () => void;
  onOpen: () => void;
  onRefreshBalance: () => void;
  disabledReason: string | null;
};

type CheckEntry = {
  ok: boolean;
  text: string;
};

export function ExecutionPanel({
  stages,
  walletConnected,
  testnet,
  hasManager,
  managerBalanceVerified,
  managerBalanceText,
  compiled,
  premiumOk,
  budgetOk,
  canDryRun,
  canOpen,
  dryRunning,
  opening,
  onDryRun,
  onOpen,
  onRefreshBalance,
  disabledReason,
}: Props) {
  const checks: CheckEntry[] = [
    { ok: walletConnected, text: "Wallet connected" },
    { ok: testnet, text: "Sui Testnet selected" },
    { ok: hasManager, text: "PredictManager ID set" },
    {
      ok: managerBalanceVerified,
      text: managerBalanceText
        ? `Manager balance verified: ${managerBalanceText}`
        : "Manager balance verified",
    },
    { ok: compiled, text: "Strategy compiled" },
    { ok: budgetOk, text: "Budget covers premium" },
    { ok: premiumOk, text: "Wallet and manager can cover the premium" },
  ];

  return (
    <section className="panel">
      <div className="panel-header">
        <p className="eyebrow">Execution</p>
        <h2>Open strategy</h2>
        <p className="muted">
          StructX prepares and checks the complete transaction. Your wallet
          then shows it for review and approval.
        </p>
      </div>

      <StatusStepper stages={stages} />

      <div className="execution-checks">
        {checks.map((c) => (
          <div key={c.text} className={c.ok ? "check-line ok" : "check-line bad"}>
            <span>{c.ok ? "✓" : "!"}</span>
            <p>{c.text}</p>
          </div>
        ))}
      </div>

      <div className="execution-actions">
        <button
          type="button"
          className="secondary-button"
          onClick={onRefreshBalance}
          disabled={!hasManager}
        >
          Refresh manager balance
        </button>

        <button
          type="button"
          className="secondary-button"
          onClick={onDryRun}
          disabled={!canDryRun || dryRunning || opening}
        >
          {dryRunning ? "Checking transaction…" : "Check transaction"}
        </button>

        <button
          type="button"
          className="primary-button"
          onClick={onOpen}
          disabled={!canOpen || opening}
        >
          {opening ? "Waiting for wallet…" : "Open strategy"}
        </button>
      </div>

      {disabledReason && !canOpen && (
        <p className="hint">To continue, {disabledReason}</p>
      )}

      <p className="hint">
        This check uses the current market and balance without moving funds.
      </p>
    </section>
  );
}
