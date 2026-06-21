// Client wrapper for /strategies. Holds the Normal/Advanced toggle state,
// search/filter/sort state for the Advanced shelf, and renders the chosen
// panel. Keeping the route's page.tsx as a Server Component preserves the
// route metadata export.

"use client";

import Link from "next/link";
import { useCallback, useEffect, useMemo, useState } from "react";

import {
  NewModeToggle,
  type NewAppMode,
} from "@/components/landing/NewModeToggle";
import { NormalModeIntentPanel } from "@/components/intent/NormalModeIntentPanel";
import { STRATEGY_CATALOG, type StrategyCatalogEntry } from "@/lib/strategyCatalog";
import type { StrategyId } from "@/types/structx";

// Filter taxonomy for the Advanced shelf. Each filter maps to a predicate
// over the catalog entry. We reuse the existing `categories` metadata and
// `status` flag so no per-strategy code changes — just filtering.
type ShelfFilter =
  | "all"
  | "featured"
  | "directional"
  | "range"
  | "breakout"
  | "protection"
  | "notes";

type SortKey = "recommended" | "budget" | "name";

const SHELF_FILTERS: ReadonlyArray<{
  id: ShelfFilter;
  label: string;
  match: (e: StrategyCatalogEntry) => boolean;
}> = [
  { id: "all", label: "All", match: () => true },
  { id: "featured", label: "Featured", match: (e) => e.status === "recommended" },
  {
    id: "directional",
    label: "Directional",
    match: (e) => e.categories.some((c) => c === "Upside" || c === "Downside"),
  },
  { id: "range", label: "Range", match: (e) => e.categories.includes("Range") },
  { id: "breakout", label: "Breakout", match: (e) => e.categories.includes("Breakout") },
  { id: "protection", label: "Protection", match: (e) => e.categories.includes("Protection") },
  { id: "notes", label: "Notes", match: (e) => e.categories.includes("Notes") },
];

// localStorage key. Kept separate from `structx.mode` which the legacy
// /app workspace uses, so toggling on the new frontend doesn't perturb the
// legacy persistence and vice versa.
const NEW_MODE_KEY = "structx.newmode";

// Risk tier per strategy, used to render a three-dot meter on each card.
// 1 = selector / lowest, 2 = defined-risk standard, 3 = convex-tail leaning.
const RISK_TIER: Record<StrategyId, 1 | 2 | 3> = {
  SMART_BUDGET_SELECTOR: 1,
  PORTFOLIO_CRASH_SHIELD: 2,
  BREAKOUT_PROTECTION: 2,
  EXPIRY_MOVE_NOTE: 2,
  CENTER_BAND_CONDOR: 2,
  UPSIDE_STEP_LADDER: 2,
  DOWNSIDE_STEP_LADDER: 2,
  RANGE_CONVICTION: 2,
  CONVEX_TAIL_LADDER: 3,
  MOONSHOT_UPSIDE: 3,
  DOWNSIDE_CONVEXITY: 3,
  NEAR_BARRIER_PROXY: 3,
};

const RISK_LABEL: Record<1 | 2 | 3, string> = {
  1: "Selector",
  2: "Defined",
  3: "Convex",
};

// Approximate primitive count per strategy for the footer chip.
const LEG_COUNT: Record<StrategyId, number | null> = {
  BREAKOUT_PROTECTION: 4,
  SMART_BUDGET_SELECTOR: null,
  MOONSHOT_UPSIDE: 2,
  DOWNSIDE_CONVEXITY: 2,
  UPSIDE_STEP_LADDER: 3,
  DOWNSIDE_STEP_LADDER: 3,
  CENTER_BAND_CONDOR: 4,
  NEAR_BARRIER_PROXY: 2,
  PORTFOLIO_CRASH_SHIELD: 3,
  EXPIRY_MOVE_NOTE: 4,
  CONVEX_TAIL_LADDER: 4,
  RANGE_CONVICTION: 2,
};

function parseBudgetMin(entry: StrategyCatalogEntry): number {
  const m = entry.fromBudgetDusdc.match(/(\d+)/);
  return m ? Number(m[1]) : 0;
}

function strippedBudget(entry: StrategyCatalogEntry): string {
  return entry.fromBudgetDusdc.replace(/^from\s+/i, "");
}

function StrategyGlyph({ id }: { id: StrategyId }) {
  const common = {
    width: 22,
    height: 22,
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.6,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
  };
  switch (id) {
    case "BREAKOUT_PROTECTION":
      return (
        <svg {...common} aria-hidden>
          <path d="M4 12h4l3-7 4 14 3-7h2" />
        </svg>
      );
    case "SMART_BUDGET_SELECTOR":
      return (
        <svg {...common} aria-hidden>
          <path d="M12 3l1.6 4.4L18 9l-4.4 1.6L12 15l-1.6-4.4L6 9l4.4-1.6L12 3z" />
          <path d="M18 16l.7 1.8L20.5 18l-1.8.7L18 20.5l-.7-1.8L15.5 18l1.8-.7L18 16z" />
        </svg>
      );
    case "MOONSHOT_UPSIDE":
      return (
        <svg {...common} aria-hidden>
          <path d="M5 19l14-14" />
          <path d="M9 5h10v10" />
        </svg>
      );
    case "DOWNSIDE_CONVEXITY":
      return (
        <svg {...common} aria-hidden>
          <path d="M5 5l14 14" />
          <path d="M19 9v10H9" />
        </svg>
      );
    case "UPSIDE_STEP_LADDER":
      return (
        <svg {...common} aria-hidden>
          <path d="M4 19h4v-4h4v-4h4v-4h4" />
        </svg>
      );
    case "DOWNSIDE_STEP_LADDER":
      return (
        <svg {...common} aria-hidden>
          <path d="M4 5h4v4h4v4h4v4h4" />
        </svg>
      );
    case "CENTER_BAND_CONDOR":
      return (
        <svg {...common} aria-hidden>
          <path d="M3 16h4l3-8h4l3 8h4" />
        </svg>
      );
    case "NEAR_BARRIER_PROXY":
      return (
        <svg {...common} aria-hidden>
          <path d="M4 19h6V8h4v11h6" />
          <path d="M4 4v15M20 4v15" strokeOpacity="0.45" />
        </svg>
      );
    case "PORTFOLIO_CRASH_SHIELD":
      return (
        <svg {...common} aria-hidden>
          <path d="M12 3l8 3v6c0 5-3.4 8.3-8 9-4.6-.7-8-4-8-9V6l8-3z" />
          <path d="M9 12l2 2 4-4" />
        </svg>
      );
    case "EXPIRY_MOVE_NOTE":
      return (
        <svg {...common} aria-hidden>
          <path d="M3 12h4M17 12h4" />
          <path d="M7 8l-4 4 4 4" />
          <path d="M17 8l4 4-4 4" />
          <circle cx="12" cy="12" r="1.2" />
        </svg>
      );
    case "CONVEX_TAIL_LADDER":
      return (
        <svg {...common} aria-hidden>
          <path d="M4 19h4v-5h4V9h4V4h4" />
        </svg>
      );
    default:
      return null;
  }
}

// Mini payoff thumbnail rendered behind the card body. Each strategy has a
// distinct silhouette so users can recognise a payoff shape at a glance.
// viewBox is 120x44; payoff baseline at y=38, peaks around y=6.
function PayoffThumb({ id }: { id: StrategyId }) {
  const props = {
    viewBox: "0 0 120 44",
    width: "100%",
    height: 44,
    preserveAspectRatio: "none" as const,
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.6,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
  };
  const baseline = (
    <line
      x1="0"
      y1="38"
      x2="120"
      y2="38"
      stroke="currentColor"
      strokeOpacity="0.16"
      strokeWidth="1"
    />
  );
  switch (id) {
    case "BREAKOUT_PROTECTION":
      return (
        <svg {...props} aria-hidden>
          {baseline}
          <polyline points="0,8 22,38 52,30 68,30 98,38 120,8" />
        </svg>
      );
    case "PORTFOLIO_CRASH_SHIELD":
      return (
        <svg {...props} aria-hidden>
          {baseline}
          <polyline points="0,6 22,30 46,38 120,38" />
        </svg>
      );
    case "MOONSHOT_UPSIDE":
      return (
        <svg {...props} aria-hidden>
          {baseline}
          <polyline points="0,38 60,38 78,26 100,26 120,6" />
        </svg>
      );
    case "DOWNSIDE_CONVEXITY":
      return (
        <svg {...props} aria-hidden>
          {baseline}
          <polyline points="0,6 20,26 42,26 60,38 120,38" />
        </svg>
      );
    case "UPSIDE_STEP_LADDER":
      return (
        <svg {...props} aria-hidden>
          {baseline}
          <polyline points="0,38 32,38 32,28 64,28 64,18 96,18 96,8 120,8" />
        </svg>
      );
    case "DOWNSIDE_STEP_LADDER":
      return (
        <svg {...props} aria-hidden>
          {baseline}
          <polyline points="0,8 24,8 24,18 56,18 56,28 88,28 88,38 120,38" />
        </svg>
      );
    case "CENTER_BAND_CONDOR":
      return (
        <svg {...props} aria-hidden>
          {baseline}
          <polyline points="0,32 22,32 38,14 82,14 96,32 120,32" />
        </svg>
      );
    case "NEAR_BARRIER_PROXY":
      return (
        <svg {...props} aria-hidden>
          {baseline}
          <polyline points="0,38 72,38 86,8 96,8 110,38 120,38" />
        </svg>
      );
    case "EXPIRY_MOVE_NOTE":
      return (
        <svg {...props} aria-hidden>
          {baseline}
          <polyline points="0,10 30,28 60,38 90,28 120,10" />
        </svg>
      );
    case "CONVEX_TAIL_LADDER":
      return (
        <svg {...props} aria-hidden>
          {baseline}
          <polyline points="0,6 24,22 42,30 80,30 98,22 120,6" />
        </svg>
      );
    case "SMART_BUDGET_SELECTOR":
      return (
        <svg {...props} aria-hidden>
          {baseline}
          <rect x="22" y="22" width="14" height="16" rx="2" fill="currentColor" fillOpacity="0.18" stroke="none" />
          <rect x="50" y="14" width="14" height="24" rx="2" fill="currentColor" fillOpacity="0.32" stroke="none" />
          <rect x="78" y="26" width="14" height="12" rx="2" fill="currentColor" fillOpacity="0.18" stroke="none" />
          <polyline points="51,8 56,12 64,2" />
        </svg>
      );
    default:
      return null;
  }
}

function RiskMeter({ tier }: { tier: 1 | 2 | 3 }) {
  return (
    <span className="strategy-risk-meter" aria-label={`Risk profile: ${RISK_LABEL[tier]}`}>
      <span className={`strategy-risk-dot ${tier >= 1 ? "on" : ""}`} aria-hidden />
      <span className={`strategy-risk-dot ${tier >= 2 ? "on" : ""}`} aria-hidden />
      <span className={`strategy-risk-dot ${tier >= 3 ? "on" : ""}`} aria-hidden />
      <span className="strategy-risk-label">{RISK_LABEL[tier]}</span>
    </span>
  );
}

export function StrategiesView() {
  // Default to Normal Mode. Read the persisted choice only after mount so the
  // server-rendered HTML matches the first client paint and we don't trip a
  // hydration warning on toggle state.
  const [mode, setMode] = useState<NewAppMode>("normal");
  const [filter, setFilter] = useState<ShelfFilter>("all");
  const [query, setQuery] = useState<string>("");
  const [sort, setSort] = useState<SortKey>("recommended");

  useEffect(() => {
    try {
      const stored = window.localStorage.getItem(NEW_MODE_KEY);
      if (stored === "normal" || stored === "advanced") {
        setMode(stored);
      }
    } catch {
      // Private mode / storage disabled — silently fall back to the default.
    }
  }, []);

  const updateMode = useCallback((next: NewAppMode) => {
    setMode(next);
    try {
      window.localStorage.setItem(NEW_MODE_KEY, next);
    } catch {
      // ignore
    }
  }, []);

  const counts = useMemo(() => {
    const out: Record<ShelfFilter, number> = {
      all: 0,
      featured: 0,
      directional: 0,
      range: 0,
      breakout: 0,
      protection: 0,
      notes: 0,
    };
    for (const s of SHELF_FILTERS) {
      out[s.id] = STRATEGY_CATALOG.filter(s.match).length;
    }
    return out;
  }, []);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    const matcher = SHELF_FILTERS.find((s) => s.id === filter)?.match ?? (() => true);
    const rows = STRATEGY_CATALOG.filter(matcher).filter((entry) => {
      if (!q) return true;
      const haystack = [
        entry.name,
        entry.displayName,
        entry.oneLiner,
        entry.useCase,
        entry.legHint,
        entry.riskLabel,
        ...entry.categories,
      ]
        .join(" ")
        .toLowerCase();
      return haystack.includes(q);
    });
    return rows.sort((a, b) => {
      if (sort === "name") return a.displayName.localeCompare(b.displayName);
      if (sort === "budget") {
        const da = parseBudgetMin(a);
        const db = parseBudgetMin(b);
        if (da !== db) return da - db;
        return a.displayName.localeCompare(b.displayName);
      }
      // recommended: status===recommended first, then alpha
      if (a.status !== b.status) return a.status === "recommended" ? -1 : 1;
      return a.displayName.localeCompare(b.displayName);
    });
  }, [filter, query, sort]);

  const resetFilters = useCallback(() => {
    setFilter("all");
    setQuery("");
    setSort("recommended");
  }, []);

  return (
    <>
      <section className="strategies-hero">
        <p className="strategies-eyebrow">
          <span className="strategies-eyebrow-dot" aria-hidden />
          Payoff library
        </p>
        <h1>
          Defined-risk BTC strategies,{" "}
          <span className="accent">compiled on DeepBook Predict.</span>
        </h1>
        <p className="strategies-sub">
          Each template compiles into transparent Predict positions on Sui
          Testnet. Describe what you want, or pick a strategy directly.
        </p>

        <div className="new-mode-toggle-wrap">
          <NewModeToggle mode={mode} onChange={updateMode} />
        </div>
      </section>

      {/*
        key={mode} forces React to unmount the previous panel and mount the
        new one, which triggers the CSS animation-name on .mode-swap. That's
        what gives the segmented control its smooth crossfade instead of a
        hard cut. prefers-reduced-motion at the top of _landing-shared.tsx
        clamps the animation duration to 1ms for users who opt out.
      */}
      <div key={mode} className="mode-swap">
        {mode === "normal" ? (
          <section className="section strategies-grid-section">
            <NormalModeIntentPanel />
          </section>
        ) : (
          <section className="section strategies-grid-section">
            <div className="strategies-toolbar" role="region" aria-label="Filter strategies">
              <div className="strategies-toolbar-search">
                <svg
                  width="16"
                  height="16"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1.7"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  aria-hidden
                >
                  <circle cx="11" cy="11" r="7" />
                  <path d="M16.4 16.4L21 21" />
                </svg>
                <input
                  type="search"
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  placeholder="Search strategies, e.g. crash, breakout, range"
                  aria-label="Search strategies"
                />
                {query && (
                  <button
                    type="button"
                    className="strategies-toolbar-clear"
                    onClick={() => setQuery("")}
                    aria-label="Clear search"
                  >
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
                      <path d="M6 6l12 12M18 6L6 18" />
                    </svg>
                  </button>
                )}
              </div>

              <div className="strategies-filters" role="tablist" aria-label="Strategy categories">
                {SHELF_FILTERS.map((s) => (
                  <button
                    key={s.id}
                    type="button"
                    role="tab"
                    aria-selected={filter === s.id}
                    className={`strategies-filter ${filter === s.id ? "active" : ""}`}
                    onClick={() => setFilter(s.id)}
                  >
                    <span className="strategies-filter-label">{s.label}</span>
                    <span className="strategies-filter-count">{counts[s.id]}</span>
                  </button>
                ))}
              </div>

              <div className="strategies-sort">
                <label htmlFor="strategies-sort-select" className="strategies-sort-label">
                  Sort
                </label>
                <div className="strategies-sort-wrap">
                  <select
                    id="strategies-sort-select"
                    className="strategies-sort-select"
                    value={sort}
                    onChange={(e) => setSort(e.target.value as SortKey)}
                  >
                    <option value="recommended">Recommended</option>
                    <option value="budget">Lowest min size</option>
                    <option value="name">A – Z</option>
                  </select>
                  <svg
                    className="strategies-sort-caret"
                    width="12"
                    height="12"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="2"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    aria-hidden
                  >
                    <path d="M6 9l6 6 6-6" />
                  </svg>
                </div>
              </div>
            </div>

            <div className="strategies-result-bar">
              <span className="strategies-result-count">
                Showing <strong>{filtered.length}</strong> of {STRATEGY_CATALOG.length} strategies
              </span>
              {(filter !== "all" || query.trim() || sort !== "recommended") && (
                <button
                  type="button"
                  className="strategies-result-clear"
                  onClick={resetFilters}
                >
                  <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
                    <path d="M3 12a9 9 0 1 0 3-6.7" />
                    <path d="M3 4v5h5" />
                  </svg>
                  Reset
                </button>
              )}
            </div>

            {filtered.length === 0 ? (
              <div className="strategies-empty">
                <span className="strategies-empty-glyph" aria-hidden>
                  <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
                    <circle cx="11" cy="11" r="7" />
                    <path d="M16.4 16.4L21 21" />
                  </svg>
                </span>
                <h3 className="strategies-empty-title">No strategies match those filters.</h3>
                <p className="strategies-empty-sub">
                  Try a broader category, a different search term, or reset to see the full library.
                </p>
                <button type="button" className="strategies-empty-cta" onClick={resetFilters}>
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
                    <path d="M3 12a9 9 0 1 0 3-6.7" />
                    <path d="M3 4v5h5" />
                  </svg>
                  Reset filters
                </button>
              </div>
            ) : (
              <div className="strategy-cards">
                {filtered.map((entry) => {
                  const legs = LEG_COUNT[entry.strategyId];
                  return (
                    <Link
                      key={entry.id}
                      href={`/strategies/${entry.id}`}
                      className={`strategy-card accent-${entry.accent}${entry.status === "recommended" ? " is-featured" : ""}`}
                    >
                      <span className="strategy-card-rule" aria-hidden />

                      <div className="strategy-card-head">
                        <span className="strategy-card-glyph" aria-hidden>
                          <StrategyGlyph id={entry.strategyId} />
                        </span>
                        {entry.status === "recommended" ? (
                          <span className="strategy-card-pill is-featured">
                            <svg width="11" height="11" viewBox="0 0 24 24" fill="currentColor" aria-hidden>
                              <path d="M12 2l3 6.5 7 .8-5.2 4.8 1.5 7-6.3-3.6L5.7 21l1.5-7L2 9.3l7-.8L12 2z" />
                            </svg>
                            Featured
                          </span>
                        ) : null}
                      </div>

                      <div className="strategy-card-body">
                        <h3 className="strategy-card-title">{entry.displayName}</h3>
                        <p className="strategy-card-desc">{entry.oneLiner}</p>
                      </div>

                      <div className="strategy-card-thumb" aria-hidden>
                        <PayoffThumb id={entry.strategyId} />
                        <span className="strategy-card-thumb-axis">
                          <span>BTC at expiry</span>
                          <span className="strategy-card-thumb-arrow">→</span>
                        </span>
                      </div>

                      <div className="strategy-card-tags">
                        {entry.categories.map((c) => (
                          <span key={c} className="strategy-card-tag">
                            {c}
                          </span>
                        ))}
                      </div>

                      <div className="strategy-card-meta">
                        <div className="strategy-card-meta-cell">
                          <span>Risk</span>
                          <RiskMeter tier={RISK_TIER[entry.strategyId]} />
                        </div>
                        <div className="strategy-card-meta-cell">
                          <span>Min size</span>
                          <strong className="mono">{strippedBudget(entry)}</strong>
                        </div>
                        <div className="strategy-card-meta-cell">
                          <span>Legs</span>
                          <strong className="mono">{legs ?? "Auto"}</strong>
                        </div>
                      </div>
                    </Link>
                  );
                })}
              </div>
            )}
          </section>
        )}
      </div>
    </>
  );
}
