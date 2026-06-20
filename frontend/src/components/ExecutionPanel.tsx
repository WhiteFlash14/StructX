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
    { ok: premiumOk, text: "Manager balance covers premium" },
  ];

  return (
    <section className="panel">
      <div className="panel-header">
        <p className="eyebrow">Execution</p>
        <h2>Open strategy</h2>
        <p className="muted">
          StructX builds a wallet-signed PTB. Your wallet asks for signature.
          StructX never receives private keys and never custodies funds.
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
          {dryRunning ? "Dry-running…" : "Dry-run transaction"}
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
        <p className="hint">Why disabled? {disabledReason}</p>
      )}

      <p className="hint">
        Dry-run confirms the transaction shape before the wallet signature.
      </p>
    </section>
  );
}
