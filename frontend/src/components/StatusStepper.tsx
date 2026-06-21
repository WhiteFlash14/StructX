"use client";

import type { ExecutionStage, StageStatus } from "@/types/structx";

const STAGES: { id: ExecutionStage; label: string }[] = [
  { id: "configure", label: "Configure" },
  { id: "preview", label: "Preview" },
  { id: "preflight", label: "Balance check" },
  { id: "dryRun", label: "Transaction check" },
  { id: "signature", label: "Wallet approval" },
  { id: "submitted", label: "Submitted" },
  { id: "audited", label: "Verified" },
];

type Props = {
  stages: Record<ExecutionStage, StageStatus>;
};

const STATUS_GLYPH: Record<StageStatus, string> = {
  pending: "·",
  active: "•",
  success: "✓",
  failed: "!",
};

export function StatusStepper({ stages }: Props) {
  return (
    <ol className="stepper">
      {STAGES.map((stage) => {
        const status = stages[stage.id];
        return (
          <li key={stage.id} className={`stepper-item ${status}`}>
            <span className="stepper-dot" aria-hidden>
              {STATUS_GLYPH[status]}
            </span>
            <span className="stepper-label">{stage.label}</span>
          </li>
        );
      })}
    </ol>
  );
}
