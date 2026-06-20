"use client";

import { useMemo, useState } from "react";

import { EmptyState } from "@/components/common/EmptyState";
import { LegsTable } from "@/components/preview/LegsTable";
import { PayoffTable } from "@/components/preview/PayoffTable";
import { PayoffVisualization } from "@/components/preview/PayoffVisualization";
import { PreviewSummary } from "@/components/preview/PreviewSummary";
import { SmartSelectorPanel } from "@/components/preview/SmartSelectorPanel";
import { WarningsPanel } from "@/components/WarningsPanel";
import { ApiError, compileFromIntent, parseIntent } from "@/lib/api";
import { formatDusdcDisplayString } from "@/lib/format";
import { strategyDisplayName } from "@/lib/strategyCatalog";
import type {
  CompileResponse,
  GuidedCompileResponse,
  ParsedIntentResponse,
  ParsedIntentSuccess,
  StrategyStyle,
} from "@/types/structx";

type Props = {
  owner: string;
  connectedAddress: string | null;
  managerId: string;
  managerBalance: string | null;
  managerBalanceLoading: boolean;
  managerDiscovering: boolean;
  managerNotice: string | null;
  managerNoticeTone: "info" | "error" | null;
  creatingManager: boolean;
  onRefreshBalance: () => void;
  onCreateManager: () => void;
  onUseRecommendation: (
    compiled: GuidedCompileResponse,
    parsed: ParsedIntentSuccess,
    action: "open" | "customize",
  ) => void;
  onCopied: (label: string) => void;
};

type RiskPreference = "conservative" | "balanced" | "aggressive";
type TimePreference = "nearest_active" | "today" | "this_week";

const QUICK_INTENTS = [
  "Protect me if BTC dumps today with 5 dUSDC.",
  "I want upside if BTC breaks out with 5 dUSDC.",
  "I expect a big BTC move but do not know direction. Use 5 dUSDC.",
  "I want defined risk with a small 2 dUSDC budget.",
];

export function NormalModeView({
  owner,
  connectedAddress,
  managerId,
  managerBalance,
  managerBalanceLoading,
  managerDiscovering,
  managerNotice,
  managerNoticeTone,
  creatingManager,
  onRefreshBalance,
  onCreateManager,
  onUseRecommendation,
  onCopied,
}: Props) {
  const [message, setMessage] = useState(
    "I expect a big BTC move today but I do not know direction. Use 5 dUSDC.",
  );
  const [budgetDUSDC, setBudgetDUSDC] = useState("5");
  const [riskPreference, setRiskPreference] =
    useState<RiskPreference>("balanced");
  const [timePreference, setTimePreference] =
    useState<TimePreference>("nearest_active");

  const [loading, setLoading] = useState(false);
  const [parsedIntent, setParsedIntent] = useState<ParsedIntentResponse | null>(
    null,
  );
  const [compiled, setCompiled] = useState<GuidedCompileResponse | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function generateStrategy() {
    setLoading(true);
    setParsedIntent(null);
    setCompiled(null);
    setError(null);

    try {
      const parsed = await parseIntent({
        owner: connectedAddress ?? owner,
        message,
        budgetDUSDC,
        riskPreference,
        timePreference,
      });
      setParsedIntent(parsed);

      if (!parsed.ok) {
        setError(parsed.clarifyingQuestion);
        return;
      }

      const compiledJson = await compileFromIntent({
        owner: connectedAddress ?? owner,
        intent: parsed,
      });

      setCompiled(compiledJson);
      setParsedIntent(parsed);
    } catch (err) {
      if (err instanceof ApiError) {
        const fallbackIntent = err.body.fallbackIntent as
          | ParsedIntentSuccess
          | undefined;
        if (fallbackIntent) {
          setParsedIntent({ ok: false, fallbackIntent, missingFields: err.body.missingFields ?? [], clarifyingQuestion: err.body.clarifyingQuestion ?? err.message });
        }
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

  const guidedWarnings = useMemo(() => {
    const warnings: string[] = [];
    if (parsedIntent?.ok) {
      warnings.push(...parsedIntent.warnings);
    }
    if (compiled?.recommendation?.reasoningSummary) {
      warnings.unshift(compiled.recommendation.reasoningSummary);
    }
    if (compiled) {
      warnings.push(...compiled.warnings);
    }
    return warnings;
  }, [compiled, parsedIntent]);

  return (
    <section className="normal-layout">
      <section className="panel guided-panel">
        <div className="panel-header">
          <p className="eyebrow">Normal Mode</p>
          <h2>Describe your market goal</h2>
          <p className="muted">
            StructX converts plain-English goals into transparent DeepBook
            Predict payoff candidates. You review outcomes, then sign from your
            wallet.
          </p>
        </div>

        <div className="balance-pill">
          <div>
            <span>Selected manager balance</span>
            <strong>
              {managerDiscovering
                ? "Finding manager…"
                : managerBalanceLoading
                ? "Loading…"
                : managerBalance ?? "Unavailable"}
            </strong>
          </div>
          <div className="balance-actions">
            {!managerId && (
              <button
                type="button"
                className="mini-button"
                onClick={onCreateManager}
                disabled={creatingManager || managerDiscovering}
              >
                {creatingManager ? "Creating…" : "Create manager"}
              </button>
            )}
            <button
              type="button"
              className="mini-button"
              onClick={onRefreshBalance}
              disabled={managerBalanceLoading || managerDiscovering || !managerId}
            >
              {managerBalanceLoading ? "Refreshing…" : "Refresh"}
            </button>
          </div>
        </div>
        {managerNotice && (
          <p className={`muted ${managerNoticeTone === "error" ? "danger" : ""}`}>
            {managerNotice}
          </p>
        )}
        <p className="muted">
          Guided Mode still opens through the selected PredictManager in
          Advanced Mode. This is not your raw wallet cash balance.
        </p>

        <div className="intent-chips">
          {QUICK_INTENTS.map((intent) => (
            <button
              key={intent}
              type="button"
              onClick={() => setMessage(intent)}
            >
              {intent}
            </button>
          ))}
        </div>

        <label className="field">
          <span className="field-label">Intent</span>
          <textarea
            value={message}
            onChange={(event) => setMessage(event.target.value)}
            placeholder="Example: I want downside protection on BTC for the next expiry with about 20 dUSDC."
            rows={5}
          />
        </label>

        <div className="ticket-row">
          <label className="field">
            <span className="field-label">Budget</span>
            <div className="input-suffix">
              <input
                value={budgetDUSDC}
                onChange={(event) => setBudgetDUSDC(event.target.value)}
                inputMode="decimal"
              />
              <span>dUSDC</span>
            </div>
          </label>

          <label className="field">
            <span className="field-label">Risk</span>
            <select
              value={riskPreference}
              onChange={(event) =>
                setRiskPreference(event.target.value as RiskPreference)
              }
            >
              <option value="conservative">Conservative</option>
              <option value="balanced">Balanced</option>
              <option value="aggressive">Aggressive</option>
            </select>
          </label>
        </div>

        <label className="field">
          <span className="field-label">Time preference</span>
          <select
            value={timePreference}
            onChange={(event) =>
              setTimePreference(event.target.value as TimePreference)
            }
          >
            <option value="nearest_active">Nearest active</option>
            <option value="today">Today</option>
            <option value="this_week">This week</option>
          </select>
        </label>

        <button
          className="primary-button"
          type="button"
          disabled={loading || !message.trim()}
          onClick={() => void generateStrategy()}
        >
          {loading ? "Generating…" : "Generate strategy"}
        </button>

        <div className="guided-disclaimer">
          AI helps interpret intent. The final payoff, premium, and transaction
          are produced by deterministic StructX compiler logic. You must approve
          every transaction in your wallet.
        </div>

        {error && (
          <div className="error-box">
            <strong>Guided mode needs input</strong>
            <pre>{error}</pre>
          </div>
        )}
      </section>

      <section className="preview-column">
        {!compiled && !parsedIntent && (
          <EmptyState
            title="No recommendation yet"
            body="Enter your goal and budget. StructX will parse the intent, recommend a strategy, and compile the payoff preview."
          />
        )}

        {parsedIntent?.ok && <ParsedIntentCard parsed={parsedIntent} />}

        {compiled && (
          <>
            <RecommendationCard compiled={compiled} />
            <PreviewSummary
              compiled={compiled}
              displayName="Recommended strategy"
              onCopied={onCopied}
            />
            {compiled.smartSelector && (
              <SmartSelectorPanel info={compiled.smartSelector} />
            )}
            <PayoffVisualization compiled={compiled} />
            <LegsTable legs={compiled.legs} />
            <PayoffTable rows={compiled.payoffTable} strikes={compiled.strikes} />
            {guidedWarnings.length > 0 && (
              <WarningsPanel warnings={guidedWarnings} />
            )}

            <section className="panel">
              <div className="panel-header">
                <p className="eyebrow">Next step</p>
                <h2>Open or customize</h2>
                <p className="muted">
                  Move into Advanced Mode to use the existing dry-run, wallet
                  signature, and audit flow with this recommendation.
                </p>
              </div>

              <button
                className="primary-button"
                type="button"
                onClick={() => {
                  if (parsedIntent?.ok) {
                    onUseRecommendation(compiled, parsedIntent, "open");
                  }
                }}
              >
                Open recommended strategy
              </button>

              <button
                className="secondary-button full-width top-space"
                type="button"
                onClick={() => {
                  if (parsedIntent?.ok) {
                    onUseRecommendation(compiled, parsedIntent, "customize");
                  }
                }}
              >
                Customize in Advanced Mode
              </button>
            </section>
          </>
        )}
      </section>
    </section>
  );
}

function ParsedIntentCard({ parsed }: { parsed: ParsedIntentSuccess }) {
  return (
    <section className="panel">
      <div className="panel-header">
        <p className="eyebrow">Parsed intent</p>
        <h2>{intentGoalLabel(parsed.goal)}</h2>
      </div>

      <div className="meta-grid">
        <Meta label="Asset" value={parsed.asset} />
        <Meta label="Budget" value={`${parsed.budgetDUSDC} dUSDC`} />
        <Meta label="Risk" value={parsed.riskPreference} />
        <Meta label="Time" value={parsed.timePreference} />
        <Meta
          label="Strategy"
          value={strategyDisplayName(parsed.recommendedStrategy)}
        />
        <Meta label="Style" value={parsed.recommendedStyle} />
      </div>

      <p className="muted normal-reason">{parsed.reasoningSummary}</p>
    </section>
  );
}

function RecommendationCard({ compiled }: { compiled: GuidedCompileResponse }) {
  return (
    <section className="panel recommendation-card">
      <div className="panel-header">
        <p className="eyebrow">Recommended</p>
        <h2>{strategyDisplayName(compiled.strategy)}</h2>
      </div>

      <p className="muted">
        Suggested from your intent, then priced by the deterministic StructX
        compiler.
      </p>

      <div className="stats-grid">
        <Stat label="Premium" value={compiled.premiumRequiredDisplay} />
        <Stat label="Max loss" value={compiled.maxLossDisplay} />
        <Stat label="Max gross payout" value={compiled.maxGrossPayoutDisplay} />
        <Stat label="Style" value={compiled.style} />
      </div>
    </section>
  );
}

function Meta({ label, value }: { label: string; value: string }) {
  return (
    <div className="meta-item">
      <label>{label}</label>
      <span>{value}</span>
    </div>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="stat">
      <label>{label}</label>
      <strong>{formatDusdcMaybe(value)}</strong>
    </div>
  );
}

function formatDusdcMaybe(value: string) {
  return value.includes("dUSDC") ? formatDusdcDisplayString(value) : value;
}

function intentGoalLabel(goal: string) {
  switch (goal) {
    case "downside_protection":
      return "Downside protection";
    case "upside_speculation":
      return "Upside speculation";
    case "two_sided_breakout":
      return "Two-sided breakout";
    case "range_income":
      return "Range payoff";
    default:
      return "Structured payoff";
  }
}
