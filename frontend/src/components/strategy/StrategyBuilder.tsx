"use client";

import { useState } from "react";

import { CopyButton } from "@/components/common/CopyButton";
import { StyleSegmentedControl } from "@/components/strategy/StyleSegmentedControl";
import { shortAddress } from "@/lib/format";
import type { StrategyId, StrategyStyle } from "@/types/structx";

type Props = {
  strategyId: StrategyId;
  displayName: string;
  legHint: string;
  status: "recommended" | "beta";
  owner: string;
  onOwnerChange: (value: string) => void;
  walletAddress: string | null;
  managerId: string;
  onManagerIdChange: (value: string) => void;
  budgetDUSDC: string;
  onBudgetChange: (value: string) => void;
  slippageBps: string;
  onSlippageChange: (value: string) => void;
  style: StrategyStyle;
  onStyleChange: (value: StrategyStyle) => void;
  portfolioExposureDUSDC: string;
  onPortfolioExposureChange: (value: string) => void;
  overHedgeCapBps: string;
  onOverHedgeCapBpsChange: (value: string) => void;
  deadZoneBps: string;
  onDeadZoneBpsChange: (value: string) => void;
  convexGammaBps: string;
  onConvexGammaBpsChange: (value: string) => void;
  moonshotRangeWeightBps: string;
  onMoonshotRangeWeightBpsChange: (value: string) => void;
  moonshotTailGammaBps: string;
  onMoonshotTailGammaBpsChange: (value: string) => void;
  upsideNearRangeWeightBps: string;
  onUpsideNearRangeWeightBpsChange: (value: string) => void;
  upsideUpperRangeWeightBps: string;
  onUpsideUpperRangeWeightBpsChange: (value: string) => void;
  upsideTailGammaBps: string;
  onUpsideTailGammaBpsChange: (value: string) => void;
  downsideNearRangeWeightBps: string;
  onDownsideNearRangeWeightBpsChange: (value: string) => void;
  downsideLowerRangeWeightBps: string;
  onDownsideLowerRangeWeightBpsChange: (value: string) => void;
  downsideStepTailGammaBps: string;
  onDownsideStepTailGammaBpsChange: (value: string) => void;
  condorCenterWeightBps: string;
  onCondorCenterWeightBpsChange: (value: string) => void;
  barrierSide: "up" | "down";
  onBarrierSideChange: (value: "up" | "down") => void;
  barrierNearRangeWeightBps: string;
  onBarrierNearRangeWeightBpsChange: (value: string) => void;
  barrierTailGammaBps: string;
  onBarrierTailGammaBpsChange: (value: string) => void;
  managerBalance: string | null;
  managerBalanceLoading: boolean;
  managerDiscovering: boolean;
  managerNotice: string | null;
  managerNoticeTone: "info" | "error" | null;
  creatingManager: boolean;
  onRefreshBalance: () => void;
  onCreateManager: () => void;
  onCompile: () => void;
  compileDisabled: boolean;
  compiling: boolean;
  disabledReason: string | null;
  onCopied: (label: string) => void;
};

export function StrategyBuilder({
  strategyId,
  displayName,
  legHint,
  status,
  owner,
  onOwnerChange,
  walletAddress,
  managerId,
  onManagerIdChange,
  budgetDUSDC,
  onBudgetChange,
  slippageBps,
  onSlippageChange,
  style,
  onStyleChange,
  portfolioExposureDUSDC,
  onPortfolioExposureChange,
  overHedgeCapBps,
  onOverHedgeCapBpsChange,
  deadZoneBps,
  onDeadZoneBpsChange,
  convexGammaBps,
  onConvexGammaBpsChange,
  moonshotRangeWeightBps,
  onMoonshotRangeWeightBpsChange,
  moonshotTailGammaBps,
  onMoonshotTailGammaBpsChange,
  upsideNearRangeWeightBps,
  onUpsideNearRangeWeightBpsChange,
  upsideUpperRangeWeightBps,
  onUpsideUpperRangeWeightBpsChange,
  upsideTailGammaBps,
  onUpsideTailGammaBpsChange,
  downsideNearRangeWeightBps,
  onDownsideNearRangeWeightBpsChange,
  downsideLowerRangeWeightBps,
  onDownsideLowerRangeWeightBpsChange,
  downsideStepTailGammaBps,
  onDownsideStepTailGammaBpsChange,
  condorCenterWeightBps,
  onCondorCenterWeightBpsChange,
  barrierSide,
  onBarrierSideChange,
  barrierNearRangeWeightBps,
  onBarrierNearRangeWeightBpsChange,
  barrierTailGammaBps,
  onBarrierTailGammaBpsChange,
  managerBalance,
  managerBalanceLoading,
  managerDiscovering,
  managerNotice,
  managerNoticeTone,
  creatingManager,
  onRefreshBalance,
  onCreateManager,
  onCompile,
  compileDisabled,
  compiling,
  disabledReason,
  onCopied,
}: Props) {
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const ownerLocked = Boolean(walletAddress);

  return (
    <section className="ticket panel">
      <div className="ticket-head">
        <div>
          <p className="eyebrow">
            Build{status === "beta" ? " · Beta" : " · Recommended"}
          </p>
          <h2>{displayName}</h2>
          <p className="muted">
            Legs: <span className="mono">{legHint}</span>
          </p>
        </div>
      </div>

      <div className="ticket-row">
        <label className="field">
          <span className="field-label">Wallet owner</span>
          {ownerLocked ? (
            <div className="readonly-row">
              <code title={owner}>{shortAddress(owner)}</code>
              <CopyButton
                value={owner}
                label=""
                onCopied={() => onCopied("owner address")}
                className="tight"
              />
            </div>
          ) : (
            <input
              value={owner}
              onChange={(event) => onOwnerChange(event.target.value)}
              placeholder="0x..."
            />
          )}
          <span className="field-help">
            {ownerLocked
              ? "Locked to the connected wallet."
              : "Connect a wallet to lock this to your address."}
          </span>
        </label>
      </div>

      <div className="balance-pill">
        <div>
          <span>Funding manager balance</span>
          <strong>
            {managerDiscovering
              ? "Finding manager…"
              : managerBalanceLoading
              ? "Loading…"
              : (managerBalance ?? "Unavailable")}
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
        <p className={`field-help ${managerNoticeTone === "error" ? "danger" : ""}`}>
          {managerNotice}
        </p>
      )}
      <p className="field-help">
        StructX auto-detects the funding manager for the connected wallet and
        spends premium from there during mint.
      </p>

      <div className="ticket-row">
        <label className="field">
          <span className="field-label">Budget</span>
          <div className="input-suffix">
            <input
              value={budgetDUSDC}
              onChange={(event) => onBudgetChange(event.target.value)}
              inputMode="decimal"
              aria-label="Budget in dUSDC"
            />
            <span>dUSDC</span>
          </div>
          <span className="field-help">
            Max premium you are willing to pay for this strategy.
          </span>
        </label>
      </div>

      <div className="ticket-row">
        <label className="field">
          <span className="field-label">Style</span>
          <StyleSegmentedControl value={style} onChange={onStyleChange} />
        </label>
      </div>

      {(strategyId === "PORTFOLIO_CRASH_SHIELD" ||
        strategyId === "SMART_BUDGET_SELECTOR") && (
        <div className="ticket-row">
          <label className="field">
            <span className="field-label">Portfolio exposure</span>
            <div className="input-suffix">
              <input
                value={portfolioExposureDUSDC}
                onChange={(event) =>
                  onPortfolioExposureChange(event.target.value)
                }
                inputMode="decimal"
                aria-label="Portfolio exposure in dUSDC"
              />
              <span>dUSDC</span>
            </div>
            <span className="field-help">
              Approximate BTC-linked portfolio exposure to hedge.
            </span>
          </label>

          <label className="field">
            <span className="field-label">Max over-hedge</span>
            <div className="input-suffix">
              <input
                value={overHedgeCapBps}
                onChange={(event) => onOverHedgeCapBpsChange(event.target.value)}
                inputMode="numeric"
                aria-label="Max over hedge in basis points"
              />
              <span>bps</span>
            </div>
            <span className="field-help">
              Caps how much payout can exceed estimated bucket loss.
            </span>
          </label>
        </div>
      )}

      {(strategyId === "CONVEX_TAIL_LADDER" ||
        strategyId === "SMART_BUDGET_SELECTOR") && (
        <div className="ticket-row">
          <label className="field">
            <span className="field-label">Dead zone</span>
            <div className="input-suffix">
              <input
                value={deadZoneBps}
                onChange={(event) => onDeadZoneBpsChange(event.target.value)}
                inputMode="numeric"
                aria-label="Dead zone in basis points"
              />
              <span>bps</span>
            </div>
            <span className="field-help">
              Moderate moves below this band get less allocation.
            </span>
          </label>

          <label className="field">
            <span className="field-label">Convexity gamma</span>
            <div className="input-suffix">
              <input
                value={convexGammaBps}
                onChange={(event) => onConvexGammaBpsChange(event.target.value)}
                inputMode="numeric"
                aria-label="Convexity gamma in basis points"
              />
              <span>bps</span>
            </div>
            <span className="field-help">
              Higher gamma shifts more budget into extreme tails.
            </span>
          </label>
        </div>
      )}

      {(strategyId === "MOONSHOT_UPSIDE" ||
        strategyId === "SMART_BUDGET_SELECTOR") && (
        <div className="ticket-row">
          <label className="field">
            <span className="field-label">Range allocation weight</span>
            <div className="input-suffix">
              <input
                value={moonshotRangeWeightBps}
                onChange={(event) =>
                  onMoonshotRangeWeightBpsChange(event.target.value)
                }
                inputMode="numeric"
                aria-label="Range allocation weight in basis points"
              />
              <span>bps</span>
            </div>
            <span className="field-help">
              Splits budget between the upside breakout zone and the moonshot tail.
            </span>
          </label>

          <label className="field">
            <span className="field-label">Tail convexity gamma</span>
            <div className="input-suffix">
              <input
                value={moonshotTailGammaBps}
                onChange={(event) =>
                  onMoonshotTailGammaBpsChange(event.target.value)
                }
                inputMode="numeric"
                aria-label="Moonshot tail gamma in basis points"
              />
              <span>bps</span>
            </div>
            <span className="field-help">
              Higher gamma pushes more weight into the far-upside tail.
            </span>
          </label>
        </div>
      )}

      {strategyId === "UPSIDE_STEP_LADDER" && (
        <div className="ticket-row">
          <label className="field">
            <span className="field-label">Near upside weight</span>
            <div className="input-suffix">
              <input
                value={upsideNearRangeWeightBps}
                onChange={(event) =>
                  onUpsideNearRangeWeightBpsChange(event.target.value)
                }
                inputMode="numeric"
                aria-label="Near upside weight in basis points"
              />
              <span>bps</span>
            </div>
            <span className="field-help">
              Budget allocated to the first grind-higher upside step.
            </span>
          </label>

          <label className="field">
            <span className="field-label">Upper range weight</span>
            <div className="input-suffix">
              <input
                value={upsideUpperRangeWeightBps}
                onChange={(event) =>
                  onUpsideUpperRangeWeightBpsChange(event.target.value)
                }
                inputMode="numeric"
                aria-label="Upper range weight in basis points"
              />
              <span>bps</span>
            </div>
            <span className="field-help">
              Budget allocated to the higher breakout range before the tail.
            </span>
          </label>

          <label className="field">
            <span className="field-label">Tail convexity gamma</span>
            <div className="input-suffix">
              <input
                value={upsideTailGammaBps}
                onChange={(event) =>
                  onUpsideTailGammaBpsChange(event.target.value)
                }
                inputMode="numeric"
                aria-label="Upside continuation tail gamma in basis points"
              />
              <span>bps</span>
            </div>
            <span className="field-help">
              Higher gamma pushes more weight into the continuation-up tail.
            </span>
          </label>
        </div>
      )}

      {strategyId === "DOWNSIDE_STEP_LADDER" && (
        <div className="ticket-row">
          <label className="field">
            <span className="field-label">Near downside weight</span>
            <div className="input-suffix">
              <input
                value={downsideNearRangeWeightBps}
                onChange={(event) =>
                  onDownsideNearRangeWeightBpsChange(event.target.value)
                }
                inputMode="numeric"
                aria-label="Near downside weight in basis points"
              />
              <span>bps</span>
            </div>
            <span className="field-help">
              Budget allocated to the first grind-lower downside step.
            </span>
          </label>

          <label className="field">
            <span className="field-label">Lower range weight</span>
            <div className="input-suffix">
              <input
                value={downsideLowerRangeWeightBps}
                onChange={(event) =>
                  onDownsideLowerRangeWeightBpsChange(event.target.value)
                }
                inputMode="numeric"
                aria-label="Lower downside range weight in basis points"
              />
              <span>bps</span>
            </div>
            <span className="field-help">
              Budget allocated to the lower breakdown range before the tail.
            </span>
          </label>

          <label className="field">
            <span className="field-label">Tail convexity gamma</span>
            <div className="input-suffix">
              <input
                value={downsideStepTailGammaBps}
                onChange={(event) =>
                  onDownsideStepTailGammaBpsChange(event.target.value)
                }
                inputMode="numeric"
                aria-label="Downside continuation tail gamma in basis points"
              />
              <span>bps</span>
            </div>
            <span className="field-help">
              Higher gamma pushes more weight into the continuation-down tail.
            </span>
          </label>
        </div>
      )}

      {strategyId === "CENTER_BAND_CONDOR" && (
        <div className="ticket-row">
          <label className="field">
            <span className="field-label">Center band weight</span>
            <div className="input-suffix">
              <input
                value={condorCenterWeightBps}
                onChange={(event) =>
                  onCondorCenterWeightBpsChange(event.target.value)
                }
                inputMode="numeric"
                aria-label="Center band weight in basis points"
              />
              <span>bps</span>
            </div>
            <span className="field-help">
              More weight concentrates premium into the two center corridor ranges.
            </span>
          </label>
        </div>
      )}

      {strategyId === "NEAR_BARRIER_PROXY" && (
        <div className="ticket-row">
          <label className="field">
            <span className="field-label">Barrier side</span>
            <select
              value={barrierSide}
              onChange={(event) => onBarrierSideChange(event.target.value as "up" | "down")}
              aria-label="Barrier side"
            >
              <option value="up">Up barrier</option>
              <option value="down">Down barrier</option>
            </select>
            <span className="field-help">
              Chooses whether the proxy is built around the upside or downside barrier.
            </span>
          </label>

          <label className="field">
            <span className="field-label">Near-barrier range weight</span>
            <div className="input-suffix">
              <input
                value={barrierNearRangeWeightBps}
                onChange={(event) =>
                  onBarrierNearRangeWeightBpsChange(event.target.value)
                }
                inputMode="numeric"
                aria-label="Near barrier range weight in basis points"
              />
              <span>bps</span>
            </div>
            <span className="field-help">
              Allocates more of the budget to the near-barrier range than the far tail.
            </span>
          </label>

          <label className="field">
            <span className="field-label">Tail convexity gamma</span>
            <div className="input-suffix">
              <input
                value={barrierTailGammaBps}
                onChange={(event) => onBarrierTailGammaBpsChange(event.target.value)}
                inputMode="numeric"
                aria-label="Barrier tail gamma in basis points"
              />
              <span>bps</span>
            </div>
            <span className="field-help">
              Controls how aggressively the beyond-barrier tail is sized.
            </span>
          </label>
        </div>
      )}

      <button
        type="button"
        className="advanced-toggle"
        aria-expanded={advancedOpen}
        onClick={() => setAdvancedOpen((v) => !v)}
      >
        {advancedOpen ? "Hide" : "Show"} advanced settings
      </button>

      {advancedOpen && (
        <div className="ticket-row">
          <label className="field">
            <span className="field-label">Slippage</span>
            <div className="input-suffix">
              <input
                value={slippageBps}
                onChange={(event) => onSlippageChange(event.target.value)}
                inputMode="numeric"
                aria-label="Slippage in basis points"
              />
              <span>bps</span>
            </div>
            <span className="field-help">
              Applied as a slippage guard before transaction build.
            </span>
          </label>
          <label className="field">
            <span className="field-label">Expiry preference</span>
            <select value="nearest_active" disabled>
              <option value="nearest_active">Nearest active</option>
            </select>
            <span className="field-help">
              Expiry selection becomes editable in a future milestone.
            </span>
          </label>
        </div>
      )}

      <button
        type="button"
        className="primary-button"
        onClick={onCompile}
        disabled={compileDisabled}
      >
        {compiling ? "Compiling…" : "Preview payoff"}
      </button>

      {disabledReason && compileDisabled && (
        <p className="hint">{disabledReason}</p>
      )}
    </section>
  );
}
