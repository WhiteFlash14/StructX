"use client";

import type { ExecutionStage, StageStatus } from "@/types/structx";

const STAGES: { id: ExecutionStage; label: string; sub: string }[] = [
  { id: "configure", label: "Configure", sub: "Set your inputs" },
  { id: "preview", label: "Preview", sub: "Build the payoff" },
  { id: "preflight", label: "Balance check", sub: "Check wallet and manager" },
  { id: "dryRun", label: "Transaction check", sub: "Test the full transaction" },
  { id: "signature", label: "Wallet approval", sub: "Review in your wallet" },
  { id: "submitted", label: "Submitted", sub: "Wait for confirmation" },
  { id: "audited", label: "Verified", sub: "Confirm the opened positions" },
];

type Props = {
  stages: Record<ExecutionStage, StageStatus>;
};

export function ExecutionStepper({ stages }: Props) {
  return (
    <ol className="stepper">
      {STAGES.map((stage, idx) => {
        const status = stages[stage.id];
        const isLast = idx === STAGES.length - 1;
        return (
          <li key={stage.id} className={`stepper-step ${status}`}>
            <span className="stepper-bubble" aria-hidden>
              {status === "success" ? "✓" : status === "failed" ? "!" : idx + 1}
            </span>
            <span className="stepper-text">
              <strong>{stage.label}</strong>
              <span>{stage.sub}</span>
            </span>
            {!isLast && <span className="stepper-bar" aria-hidden />}
          </li>
        );
      })}
    </ol>
  );
}
