"use client";

import { ExecutionStepper } from "@/components/execution/ExecutionStepper";
import {
  PreflightChecklist,
  type CheckItem,
} from "@/components/execution/PreflightChecklist";
import type { ExecutionStage, StageStatus } from "@/types/structx";

type Phase = "preflight" | "ready-dryrun" | "ready-open" | "running" | "done";

type Props = {
  stages: Record<ExecutionStage, StageStatus>;
  checks: CheckItem[];
  phase: Phase;
  dryRunning: boolean;
  opening: boolean;
  onDryRun: () => void;
  onOpen: () => void;
  onRefreshBalance: () => void;
  disabledReason: string | null;
};

export function ExecutionPanel({
  stages,
  checks,
  phase,
  dryRunning,
  opening,
  onDryRun,
  onOpen,
  onRefreshBalance,
  disabledReason,
}: Props) {
  return (
    <section className="panel">
      <div className="panel-header">
        <p className="eyebrow">Execution</p>
        <h2>Open strategy</h2>
        <p className="muted">
          StructX prepares the transaction using your selected PredictManager,
          then your wallet shows the details for approval.
        </p>
      </div>

      <ExecutionStepper stages={stages} />

      <div className="exec-split">
        <PreflightChecklist items={checks} />

        <div className="exec-actions">
          <button
            type="button"
            className="secondary-button"
            onClick={onRefreshBalance}
          >
            Refresh selected manager balance
          </button>

          {phase === "preflight" && (
            <button type="button" className="primary-button" disabled>
              Complete the checks above
            </button>
          )}

          {(phase === "ready-dryrun" || phase === "running") && (
            <button
              type="button"
              className="primary-button"
              onClick={onDryRun}
              disabled={dryRunning || opening}
            >
              {dryRunning ? "Checking transaction…" : "Check transaction"}
            </button>
          )}

          {(phase === "ready-open" || phase === "done") && (
            <>
              <button
                type="button"
                className="secondary-button"
                onClick={onDryRun}
                disabled={dryRunning || opening}
              >
                {dryRunning ? "Checking transaction…" : "Check again"}
              </button>
              <button
                type="button"
                className="primary-button"
                onClick={onOpen}
                disabled={opening}
              >
                {opening ? "Waiting for wallet…" : "Review and open strategy"}
              </button>
            </>
          )}

          {disabledReason && phase === "preflight" && (
            <p className="hint">{disabledReason}</p>
          )}
        </div>
      </div>
    </section>
  );
}
