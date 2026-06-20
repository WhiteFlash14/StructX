"use client";

import type { StrategyStyle } from "@/types/structx";

const styleDescriptions: Record<StrategyStyle, string> = {
  "tail-heavy": "More budget goes to far-tail protection.",
  balanced: "Balances tail and moderate breakout exposure.",
  "higher-hit-rate": "More budget goes to closer ranges with higher hit probability.",
};

type Props = {
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
  managerBalance: string | null;
  managerBalanceLoading: boolean;
  managerBalanceError: string | null;
  onRefreshBalance: () => void;
  onCompile: () => void;
  compileDisabled: boolean;
  compiling: boolean;
};

export function BreakoutForm({
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
  managerBalance,
  managerBalanceLoading,
  managerBalanceError,
  onRefreshBalance,
  onCompile,
  compileDisabled,
  compiling,
}: Props) {
  const ownerLocked = Boolean(walletAddress);

  return (
    <section className="panel">
      <div className="panel-header">
        <p className="eyebrow">Configure</p>
        <h2>Breakout Protection</h2>
        <p className="muted">
          Define payoff parameters. StructX compiles a Predict mint plan in
          your wallet. No funds are moved into a vault.
        </p>
      </div>

      <label className="field">
        Wallet owner address
        <input
          value={owner}
          onChange={(event) => onOwnerChange(event.target.value)}
          placeholder="0x..."
          disabled={ownerLocked}
        />
      </label>
      <p className="style-help">
        {ownerLocked
          ? "Owner is locked to the connected wallet address."
          : "Connect wallet to use your address automatically. Manual owner is for local preview only."}
      </p>

      <label className="field">
        PredictManager ID
        <input
          value={managerId}
          onChange={(event) => onManagerIdChange(event.target.value)}
          placeholder="0x..."
        />
      </label>

      <div className="balance-box">
        <div>
          <span>Manager balance</span>
          <strong>
            {managerBalanceLoading
              ? "Loading…"
              : managerBalance
                ? managerBalance
                : "Unavailable"}
          </strong>
        </div>
        <button
          type="button"
          className="mini-button"
          onClick={onRefreshBalance}
          disabled={managerBalanceLoading || !managerId}
        >
          {managerBalanceLoading ? "Refreshing…" : "Refresh"}
        </button>
      </div>

      {managerBalanceError && (
        <p className="hint danger">{managerBalanceError}</p>
      )}

      <div className="form-row">
        <label className="field">
          Budget
          <div className="input-suffix">
            <input
              value={budgetDUSDC}
              onChange={(event) => onBudgetChange(event.target.value)}
              inputMode="decimal"
            />
            <span>dUSDC</span>
          </div>
        </label>

        <label className="field">
          Slippage
          <div className="input-suffix">
            <input
              value={slippageBps}
              onChange={(event) => onSlippageChange(event.target.value)}
              inputMode="numeric"
            />
            <span>bps</span>
          </div>
        </label>
      </div>

      <label className="field">
        Style
        <select
          value={style}
          onChange={(event) => onStyleChange(event.target.value as StrategyStyle)}
        >
          <option value="tail-heavy">Tail-heavy</option>
          <option value="balanced">Balanced</option>
          <option value="higher-hit-rate">Higher-hit-rate</option>
        </select>
      </label>
      <p className="style-help">{styleDescriptions[style]}</p>

      <label className="field">
        Expiry preference
        <select value="nearest_active" disabled>
          <option value="nearest_active">Nearest active</option>
        </select>
      </label>
      <p className="style-help">
        Expiry selection will become user-editable in a later milestone.
      </p>

      <button
        type="button"
        className="primary-button"
        onClick={onCompile}
        disabled={compileDisabled}
      >
        {compiling ? "Compiling strategy…" : "Preview strategy"}
      </button>

      <p className="hint">
        Quotes are live and may change before signing. Testnet only.
      </p>
    </section>
  );
}
