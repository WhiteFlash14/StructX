"use client";

import { EmptyState } from "@/components/common/EmptyState";
import { StrategyCard } from "@/components/strategy/StrategyCard";
import type { StrategyCatalogEntry } from "@/lib/strategyCatalog";

type Props = {
  entries: StrategyCatalogEntry[];
  activeId: string | null;
  onSelect: (id: string) => void;
  query: string;
  onClearFilters: () => void;
};

export function StrategyGrid({
  entries,
  activeId,
  onSelect,
  query,
  onClearFilters,
}: Props) {
  if (entries.length === 0) {
    return (
      <EmptyState
        title={
          query
            ? "No strategies match your search"
            : "No live strategies in this category yet"
        }
        body={
          query
            ? "Try a different keyword, or clear the filters."
            : "We are working on more payoff types. Clear filters to see what's available now."
        }
        action={
          <button type="button" className="primary-button compact" onClick={onClearFilters}>
            Clear filters
          </button>
        }
      />
    );
  }

  return (
    <div className="strategy-grid">
      {entries.map((entry) => (
        <StrategyCard
          key={entry.id}
          entry={entry}
          active={entry.id === activeId}
          onSelect={onSelect}
        />
      ))}
      {/* Filler card showing "more coming" */}
      <div className="strategy-card filler" aria-hidden>
        <div className="filler-inner">
          <p className="filler-eyebrow">Roadmap</p>
          <p className="filler-text">
            More strategies coming as StructX matures on DeepBook Predict.
          </p>
        </div>
      </div>
    </div>
  );
}
