"use client";

import { classifyWarning, type Severity } from "@/lib/errors";

type WarningItem = {
  text: string;
  severity?: Severity;
};

type Props = {
  warnings: Array<string | WarningItem>;
};

const SEVERITY_LABEL: Record<Severity, string> = {
  info: "Info",
  caution: "Caution",
  blocking: "Blocking",
};

const SEVERITY_ORDER: Severity[] = ["blocking", "caution", "info"];

export function WarningsPanel({ warnings }: Props) {
  if (!warnings.length) return null;

  const normalized = warnings.map((entry) => {
    if (typeof entry === "string") {
      return { text: entry, severity: classifyWarning(entry) };
    }
    return {
      text: entry.text,
      severity: entry.severity ?? classifyWarning(entry.text),
    };
  });

  const grouped: Record<Severity, typeof normalized> = {
    blocking: [],
    caution: [],
    info: [],
  };
  for (const w of normalized) {
    grouped[w.severity].push(w);
  }

  return (
    <section className="panel">
      <div className="panel-header">
        <p className="eyebrow">Risk controls</p>
        <h2>Warnings</h2>
      </div>

      <div className="warnings">
        {SEVERITY_ORDER.flatMap((sev) =>
          grouped[sev].map((w, idx) => (
            <div
              key={`${sev}-${idx}-${w.text.slice(0, 30)}`}
              className={`warning-item severity-${sev}`}
            >
              <span className="warning-tag">{SEVERITY_LABEL[sev]}</span>
              <p>{w.text}</p>
            </div>
          )),
        )}
      </div>
    </section>
  );
}
