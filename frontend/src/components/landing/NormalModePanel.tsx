// Normal Mode panel for the new frontend.
//
// Flow:
//   1. User types a goal in plain English + budget + risk + time
//   2. POST /api/intent/parse  → ParsedIntent (or clarifying question)
//   3. POST /api/strategies/compile-from-intent → recommended strategy + payoff preview
//   4. User clicks "Open this strategy" → navigate to
//        /strategies/<catalog-id>?budget=<value>
//      The existing StrategyWorkbench on the detail page does compile +
//      wallet signature + audit unchanged.
//
// This is *not* an auto-trader. The AI never builds Move calls. Final
// payoff and the signed transaction are produced by the deterministic
// compiler + the user's wallet.

"use client";

import Link from "next/link";
import { useEffect, useState } from "react";

import { ApiError, compileFromIntent, parseIntent } from "@/lib/api";
import {
  bigIntSafe,
  formatDusdcDisplayString,
  formatPriceCompact,
} from "@/lib/format";
import { findCatalogEntryByStrategyId } from "@/lib/strategyCatalog";
import type {
  GuidedCompileResponse,
  ParsedIntentResponse,
  ParsedIntentSuccess,
} from "@/types/structx";

type RiskPreference = "conservative" | "balanced" | "aggressive";
type TimePreference = "nearest_active" | "today" | "this_week";

type Chip = {
  id: string;
  label: string;
  prompt: string;
  icon: React.ReactNode;
};

const ICON_PROPS = {
  width: 14,
  height: 14,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.8,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  "aria-hidden": true,
};

const QUICK_CHIPS: ReadonlyArray<Chip> = [
  {
    id: "protect",
    label: "Protect against a dump",
    prompt: "I want to protect against a BTC dump with 50 dUSDC.",
    icon: (
      <svg {...ICON_PROPS}>
        <path d="M12 3l8 3v6c0 5-3.4 8.3-8 9-4.6-.7-8-4-8-9V6l8-3z" />
        <path d="M9 12l2 2 4-4" />
      </svg>
    ),
  },
  {
    id: "upside",
    label: "Catch a breakout",
    prompt: "I want upside if BTC breaks out with 50 dUSDC.",
    icon: (
      <svg {...ICON_PROPS}>
        <path d="M5 19l14-14" />
        <path d="M9 5h10v10" />
      </svg>
    ),
  },
  {
    id: "either",
    label: "Big move, either way",
    prompt:
      "I expect a big BTC move in either direction. Use 50 dUSDC.",
    icon: (
      <svg {...ICON_PROPS}>
        <path d="M3 12h18" />
        <path d="M7 8l-4 4 4 4" />
        <path d="M17 8l4 4-4 4" />
      </svg>
    ),
  },
  {
    id: "best",
    label: "Pick the best for me",
    prompt: "Pick the best strategy for my 50 dUSDC budget.",
    icon: (
      <svg {...ICON_PROPS}>
        <path d="M12 3l1.6 4.4L18 9l-4.4 1.6L12 15l-1.6-4.4L6 9l4.4-1.6L12 3z" />
      </svg>
    ),
  },
];

function isParsedSuccess(
  parsed: ParsedIntentResponse | null,
): parsed is ParsedIntentSuccess {
  return Boolean(parsed && parsed.ok);
}

export function NormalModePanel() {
  const [message, setMessage] = useState(QUICK_CHIPS[0].prompt);
  const [budgetDUSDC, setBudgetDUSDC] = useState("50");
  const [riskPreference, setRiskPreference] =
    useState<RiskPreference>("balanced");
  const [timePreference, setTimePreference] =
    useState<TimePreference>("nearest_active");
  const [activeChipId, setActiveChipId] = useState<string | null>(
    QUICK_CHIPS[0].id,
  );

  const [loading, setLoading] = useState(false);
  const [parsed, setParsed] = useState<ParsedIntentResponse | null>(null);
  const [compiled, setCompiled] = useState<GuidedCompileResponse | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function generate() {
    setLoading(true);
    setError(null);
    setParsed(null);
    setCompiled(null);
    try {
      const parsedRes = await parseIntent({
        owner: "0x0",
        message,
        budgetDUSDC,
        riskPreference,
        timePreference,
      });
      setParsed(parsedRes);
      if (!parsedRes.ok) {
        setError(parsedRes.clarifyingQuestion);
        return;
      }
      const compiledRes = await compileFromIntent({
        owner: "0x0",
        intent: parsedRes,
      });
      setCompiled(compiledRes);
    } catch (err) {
      if (err instanceof ApiError) {
        setError(
          err.body.clarifyingQuestion ??
            err.body.message ??
            err.body.error ??
            err.message,
        );
      } else if (err instanceof Error) {
        setError(err.message);
      } else {
        setError(String(err));
      }
    } finally {
      setLoading(false);
    }
  }

  const recommendedHref = (() => {
    if (!compiled) return null;
    const entry = findCatalogEntryByStrategyId(compiled.strategy);
    if (!entry) return null;
    return {
      href: `/strategies/${entry.id}?budget=${encodeURIComponent(budgetDUSDC)}`,
      displayName: entry.displayName,
    };
  })();

  return (
    <section className="normal-panel">
      <div className="normal-panel-grid">
        <div className="normal-panel-form">
          <div className="normal-step">
            <span className="normal-step-num">1</span>
            <div>
              <h2>Describe your market goal</h2>
              <p className="normal-panel-sub">
                Start with one of these ideas or write your own. StructX will
                match it to a strategy with a clear maximum loss.
              </p>
            </div>
          </div>

          <div className="normal-chips">
            {QUICK_CHIPS.map((chip) => (
              <button
                key={chip.id}
                type="button"
                className={`normal-chip ${activeChipId === chip.id ? "active" : ""}`}
                onClick={() => {
                  setMessage(chip.prompt);
                  setActiveChipId(chip.id);
                }}
              >
                <span className="normal-chip-icon">{chip.icon}</span>
                {chip.label}
              </button>
            ))}
          </div>

          <label className="normal-field">
            <span className="normal-field-label">Your goal</span>
            <textarea
              value={message}
              onChange={(e) => {
                setMessage(e.target.value);
                setActiveChipId(null);
              }}
              rows={3}
              placeholder="Example: I want downside protection on BTC for the next expiry with 50 dUSDC."
            />
          </label>

          <div className="normal-step normal-step-secondary">
            <span className="normal-step-num">2</span>
            <div>
              <h3>Set your size and risk</h3>
            </div>
          </div>

          <div className="normal-row">
            <label className="normal-field">
              <span className="normal-field-label">Budget</span>
              <div className="normal-suffix">
                <input
                  value={budgetDUSDC}
                  onChange={(e) => setBudgetDUSDC(e.target.value)}
                  inputMode="decimal"
                />
                <span>dUSDC</span>
              </div>
            </label>

            <label className="normal-field">
              <span className="normal-field-label">Risk</span>
              <select
                value={riskPreference}
                onChange={(e) =>
                  setRiskPreference(e.target.value as RiskPreference)
                }
              >
                <option value="conservative">Conservative</option>
                <option value="balanced">Balanced</option>
                <option value="aggressive">Aggressive</option>
              </select>
            </label>

            <label className="normal-field">
              <span className="normal-field-label">Expiry</span>
              <select
                value={timePreference}
                onChange={(e) =>
                  setTimePreference(e.target.value as TimePreference)
                }
              >
                <option value="nearest_active">Nearest active</option>
                <option value="today">Today</option>
                <option value="this_week">This week</option>
              </select>
            </label>
          </div>

          <button
            type="button"
            className="normal-generate"
            disabled={loading || !message.trim()}
            onClick={() => void generate()}
          >
            {loading ? (
              <>
                <span className="normal-generate-spinner" aria-hidden />
                Finding a strategy…
              </>
            ) : (
              <>
                Find my strategy
                <svg
                  width="14"
                  height="14"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2.2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  aria-hidden
                >
                  <path d="M5 12h14M13 6l6 6-6 6" />
                </svg>
              </>
            )}
          </button>

          <p className="normal-disclaimer">
            StructX uses your message to choose a strategy, then calculates
            the payoff and premium from live market data. Your wallet shows
            the full transaction for approval when you decide to open it.
          </p>
        </div>

        <div className="normal-panel-output">
          {!parsed && !compiled && !loading && !error && (
            <EmptyOutput />
          )}

          {loading && <LoadingOutput />}

          {error && (
            <div className="normal-error">
              <strong>Need a bit more info</strong>
              <p>{error}</p>
            </div>
          )}

          {isParsedSuccess(parsed) && (
            <div className="normal-card">
              <div className="normal-card-eyebrow">
                <span className="normal-step-num small">3</span>
                What we understood
              </div>
              <h3>{goalLabel(parsed.goal)}</h3>
              <dl className="normal-meta">
                <dt>Asset</dt>
                <dd>{parsed.asset}</dd>
                <dt>Budget</dt>
                <dd className="mono">{parsed.budgetDUSDC} dUSDC</dd>
                <dt>Risk</dt>
                <dd className="capitalize">{parsed.riskPreference}</dd>
                <dt>Expiry</dt>
                <dd className="capitalize">
                  {parsed.timePreference.replace(/_/g, " ")}
                </dd>
              </dl>
              {parsed.reasoningSummary && (
                <p className="normal-reason">{parsed.reasoningSummary}</p>
              )}
            </div>
          )}

          {compiled && (
            <div className="normal-card normal-card-recommend">
              <div className="normal-card-eyebrow">
                <span className="normal-step-num small">4</span>
                Recommended strategy
              </div>
              <h3>{prettyStrategyName(compiled.strategy)}</h3>

              <PayoffCurve compiled={compiled} />

              <div className="normal-stats">
                <Stat
                  label="Premium"
                  value={formatDusdcDisplayString(
                    compiled.premiumRequiredDisplay,
                  )}
                />
                <Stat
                  label="Max loss"
                  value={formatDusdcDisplayString(compiled.maxLossDisplay)}
                  tone="neg"
                />
                <Stat
                  label="Max payout"
                  value={formatDusdcDisplayString(
                    compiled.maxGrossPayoutDisplay,
                  )}
                  tone="pos"
                />
                <Stat
                  label="Legs"
                  value={String(compiled.legs.length)}
                />
              </div>
              {compiled.recommendation?.reasoningSummary && (
                <p className="normal-reason">
                  {compiled.recommendation.reasoningSummary}
                </p>
              )}
              {recommendedHref && (
                <Link href={recommendedHref.href} className="normal-cta">
                  Open {recommendedHref.displayName}
                  <svg
                    width="14"
                    height="14"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="2.2"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    aria-hidden
                  >
                    <path d="M5 12h14M13 6l6 6-6 6" />
                  </svg>
                </Link>
              )}
            </div>
          )}
        </div>
      </div>
    </section>
  );
}

function Stat({
  label,
  value,
  tone,
}: {
  label: string;
  value: string;
  tone?: "pos" | "neg";
}) {
  return (
    <div className={`normal-stat ${tone ?? ""}`}>
      <span>{label}</span>
      <strong className="mono">{value}</strong>
    </div>
  );
}

/**
 * Smoothed payoff curve. Treats each of the 5 scenario buckets as a sample
 * point on the net-PnL function, then renders a Catmull-Rom-ish smooth path
 * via cubic Beziers. Adds a zero baseline, a soft gradient fill, and small
 * dot markers so the user can see the shape and the breakeven crossover
 * clearly. Much more credible than five bars.
 */
function PayoffCurve({ compiled }: { compiled: GuidedCompileResponse }) {
  const W = 320; // intrinsic viewBox width — scales via CSS
  const H = 130;
  const PAD_X = 16;
  const PAD_Y_TOP = 14;
  const PAD_Y_BOT = 28; // leave room for axis labels

  const pnls = compiled.payoffTable.map((row) => {
    const v = bigIntSafe(row.netPnlRaw);
    return v === null ? 0 : Number(v);
  });
  // Symmetric y-range around zero so positive and negative magnitudes are
  // visually comparable.
  const absMax = Math.max(1, ...pnls.map((v) => Math.abs(v)));
  const yScale = (v: number) => {
    const norm = v / absMax; // [-1, 1]
    const top = PAD_Y_TOP;
    const bot = H - PAD_Y_BOT;
    const mid = (top + bot) / 2;
    return mid - norm * (mid - top);
  };
  const xs = pnls.map((_, i) => {
    const usable = W - PAD_X * 2;
    return PAD_X + (usable * i) / (pnls.length - 1);
  });
  const ys = pnls.map((v) => yScale(v));

  // Build a smooth path using a simple Bezier smoothing between samples.
  const linePath = (() => {
    if (xs.length < 2) return "";
    const parts: string[] = [`M ${xs[0]} ${ys[0]}`];
    for (let i = 0; i < xs.length - 1; i += 1) {
      const x0 = xs[i];
      const y0 = ys[i];
      const x1 = xs[i + 1];
      const y1 = ys[i + 1];
      const cp1x = x0 + (x1 - x0) / 2;
      const cp1y = y0;
      const cp2x = x0 + (x1 - x0) / 2;
      const cp2y = y1;
      parts.push(`C ${cp1x} ${cp1y}, ${cp2x} ${cp2y}, ${x1} ${y1}`);
    }
    return parts.join(" ");
  })();
  const fillPath = (() => {
    if (!linePath) return "";
    const baseline = yScale(0);
    return `${linePath} L ${xs[xs.length - 1]} ${baseline} L ${xs[0]} ${baseline} Z`;
  })();

  const zeroY = yScale(0);
  const labels = [
    `< ${formatPriceCompact(compiled.strikes.k1)}`,
    formatPriceCompact(compiled.strikes.k1),
    formatPriceCompact(compiled.strikes.k2),
    formatPriceCompact(compiled.strikes.k3),
    `> ${formatPriceCompact(compiled.strikes.k4)}`,
  ];

  return (
    <div className="normal-curve">
      <svg
        viewBox={`0 0 ${W} ${H}`}
        preserveAspectRatio="none"
        className="normal-curve-svg"
        role="img"
        aria-label="Payoff curve"
      >
        <defs>
          <linearGradient id="normal-curve-fill" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="var(--sx-teal-dark)" stopOpacity="0.28" />
            <stop offset="100%" stopColor="var(--sx-teal-dark)" stopOpacity="0" />
          </linearGradient>
        </defs>

        {/* zero baseline */}
        <line
          x1={PAD_X}
          x2={W - PAD_X}
          y1={zeroY}
          y2={zeroY}
          stroke="var(--sx-border-strong)"
          strokeDasharray="2 4"
          strokeWidth="1"
        />

        {/* shaded area under the curve down to baseline */}
        <path d={fillPath} fill="url(#normal-curve-fill)" />

        {/* curve itself */}
        <path
          d={linePath}
          fill="none"
          stroke="var(--sx-teal-dark)"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        />

        {/* sample dots, tinted by sign */}
        {xs.map((x, i) => {
          const v = pnls[i];
          const color =
            v > 0
              ? "var(--sx-teal-dark)"
              : v < 0
                ? "var(--sx-danger)"
                : "var(--sx-navy-muted)";
          return (
            <g key={i}>
              <circle
                cx={x}
                cy={ys[i]}
                r={3.5}
                fill="var(--sx-surface)"
                stroke={color}
                strokeWidth="2"
              />
            </g>
          );
        })}
      </svg>

      {/* X-axis labels — 5 strikes, mono so the alignment looks intentional */}
      <div className="normal-curve-axis">
        {labels.map((l, i) => (
          <span key={i} className="mono">
            {l}
          </span>
        ))}
      </div>
      <div className="normal-curve-legend">
        <span>
          <span className="dot teal" aria-hidden /> Profit
        </span>
        <span>
          <span className="dot red" aria-hidden /> Loss
        </span>
        <span>
          <span className="dot dash" aria-hidden /> Breakeven
        </span>
      </div>
    </div>
  );
}

function EmptyOutput() {
  return (
    <div className="normal-empty">
      <div className="normal-empty-illustration" aria-hidden>
        {[78, 44, 18, 44, 78].map((h, i) => (
          <span
            key={i}
            className="normal-empty-bar"
            style={{ height: `${h}%` }}
          />
        ))}
      </div>
      <h3>Your payoff preview shows up here</h3>
      <p>
        Hit <strong>Generate strategy</strong>. You will see the parsed
        intent, the recommended payoff shape, premium, and max payout.
        Nothing is signed until you say so.
      </p>
    </div>
  );
}

const LOADING_STEPS = [
  "Parsing your goal",
  "Picking a strategy",
  "Pricing legs against live DeepBook markets",
] as const;

function LoadingOutput() {
  // Advance through the narration on a timer so the user gets a sense of
  // progress even before the network round-trip finishes. We never reach the
  // last step until the parent flips `loading` to false, so the final step
  // stays in "active" until then.
  const [stepIndex, setStepIndex] = useState(0);
  useEffect(() => {
    if (stepIndex >= LOADING_STEPS.length - 1) return;
    const t = window.setTimeout(
      () => setStepIndex((i) => Math.min(i + 1, LOADING_STEPS.length - 1)),
      900,
    );
    return () => window.clearTimeout(t);
  }, [stepIndex]);

  return (
    <div className="normal-loading">
      <div className="normal-loading-illustration" aria-hidden>
        {[78, 44, 18, 44, 78].map((h, i) => (
          <span
            key={i}
            className="normal-empty-bar"
            style={{ height: `${h}%`, animationDelay: `${i * 0.1}s` }}
          />
        ))}
      </div>
      <ul className="normal-loading-steps">
        {LOADING_STEPS.map((label, i) => {
          const state =
            i < stepIndex ? "done" : i === stepIndex ? "active" : "pending";
          return (
            <li key={label} className={`normal-loading-step state-${state}`}>
              <span className="normal-loading-dot" aria-hidden>
                {state === "done" ? (
                  <svg
                    viewBox="0 0 24 24"
                    width="11"
                    height="11"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="3"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                  >
                    <path d="M5 12l5 5L20 6" />
                  </svg>
                ) : (
                  <span className="normal-loading-bullet" />
                )}
              </span>
              {label}
            </li>
          );
        })}
      </ul>
    </div>
  );
}

function goalLabel(goal: string): string {
  switch (goal) {
    case "downside_protection":
      return "Downside protection";
    case "upside_speculation":
      return "Upside speculation";
    case "two_sided_breakout":
      return "Two-sided breakout";
    case "range_income":
      return "Range income";
    case "barrier_proxy":
      return "Near-barrier proxy";
    default:
      return "Custom goal";
  }
}

function prettyStrategyName(id: string): string {
  const entry = findCatalogEntryByStrategyId(id as never);
  if (entry) return entry.displayName;
  return id
    .toLowerCase()
    .split("_")
    .map((w) => (w ? w[0].toUpperCase() + w.slice(1) : ""))
    .join(" ");
}
