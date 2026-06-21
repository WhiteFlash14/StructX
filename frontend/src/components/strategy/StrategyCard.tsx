"use client";

import { StatusPill } from "@/components/common/StatusPill";
import type { StrategyCatalogEntry } from "@/lib/strategyCatalog";

type Props = {
  entry: StrategyCatalogEntry;
  active?: boolean;
  onSelect: (id: string) => void;
};

export function StrategyCard({ entry, active, onSelect }: Props) {
  const isRecommended = entry.status === "recommended";
  return (
    <article
      className={`strategy-card accent-${entry.accent} ${active ? "active" : ""} ${
        isRecommended ? "live" : "beta"
      }`}
    >
      <header className="strategy-card-head">
        <div className="strategy-card-id">
          <span className="strategy-card-glyph" aria-hidden>
            {entry.name.charAt(0)}
          </span>
          <div>
            <h3 className="strategy-card-title">{entry.displayName}</h3>
            <p className="strategy-card-desc">{entry.oneLiner}</p>
          </div>
        </div>
        <StatusPill
          label={isRecommended ? "Recommended" : "Beta"}
          tone={isRecommended ? "live" : "warn"}
          dot={isRecommended}
        />
      </header>

      <div className="strategy-card-tags">
        {entry.categories.map((c) => (
          <span key={c} className="strategy-card-tag">
            {c}
          </span>
        ))}
      </div>

      <div className="strategy-card-meta">
        <div>
          <span className="strategy-meta-label">Legs</span>
          <strong className="strategy-meta-value muted-strong">
            {entry.legHint}
          </strong>
        </div>
      </div>

      <div className="strategy-card-actions">
        <button
          type="button"
          className={`poly-button ${isRecommended ? "poly-yes" : "poly-beta"}`}
          onClick={() => onSelect(entry.id)}
          aria-pressed={Boolean(active)}
        >
          {active ? "Selected ✓" : "Build strategy"}
        </button>
      </div>
    </article>
  );
}
