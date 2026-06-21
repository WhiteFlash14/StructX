"use client";

import {
  useCurrentAccount,
  useSignAndExecuteTransaction,
  useSuiClient,
  useSuiClientContext,
} from "@mysten/dapp-kit";
import { useEffect, useState } from "react";

import {
  ApiError,
  auditIntentExecution,
  buildIntentExecutePlan,
  buildOpenStrategy,
  getManagerBalance,
  getStoredManager,
  invalidateManagerBalance,
  planFromIntent,
  quoteIntentPlan,
  saveStoredManager,
  type AuditIntentExecutionResponse,
  type CompiledProposalLeg,
  type ExecutionProposal,
  type IntentExecutePlanResponse,
  type IntentPlanningResponse,
  type RiskStyle,
} from "@/lib/api";
import {
  buildCreateManagerTransaction,
  buildDepositAndOpenStrategyTransaction,
  fetchWalletDusdcBalance,
  PREDICT_MANAGER_TYPE,
  requiredManagerReserveRaw,
} from "@/lib/tx";
import { Select } from "@/components/ui/Select";

type QuickPrompt = {
  label: string;
  prompt: string;
  tone: "bullish" | "protection" | "range" | "selector";
};

const QUICK_PROMPTS: ReadonlyArray<QuickPrompt> = [
  {
    label: "Bullish week",
    prompt: "I think BTC will pump this week",
    tone: "bullish",
  },
  {
    label: "Crash hedge",
    prompt: "Protect me if bitcoin crashes this week",
    tone: "protection",
  },
  {
    label: "Range view",
    prompt: "BTC between 100k and 110k with 25 dUSDC",
    tone: "range",
  },
  {
    label: "Best for budget",
    prompt: "Pick the best BTC trade for 100 dUSDC",
    tone: "selector",
  },
];

function QuickPromptIcon({ tone }: { tone: QuickPrompt["tone"] }) {
  const p = {
    width: 16,
    height: 16,
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.8,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
  };
  switch (tone) {
    case "bullish":
      return (
        <svg {...p} aria-hidden>
          <path d="M3 17l6-6 4 4 8-8" />
          <path d="M14 7h7v7" />
        </svg>
      );
    case "protection":
      return (
        <svg {...p} aria-hidden>
          <path d="M12 3l8 3v6c0 5-3.4 8.3-8 9-4.6-.7-8-4-8-9V6l8-3z" />
          <path d="M9 12l2 2 4-4" />
        </svg>
      );
    case "range":
      return (
        <svg {...p} aria-hidden>
          <path d="M3 12h4l3-7 4 14 3-7h4" />
        </svg>
      );
    case "selector":
      return (
        <svg {...p} aria-hidden>
          <path d="M12 3l1.6 4.4L18 9l-4.4 1.6L12 15l-1.6-4.4L6 9l4.4-1.6L12 3z" />
          <path d="M18 16l.7 1.8L20.5 18l-1.8.7L18 20.5l-.7-1.8L15.5 18l1.8-.7L18 16z" />
        </svg>
      );
    default:
      return null;
  }
}

export function NormalModeIntentPanel() {
  const account = useCurrentAccount();
  const suiClient = useSuiClient();
  const suiContext = useSuiClientContext();
  const { mutateAsync: signAndExecuteTransaction } = useSignAndExecuteTransaction();
  const [prompt, setPrompt] = useState<string>(QUICK_PROMPTS[0].prompt);
  const [budgetDusdc, setBudgetDusdc] = useState("100");
  const [riskStyle, setRiskStyle] = useState<RiskStyle>("balanced");
  const [response, setResponse] = useState<IntentPlanningResponse | null>(null);
  const [proposal, setProposal] = useState<ExecutionProposal | null>(null);
  const [executePlan, setExecutePlan] = useState<IntentExecutePlanResponse | null>(null);
  const [auditResult, setAuditResult] = useState<AuditIntentExecutionResponse | null>(null);
  const [managerId, setManagerId] = useState<string | null>(null);
  const [managerStatus, setManagerStatus] = useState<
    "idle" | "checking" | "ready" | "creating" | "error"
  >("idle");
  const [managerError, setManagerError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    async function loadManager() {
      setResponse(null);
      setProposal(null);
      setExecutePlan(null);
      setAuditResult(null);
      setError(null);
      setManagerError(null);

      if (!account?.address) {
        setManagerId(null);
        return;
      }

      setManagerId(null);
      setManagerStatus("checking");


      try {
        const network = suiContext.network ?? "testnet";
        const storageKey = `structx:manager:${network}:${account.address.toLowerCase()}`;
        let cached: string | null = null;
        try {
          cached = window.localStorage.getItem(storageKey);
        } catch {
          cached = null;
        }

        const stored = cached ?? (await getStoredManager(account.address));
        if (!cancelled) {
          setManagerId(stored);
          setManagerStatus(stored ? "ready" : "idle");
        }
      } catch {
        if (!cancelled) {
          setManagerId(null);
          setManagerStatus("idle");
        }
      }
    }

    void loadManager();

    return () => {
      cancelled = true;
    };
 }, [account?.address, suiContext.network]);

  async function findHistoricalManagerIds(address: string): Promise<string[]> {
    const ids = new Set<string>();
    let cursor: string | null | undefined = null;

    for (let pageIndex = 0; pageIndex < 4; pageIndex += 1) {
      const page = await suiClient.queryTransactionBlocks({
        filter: { FromAddress: address },
        options: { showObjectChanges: true },
        limit: 50,
        cursor,
        order: "descending",
      });

      for (const tx of page.data) {
        for (const change of tx.objectChanges ?? []) {
          if (
            change.type === "created" &&
            change.objectType === PREDICT_MANAGER_TYPE &&
            typeof change.objectId === "string"
          ) {
            ids.add(change.objectId);
          }
        }
      }

      if (!page.hasNextPage || !page.nextCursor) break;
      cursor = page.nextCursor;
    }

    return Array.from(ids);
  }

  function persistManager(address: string, id: string) {
    const network = suiContext.network ?? "testnet";
    try {
      window.localStorage.setItem(
        `structx:manager:${network}:${address.toLowerCase()}`,
        id,
      );
    } catch {
      // The backend mapping still keeps the manager available when browser
      // storage is disabled.
    }
    void saveStoredManager(address, id);
    setManagerId(id);
    setManagerStatus("ready");
    setManagerError(null);
  }

  async function ensurePredictManager(address: string): Promise<string> {
    if (managerId) {
      invalidateManagerBalance(managerId);
      const current = await getManagerBalance(managerId);
      if (current.ok && current.balanceRaw !== undefined) {
        return managerId;
      }
    }

    if ((suiContext.network ?? "testnet") !== "testnet") {
      throw new Error("Switch your wallet to Sui Testnet before opening a position.");
    }

    setManagerStatus("checking");
    setManagerError(null);

    const candidates = new Set<string>();
    const storageKey = `structx:manager:testnet:${address.toLowerCase()}`;
    try {
      const cached = window.localStorage.getItem(storageKey);
      if (cached?.startsWith("0x")) candidates.add(cached);
    } catch {
      // Continue with backend and on-chain discovery.
    }

    const stored = await getStoredManager(address);
    if (stored?.startsWith("0x")) candidates.add(stored);

    try {
      for (const id of await findHistoricalManagerIds(address)) {
        candidates.add(id);
      }
    } catch {
      // A temporary history lookup failure should not block manager creation.
    }

    const validManagers: Array<{ id: string; balanceRaw: bigint }> = [];
    for (const id of candidates) {
      invalidateManagerBalance(id);
      try {
        const balance = await getManagerBalance(id);
        if (balance.ok && balance.balanceRaw !== undefined) {
          validManagers.push({ id, balanceRaw: BigInt(balance.balanceRaw) });
        }
      } catch {
        // Ignore stale manager ids and continue checking the other candidates.
      }
    }

    if (validManagers.length > 0) {
      validManagers.sort((left, right) =>
        left.balanceRaw === right.balanceRaw
          ? 0
          : left.balanceRaw > right.balanceRaw
            ? -1
            : 1,
      );
      const selected = validManagers[0].id;
      persistManager(address, selected);
      return selected;
    }

    setManagerStatus("creating");
    const createTx = buildCreateManagerTransaction(address);
    const execution = await signAndExecuteTransaction({
      transaction: createTx,
      chain: "sui:testnet",
    });
    const confirmed = await suiClient.waitForTransaction({
      digest: execution.digest,
      options: { showEffects: true, showObjectChanges: true },
    });

    if (confirmed.effects?.status?.status !== "success") {
      throw new Error(
        confirmed.effects?.status?.error ?? "PredictManager creation failed.",
      );
    }

    const created = (confirmed.objectChanges ?? []).find(
      (change) =>
        change.type === "created" &&
        typeof change.objectType === "string" &&
        change.objectType === PREDICT_MANAGER_TYPE,
    );
    const createdId =
      created && "objectId" in created && typeof created.objectId === "string"
        ? created.objectId
        : null;

    if (!createdId) {
      throw new Error(
        "The manager transaction succeeded, but its new object id was missing from the Sui response.",
      );
    }

    persistManager(address, createdId);
    return createdId;
  }
  async function handlePlan() {
    setLoading(true);
    setError(null);
    setResponse(null);
    setProposal(null);
    setExecutePlan(null);
    setAuditResult(null);

    try {
      const budget = Number(budgetDusdc);
      if (!Number.isFinite(budget) || budget <= 0) {
        throw new Error("Enter a dUSDC budget greater than zero.");
      }
      if (budget * 1_000_000_000 > Number.MAX_SAFE_INTEGER) {
        throw new Error("This budget is too large to process safely.");
      }
      const result = await planFromIntent({
        userAddress: account?.address,
        prompt,
        budget: Number.isFinite(budget)
          ? Math.round(budget * 1_000_000_000)
          : undefined,
        quoteAsset: "DUSDC",
        riskStyle,
      });
      setResponse(result);
    } catch (err) {
      if (err instanceof ApiError) {
        setError(err.body.message ?? err.body.error ?? err.message);
      } else if (err instanceof Error) {
        setError(err.message);
      } else {
        setError(String(err));
      }
    } finally {
      setLoading(false);
    }
  }

  async function quoteCurrentIntent(
    source: IntentPlanningResponse,
  ): Promise<ExecutionProposal> {
    return quoteIntentPlan({
      userAddress: account?.address,
      intentPlan: source.intent_plan,
      selectedMarketId: source.selected_market?.market_id,
      budget: source.intent_plan.budget ?? undefined,
      maxQuoteAgeMs: 15_000,
    });
  }

  async function handleQuote() {
    if (!response) return;

    setLoading(true);
    setError(null);
    setProposal(null);
    setExecutePlan(null);
    setAuditResult(null);

    try {
      const result = await quoteCurrentIntent(response);
      setProposal(result);
    } catch (err) {
      if (err instanceof ApiError) {
        setError(err.body.message ?? err.body.error ?? err.message);
      } else if (err instanceof Error) {
        setError(err.message);
      } else {
        setError(String(err));
      }
    } finally {
      setLoading(false);
    }
  }

  async function handlePrepareExecution() {
    if (!proposal) return;

    setLoading(true);
    setError(null);
    setExecutePlan(null);
    setAuditResult(null);

    try {
      let activeProposal = proposal;
      let result: IntentExecutePlanResponse;

      try {
        result = await buildIntentExecutePlan({
          proposalId: activeProposal.proposal_id,
          userAddress: account?.address,
        });
      } catch (err) {
        const message =
          err instanceof ApiError
            ? err.body.message ?? err.body.error ?? err.message
            : err instanceof Error
              ? err.message
              : String(err);

        if (!message.toLowerCase().includes("proposal quote expired") || !response) {
          throw err;
        }

        const refreshedProposal = await quoteCurrentIntent(response);
        setProposal(refreshedProposal);
        activeProposal = refreshedProposal;

        result = await buildIntentExecutePlan({
          proposalId: activeProposal.proposal_id,
          userAddress: account?.address,
        });
      }

      setExecutePlan(result);
    } catch (err) {
      if (err instanceof ApiError) {
        setError(err.body.message ?? err.body.error ?? err.message);
      } else if (err instanceof Error) {
        setError(err.message);
      } else {
        setError(String(err));
      }
    } finally {
      setLoading(false);
    }
  }

  async function handleSignAndExecute() {
    if (!executePlan) return;

    if (!account?.address) {
      setError("Connect a wallet before signing.");
      return;
    }


    const compiledStrategyId = executePlan.compiled_strategy_id;
    if (!compiledStrategyId) {
      setError("Missing compiled strategy id in execute-plan response.");
      return;
    }

    setLoading(true);
    setError(null);
    setAuditResult(null);

    try {
      let activeManagerId: string;
      try {
        activeManagerId = await ensurePredictManager(account.address);
      } catch (managerSetupError) {
        const message =
          managerSetupError instanceof Error
            ? managerSetupError.message
            : String(managerSetupError);
        setManagerStatus("error");
        setManagerError(message);
        throw managerSetupError;
      }

      setManagerStatus("ready");
      setManagerError(null);
      const build = await buildOpenStrategy({
        owner: account.address,
        managerId:activeManagerId,
        compiledStrategyId,
        maxPremiumRaw: String(executePlan.proposal.total_premium || 0),
        slippageBps: 300,
      });

      invalidateManagerBalance(activeManagerId);
      const balance = await getManagerBalance(activeManagerId);
      if (!balance.ok || balance.balanceRaw === undefined) {
        throw new Error(
          balance.error ?? "Could not read the latest PredictManager balance.",
        );
      }
      const managerBalanceRaw = BigInt(balance.balanceRaw);
      const reserveRaw = requiredManagerReserveRaw(build);
      const depositRaw =
        reserveRaw > managerBalanceRaw ? reserveRaw - managerBalanceRaw : 0n;
      const wallet = await fetchWalletDusdcBalance(
        suiClient,
        account.address,
        depositRaw > 0n ? depositRaw : undefined,
      );
      if (wallet.totalRaw < depositRaw) {
        throw new Error(
          `This strategy needs ${depositRaw} raw dUSDC from your wallet, but only ${wallet.totalRaw} is available.`,
        );
      }
      const transactionArgs = {
        payload: build,
        depositRaw,
        walletDusdcCoinIds: depositRaw > 0n ? wallet.coinObjectIds : [],
      };
      const preflightTx = buildDepositAndOpenStrategyTransaction(transactionArgs);
      const preflight = await suiClient.devInspectTransactionBlock({
        sender: account.address,
        transactionBlock: preflightTx,
      });
      if (preflight.effects?.status?.status !== "success") {
        throw new Error(
          preflight.effects?.status?.error ??
            "The final transaction check did not succeed.",
        );
      }

      const tx = buildDepositAndOpenStrategyTransaction(transactionArgs);
      const execution = await signAndExecuteTransaction({
        transaction: tx,
        chain: "sui:testnet",
      });

      const confirmed = await suiClient.waitForTransaction({
        digest: execution.digest,
        options: {
          showEffects: true,
          showEvents: true,
          showObjectChanges: true,
        },
      });

      if (confirmed.effects?.status?.status !== "success") {
        throw new Error(
          confirmed.effects?.status?.error ?? "Transaction failed without detailed status.",
        );
      }

      const audit = await auditIntentExecution({
        proposalId: executePlan.proposal_id,
        txDigest: execution.digest,
        userAddress: account.address,
        managerId:activeManagerId,
        executionResult: {
          digest: execution.digest,
          effects: confirmed.effects ?? {},
          events: confirmed.events ?? [],
          objectChanges: confirmed.objectChanges ?? [],
        },
      });

      setAuditResult(audit);
    } catch (err) {
      if (err instanceof ApiError) {
        setError(err.body.message ?? err.body.error ?? err.message);
      } else if (err instanceof Error) {
        setError(err.message);
      } else {
        setError(String(err));
      }
    } finally {
      setLoading(false);
    }
  }

  function formatDusdcMicro(raw?: number | null) {
    if (raw == null) return "Unavailable";
    const value = raw / 1_000_000;
    return `${value.toLocaleString(undefined, {
      maximumFractionDigits: 6,
    })} DUSDC`;
  }

  function formatBudgetDisplay(raw?: number | null) {
    if (raw == null) return "Unavailable";
    return `${raw / 1_000_000_000} dUSDC`;
  }

  function formatBtcPrice(raw?: number | null) {
    if (raw == null) return null;
    return `$${raw.toLocaleString(undefined, { maximumFractionDigits: 2 })}`;
  }

  function humanizeLegKind(kind: string) {
    switch (kind.toUpperCase()) {
      case "RANGE":
        return "Range leg";
      case "UP":
        return "Up leg";
      case "DOWN":
        return "Down leg";
      default:
        return `${kind.replaceAll("_", " ")} leg`;
    }
  }

  function legSettlementCondition(leg: CompiledProposalLeg) {
    const kind = leg.kind.toUpperCase();
    const strike = formatBtcPrice(leg.strike);
    const lower = formatBtcPrice(leg.lower);
    const upper = formatBtcPrice(leg.upper);

    if (kind === "RANGE" && lower && upper) {
      return `Pays when BTC settles above ${lower} and at or below ${upper}`;
    }
    if (kind === "UP" && strike) {
      return `Pays when BTC settles at or above ${strike}`;
    }
    if (kind === "DOWN" && strike) {
      return `Pays when BTC settles at or below ${strike}`;
    }
    return "Its result is determined by BTC's final price at expiry";
  }

  function humanizeTemplate(value: string) {
    switch (value) {
      case "directional_above":
        return "Upside view";
      case "directional_below":
        return "Downside view";
      case "range_inside":
        return "Stays in a range";
      case "breakout_outside":
        return "Big move either way";
      case "one_sided_tail":
        return "Crash protection";
      case "upside_rocket":
        return "High-upside bet";
      case "custom_piecewise":
        return "Custom payoff";
      case "smart_budget":
        return "Best fit for budget";
      default:
        return value.replaceAll("_", " ");
    }
  }

  function humanizeDirection(value?: string | null) {
    switch (value) {
      case "up":
        return "Up";
      case "down":
        return "Down";
      case "either_side":
        return "Either direction";
      case "inside_range":
        return "Inside a range";
      default:
        return "Flexible";
    }
  }

  function humanizeConfidence(value: string) {
    switch (value) {
      case "high":
        return "High";
      case "medium":
        return "Medium";
      case "low":
        return "Low";
      default:
        return value;
    }
  }

  function humanizeRisk(value: RiskStyle) {
    switch (value) {
      case "conservative":
        return "Conservative";
      case "balanced":
        return "Balanced";
      case "aggressive":
        return "Aggressive";
      case "tail_heavy":
        return "Big payout, lower hit rate";
      case "higher_hit_rate":
        return "Higher chance, lower upside";
      default:
        return value;
    }
  }

  function humanizeMarketKind(value: string) {
    switch (value) {
      case "scalar_price":
        return "Price market";
      case "scalar_event":
        return "Outcome market";
      case "binary_event":
        return "Yes / no market";
      case "categorical_event":
        return "Multiple-choice market";
      default:
        return value.replaceAll("_", " ");
    }
  }

  function humanizeMarketStatus(value: string) {
    switch (value) {
      case "active":
        return "Live";
      case "pending_settlement":
        return "Waiting for settlement";
      case "settled":
        return "Settled";
      case "expired_unknown":
        return "Expired";
      case "inactive":
        return "Inactive";
      default:
        return value.replaceAll("_", " ");
    }
  }

  function effectiveMarketStatus(status: string, expiryMs?: number) {
    if (expiryMs && expiryMs <= Date.now() && status === "active") {
      return "expired_unknown";
    }

    return status;
  }

  function formatExpiry(ms?: number) {
    if (!ms) return "Unavailable";
    const dateLabel = new Date(ms).toLocaleString();

    if (ms <= Date.now()) {
      return `Expired on ${dateLabel}`;
    }

    return dateLabel;
  }

  return (
    <section className="normal-panel">
      <div className="normal-stage">
        <div className="normal-stage-head">
          <h2 className="normal-stage-title">What&apos;s your BTC view?</h2>
          <p className="normal-stage-sub">
            Describe the move you expect in your own words. StructX will find a
            matching DeepBook Predict market and show the strategy, price, and
            payoff before your wallet opens it.
          </p>
        </div>

        <div className="normal-prompt-block">
          <label htmlFor="normal-prompt-input" className="normal-prompt-label">
            Your view
          </label>
          <textarea
            id="normal-prompt-input"
            className="normal-prompt-input"
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
            rows={3}
            placeholder="e.g. I think BTC will pump this week with 100 dUSDC"
          />

          <div className="normal-suggested">
            <span className="normal-suggested-label">Try one of these</span>
            <div className="normal-suggested-row">
              {QUICK_PROMPTS.map((quickPrompt) => (
                <button
                  key={quickPrompt.prompt}
                  type="button"
                  className={`normal-suggested-card tone-${quickPrompt.tone}${prompt === quickPrompt.prompt ? " active" : ""}`}
                  onClick={() => setPrompt(quickPrompt.prompt)}
                >
                  <span className="normal-suggested-icon" aria-hidden>
                    <QuickPromptIcon tone={quickPrompt.tone} />
                  </span>
                  <span className="normal-suggested-body">
                    <span className="normal-suggested-name">{quickPrompt.label}</span>
                    <span className="normal-suggested-text">{quickPrompt.prompt}</span>
                  </span>
                </button>
              ))}
            </div>
          </div>
        </div>

        <div className="normal-controls">
          <label className="normal-control">
            <span className="normal-control-label">Budget</span>
            <div className="normal-control-input has-suffix">
              <input
                value={budgetDusdc}
                onChange={(e) => setBudgetDusdc(e.target.value)}
                inputMode="decimal"
                aria-label="Budget in dUSDC"
              />
              <span className="normal-control-suffix">dUSDC</span>
            </div>
          </label>

          <label className="normal-control">
            <span className="normal-control-label">Risk</span>
            <div className="normal-control-input">
              <Select
                value={riskStyle}
                onChange={(v) => setRiskStyle(v as RiskStyle)}
                options={[
                  { value: "conservative", label: "Conservative" },
                  { value: "balanced", label: "Balanced" },
                  { value: "aggressive", label: "Aggressive" },
                  { value: "tail_heavy", label: "Tail heavy" },
                  { value: "higher_hit_rate", label: "Higher hit rate" },
                ]}
                ariaLabel="Risk style"
                fullWidth
              />
            </div>
          </label>

          <label className="normal-control">
            <span className="normal-control-label">Quote asset</span>
            <div className="normal-control-input is-locked">
              <input value="DUSDC" disabled aria-label="Quote asset" />
              <span className="normal-control-locked-icon" aria-hidden>
                <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <rect x="4" y="11" width="16" height="10" rx="2" />
                  <path d="M8 11V7a4 4 0 1 1 8 0v4" />
                </svg>
              </span>
            </div>
          </label>
        </div>

        <div className="normal-cta-row">
          <button
            type="button"
            className="normal-generate"
            disabled={loading || !prompt.trim()}
            onClick={() => void handlePlan()}
          >
            {loading ? (
              <>
                <span className="normal-generate-spinner" aria-hidden />
                Planning your strategy
              </>
            ) : (
              <>
                Plan my strategy
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
                  <path d="M5 12h14M13 5l7 7-7 7" />
                </svg>
              </>
            )}
          </button>
        </div>
      </div>

      <div className="normal-output">
          {!response && !loading && !error && (
            <div className="normal-card">
              <div className="normal-card-eyebrow">
                <span className="normal-step-num small">3</span>
                Your strategy will appear here
              </div>
              <h3>Start with your BTC view</h3>
              <p className="normal-reason">
                StructX will show what it understood, the market it found, and
                the live strategy that matches your view.
              </p>
            </div>
          )}

          {error && (
            <div className="normal-error">
              <strong>
                {error.toLowerCase().includes("expired")
                  ? "The quote needs a refresh"
                  : "Need a bit more info"}
              </strong>
              <p>{error}</p>
            </div>
          )}

          {response && (
            <>
              {response.needs_clarification && (
                <div className="normal-error">
                  <strong>Clarification needed</strong>
                  <p>{response.clarification_question}</p>
                </div>
              )}

              <div className="normal-card">
                <div className="normal-card-eyebrow">
                  <span className="normal-step-num small">3</span>
                  What StructX understood
                </div>
                <h3>{response.intent_plan.market_query || "Unresolved market"}</h3>
                <dl className="normal-meta">
                  <dt>Trade idea</dt>
                  <dd>{humanizeTemplate(response.intent_plan.strategy_template)}</dd>
                  <dt>Market view</dt>
                  <dd>{humanizeDirection(response.intent_plan.direction)}</dd>
                  <dt>Confidence</dt>
                  <dd>{humanizeConfidence(response.intent_plan.confidence)}</dd>
                  <dt>Budget</dt>
                  <dd className="mono">{formatBudgetDisplay(response.intent_plan.budget)}</dd>
                  <dt>Risk</dt>
                  <dd>{humanizeRisk(response.intent_plan.risk_style)}</dd>
                </dl>
                {response.intent_plan.clarification_question && (
                  <p className="normal-reason">
                    {response.intent_plan.clarification_question}
                  </p>
                )}
              </div>

              {response.selected_market && (
                <div className="normal-card normal-card-recommend">
                  <div className="normal-card-eyebrow">
                    <span className="normal-step-num small">4</span>
                    Selected market
                  </div>
                  <h3>{response.selected_market.display_name}</h3>
                  <dl className="normal-meta">
                    <dt>Asset</dt>
                    <dd>{response.selected_market.underlying}</dd>
                    <dt>Market type</dt>
                    <dd>{humanizeMarketKind(response.selected_market.market_kind)}</dd>
                    <dt>Status</dt>
                    <dd>
                      {humanizeMarketStatus(
                        effectiveMarketStatus(
                          response.selected_market.status,
                          response.selected_market.expiry_ms,
                        ),
                      )}
                    </dd>
                    <dt>Expires</dt>
                    <dd>{formatExpiry(response.selected_market.expiry_ms)}</dd>
                  </dl>
                  <p className="normal-reason">
                    StructX will price the strategy using this live{" "}
                    {response.selected_market.underlying} market. The next step
                    shows the final cost and payoff.
                  </p>
                  {!response.needs_clarification && (
                    <button
                      type="button"
                      className="normal-cta"
                      disabled={loading}
                      onClick={() => void handleQuote()}
                    >
                      {loading ? "Getting the live price..." : "See live price"}
                    </button>
                  )}
                </div>
              )}

              {response.candidate_markets.length > 0 && (
                <div className="normal-card">
                  <div className="normal-card-eyebrow">Other matching markets</div>
                  <div className="normal-stats">
                    {response.candidate_markets.slice(0, 4).map((market) => (
                      <div key={market.market_id} className="normal-stat">
                        <span>{market.display_name}</span>
                        <strong>{humanizeMarketKind(market.market_kind)}</strong>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {(response.intent_plan.assumptions.length > 0 ||
                response.intent_plan.warnings.length > 0) && (
                <div className="normal-card">
                  <div className="normal-card-eyebrow">Things to know</div>
                  {response.intent_plan.assumptions.length > 0 && (
                    <div className="normal-reason">
                      Assumptions: {response.intent_plan.assumptions.join(" ")}
                    </div>
                  )}
                  {response.intent_plan.warnings.length > 0 && (
                    <div className="normal-reason">
                      Warnings: {response.intent_plan.warnings.join(" ")}
                    </div>
                  )}
                </div>
              )}

              {proposal && (
                <div className="normal-card normal-card-recommend">
                  <div className="normal-card-eyebrow">
                    <span className="normal-step-num small">5</span>
                    Your live strategy
                  </div>
                  <h3>{humanizeTemplate(proposal.strategy_template)}</h3>
                  <p className="normal-reason">{proposal.reason_for_selection}</p>

                  <div className="normal-stats">
                    <div className="normal-stat">
                      <span>Premium. The total cost of opening every leg.</span>
                      <strong>{formatDusdcMicro(proposal.total_premium)}</strong>
                    </div>
                    <div className="normal-stat">
                      <span>Max loss. The most this strategy can lose.</span>
                      <strong>{formatDusdcMicro(proposal.max_loss)}</strong>
                    </div>
                    <div className="normal-stat">
                      <span>
                        Best-case payout. The largest gross amount returned at expiry.
                      </span>
                      <strong>{formatDusdcMicro(proposal.max_payout)}</strong>
                    </div>
                  </div>

                  <div className="normal-card">
                    <div className="normal-card-eyebrow">Legs</div>
                     <p className="normal-reason">
                      A leg is one on-chain Predict position inside the strategy.
                      Its position size is the gross dUSDC returned when that leg wins.
                    </p>
                    <div className="normal-stats">
                      {proposal.legs.map((leg, idx) => (
                        <div key={`${leg.kind}-${idx}`} className="normal-stat">
                           <span>
                            {humanizeLegKind(leg.kind)}. {legSettlementCondition(leg)}.
                          </span>
                          <strong>
                            Position size {formatDusdcMicro(leg.quantity)}
                          </strong>
                        </div>
                      ))}
                    </div>
                  </div>

                  {proposal.payoff_table.length > 0 && (
                    <div className="normal-card">
                      <div className="normal-card-eyebrow">Result at expiry</div>
                      <p className="normal-reason">
                        Each value is the net result after subtracting the full
                        strategy premium. A positive amount is profit, while a
                        negative amount is loss.
                      </p>
                      <div className="normal-stats">
                        {proposal.payoff_table.map((row, idx) => (
                          <div key={`${row.label}-${idx}`} className="normal-stat">
                            <span>{row.label}</span>
                            <strong>{formatDusdcMicro(row.net_pnl)}</strong>
                          </div>
                        ))}
                      </div>
                    </div>
                  )}

                  {proposal.warnings.length > 0 && (
                    <div className="normal-card">
                      <div className="normal-card-eyebrow">Warnings</div>
                      <div className="normal-reason">
                        {proposal.warnings.join(" ")}
                      </div>
                    </div>
                  )}

                  <div className="normal-card">
                    <div className="normal-card-eyebrow">Ready to open</div>
                    <p className="normal-reason">
                      {managerId
                        ? `PredictManager ready: ${managerId}`
                        : managerStatus === "checking"
                          ? "StructX is checking this wallet for an existing PredictManager."
                          : managerStatus === "creating"
                            ? "Approve the wallet request to create your PredictManager."
                            : account?.address
                              ? "When you open the position, StructX will find your PredictManager or ask your wallet to create one."
                              : "Connect your Sui Testnet wallet before opening the position."}
                    </p>
                    {managerError && (
                      <p className="normal-reason">Manager setup: {managerError}</p>
                    )}
                    <p className="normal-reason">
                      Quotes expire quickly. If this one goes stale, StructX will refresh it before
                      preparing the wallet transaction.
                    </p>
                    <div className="normal-chips">
                      <button
                        type="button"
                        className="normal-cta"
                        disabled={loading}
                        onClick={() => void handlePrepareExecution()}
                      >
                        {loading ? "Preparing transaction..." : "Review wallet transaction"}
                      </button>

                      {executePlan && (
                        <button
                          type="button"
                          className="normal-cta"
                          disabled={loading}
                          onClick={() => void handleSignAndExecute()}
                        >
                          {loading
                            ? managerStatus === "creating"
                              ? "Creating your manager..."
                              : "Waiting for wallet..."
                            : "Review and open position"}
                        </button>
                      )}
                    </div>
                  </div>

                  {executePlan && (
                    <div className="normal-card">
                      <div className="normal-card-eyebrow">Wallet transaction ready</div>
                      <p className="normal-reason">
                        StructX checked the latest quote and prepared the full transaction for your
                        wallet to review.
                      </p>
                      {executePlan.compiled_strategy_id && (
                        <p className="normal-reason mono">
                          {executePlan.compiled_strategy_id}
                        </p>
                      )}
                      {executePlan.warnings.length > 0 && (
                        <div className="normal-reason">
                          {executePlan.warnings.join(" ")}
                        </div>
                      )}
                    </div>
                  )}

                  {auditResult && (
                    <div className="normal-card normal-card-recommend">
                      <div className="normal-card-eyebrow">Position opened</div>
                      <p className="normal-reason mono">{auditResult.audit.tx_digest}</p>
                      <p className="normal-reason">
                        Position sync: {auditResult.position_sync_status}
                      </p>
                      {auditResult.position_ids.length > 0 && (
                        <p className="normal-reason">
                          Positions: {auditResult.position_ids.join(", ")}
                        </p>
                      )}
                      {auditResult.warnings.length > 0 && (
                        <div className="normal-reason">
                          {auditResult.warnings.join(" ")}
                        </div>
                      )}
                    </div>
                  )}
                </div>
              )}
            </>
          )}
      </div>
    </section>
  );
}