"use client";

import {
  useCurrentAccount,
  useSignAndExecuteTransaction,
  useSuiClient,
  useSuiClientContext,
} from "@mysten/dapp-kit";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  ApiError,
  auditOpenStrategy,
  buildOpenStrategy,
  compileStrategy,
  getManagerBalance,
  getStoredManager,
  invalidateManagerBalance,
  isAbortError,
  saveStoredManager,
} from "@/lib/api";
import { mapDryRunFailure, mapError, type FriendlyError } from "@/lib/errors";
import {
  bigIntSafe,
  formatDusdcDisplay,
  formatDusdcDisplayString,
  formatPriceCompact,
  shortAddress,
} from "@/lib/format";
import {
  addSlippageBps,
  buildCreateManagerTransaction,
  buildDepositAndOpenStrategyTransaction,
  fetchWalletDusdcBalance,
  PREDICT_MANAGER_TYPE,
  requiredManagerReserveRaw,
} from "@/lib/tx";
import { WorkbenchPreviewSkeleton } from "@/components/landing/WorkbenchPreviewSkeleton";
import type {
  AuditResponse,
  CompileResponse,
  StrategyId,
  StrategyStyle,
} from "@/types/structx";

// No hardcoded owner / manager. The owner field follows the connected wallet
// and the manager id must be supplied by the user — otherwise we'd display
// another wallet's deposit balance, which is misleading.

function budgetToleranceRaw(budgetRaw: bigint): bigint {
  const percent = budgetRaw / 50n; // 2%
  const floor = 50_000n; // 0.05 dUSDC
  return percent > floor ? percent : floor;
}

function strikeTokenMap(compiled: CompileResponse): Record<string, string> {
  return {
    K1: `${(Number(compiled.strikes.k1) / 1000).toFixed(2)}K`,
    K2: `${(Number(compiled.strikes.k2) / 1000).toFixed(2)}K`,
    K3: `${(Number(compiled.strikes.k3) / 1000).toFixed(2)}K`,
    K4: `${(Number(compiled.strikes.k4) / 1000).toFixed(2)}K`,
  };
}

function compactScenarioLabel(
  condition: string,
  strikeLabels?: Record<string, string>,
): string {
  const text = condition
    .trim()
    .replace(/\b(K[1-4])\b/g, (token) => strikeLabels?.[token] ?? token);

  let match = text.match(/^BTC settles at or below (.+)$/i);
  if (match) return `BTC ≤ ${match[1]}`;

  match = text.match(/^BTC settles at or above (.+)$/i);
  if (match) return `BTC ≥ ${match[1]}`;

  match = text.match(/^BTC settles <= (.+)$/i);
  if (match) return `BTC ≤ ${match[1]}`;

  match = text.match(/^BTC settles >= (.+)$/i);
  if (match) return `BTC ≥ ${match[1]}`;

  match = text.match(/^(.+?)\s*<\s*BTC settles\s*<=\s*(.+)$/i);
  if (match) return `${match[1]} to ${match[2]}`;

  return text
    .replace(/^BTC settles\s*/i, "")
    .replace(/\s+to\s+/gi, " to ")
    .replace(/\s+/g, " ")
    .trim();
}

type CustomBandValues = {
  k1: number;
  k2: number;
  k3: number;
  k4: number;
};

function parseKValue(value: string): number | null {
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : null;
}

function formatKValue(value: number): string {
  return value.toFixed(2);
}

function readCustomBandValues(
  k1: string,
  k2: string,
  k3: string,
  k4: string,
): CustomBandValues | null {
  const values = {
    k1: parseKValue(k1),
    k2: parseKValue(k2),
    k3: parseKValue(k3),
    k4: parseKValue(k4),
  };

  if (
    values.k1 === null ||
    values.k2 === null ||
    values.k3 === null ||
    values.k4 === null
  ) {
    return null;
  }

  if (!(values.k1 < values.k2 && values.k2 < values.k3 && values.k3 < values.k4)) {
    return null;
  }

  return {
    k1: values.k1,
    k2: values.k2,
    k3: values.k3,
    k4: values.k4,
  };
}

function averageBandGapK(bands: CustomBandValues): number {
  return ((bands.k2 - bands.k1) + (bands.k3 - bands.k2) + (bands.k4 - bands.k3)) / 3;
}

function bandGapBounds(referenceGapK: number | null) {
  const base = referenceGapK && referenceGapK > 0 ? referenceGapK : 0.25;
  const min = Math.max(0.05, base * 0.3);
  const max = Math.max(min + 0.05, base * 3);
  const step = base >= 1 ? 0.05 : 0.01;
  return { min, max, step };
}

function rescaleBandSpacing(
  bands: CustomBandValues,
  targetGapK: number,
): CustomBandValues {
  const gap1 = bands.k2 - bands.k1;
  const gap2 = bands.k3 - bands.k2;
  const gap3 = bands.k4 - bands.k3;
  const currentAverage = averageBandGapK(bands);
  const scale = currentAverage > 0 ? targetGapK / currentAverage : 1;
  const scaledGap1 = gap1 * scale;
  const scaledGap2 = gap2 * scale;
  const scaledGap3 = gap3 * scale;
  const centerMid = (bands.k2 + bands.k3) / 2;
  const k2 = centerMid - scaledGap2 / 2;
  const k3 = centerMid + scaledGap2 / 2;
  const k1 = k2 - scaledGap1;
  const k4 = k3 + scaledGap3;

  return { k1, k2, k3, k4 };
}

function customBandLabels(strategyId: StrategyId) {
  switch (strategyId) {
    case "MOONSHOT_UPSIDE":
      return {
        k1: "Lower reference",
        k2: "Center reference",
        k3: "Breakout band",
        k4: "Moonshot tail",
      };
    case "UPSIDE_STEP_LADDER":
      return {
        k1: "Lower reference",
        k2: "Center reference",
        k3: "Near upside band",
        k4: "Upper upside band",
      };
    case "DOWNSIDE_STEP_LADDER":
      return {
        k1: "Crash tail band",
        k2: "Lower downside band",
        k3: "Center reference",
        k4: "Upper reference",
      };
    case "CENTER_BAND_CONDOR":
      return {
        k1: "Lower wing",
        k2: "Lower center band",
        k3: "Upper center band",
        k4: "Upper wing",
      };
    case "NEAR_BARRIER_PROXY":
      return {
        k1: "Lower downside barrier",
        k2: "Upper downside barrier",
        k3: "Lower upside barrier",
        k4: "Upper upside barrier",
      };
    default:
      return {
        k1: "Lower outer band",
        k2: "Lower inner band",
        k3: "Upper inner band",
        k4: "Upper outer band",
      };
  }
}

function BandValueFields({
  bands,
  labels,
  readOnly = false,
  onChange,
}: {
  bands: {
    k1: string;
    k2: string;
    k3: string;
    k4: string;
  };
  labels: ReturnType<typeof customBandLabels>;
  readOnly?: boolean;
  onChange?: {
    k1: (value: string) => void;
    k2: (value: string) => void;
    k3: (value: string) => void;
    k4: (value: string) => void;
  };
}) {
  return (
    <div className="wb-grid-2">
      <div className="wb-field">
        <label className="wb-label" htmlFor="wb-k1">
          {labels.k1}
        </label>
        <div className="wb-input-wrap">
          <input
            id="wb-k1"
            className="wb-input with-suffix"
            value={bands.k1}
            onChange={onChange ? (e) => onChange.k1(e.target.value) : undefined}
            inputMode="decimal"
            placeholder="64.00"
            readOnly={readOnly}
          />
          <span className="wb-input-suffix">K</span>
        </div>
      </div>
      <div className="wb-field">
        <label className="wb-label" htmlFor="wb-k2">
          {labels.k2}
        </label>
        <div className="wb-input-wrap">
          <input
            id="wb-k2"
            className="wb-input with-suffix"
            value={bands.k2}
            onChange={onChange ? (e) => onChange.k2(e.target.value) : undefined}
            inputMode="decimal"
            placeholder="64.25"
            readOnly={readOnly}
          />
          <span className="wb-input-suffix">K</span>
        </div>
      </div>
      <div className="wb-field">
        <label className="wb-label" htmlFor="wb-k3">
          {labels.k3}
        </label>
        <div className="wb-input-wrap">
          <input
            id="wb-k3"
            className="wb-input with-suffix"
            value={bands.k3}
            onChange={onChange ? (e) => onChange.k3(e.target.value) : undefined}
            inputMode="decimal"
            placeholder="64.75"
            readOnly={readOnly}
          />
          <span className="wb-input-suffix">K</span>
        </div>
      </div>
      <div className="wb-field">
        <label className="wb-label" htmlFor="wb-k4">
          {labels.k4}
        </label>
        <div className="wb-input-wrap">
          <input
            id="wb-k4"
            className="wb-input with-suffix"
            value={bands.k4}
            onChange={onChange ? (e) => onChange.k4(e.target.value) : undefined}
            inputMode="decimal"
            placeholder="65.00"
            readOnly={readOnly}
          />
          <span className="wb-input-suffix">K</span>
        </div>
      </div>
    </div>
  );
}

// Per-wallet, per-network cache for the auto-discovered / auto-created
// PredictManager id. We never share this across wallets — each connected
// address sees only its own manager.
function managerStorageKey(address: string, network: string): string {
  return `structx:manager:${network}:${address.toLowerCase()}`;
}

type DiscoverPhase =
  | { phase: "idle" }
  | { phase: "checking" }
  | { phase: "found"; source: "cache" | "backend" | "history" }
  | { phase: "creating" }
  | { phase: "created" }
  | { phase: "error"; message: string };

type ManagerCandidateSource = "cache" | "backend" | "history";

type ManagerCandidate = {
  id: string;
  source: ManagerCandidateSource;
  balanceRaw: bigint;
};

type Props = {
  strategyId: StrategyId;
  displayName: string;
  /**
   * Optional initial value for the Amount field — used by Normal Mode to
   * carry the user's chosen budget into the workbench without forcing them
   * to retype it. Defaults to "50" (the safe 4-leg minimum) when absent.
   */
  initialBudgetDUSDC?: string;
};

export function StrategyWorkbench({
  strategyId,
  displayName,
  initialBudgetDUSDC,
}: Props) {
  const account = useCurrentAccount();
  const suiClient = useSuiClient();
  const ctx = useSuiClientContext();
  const { mutateAsync: signAndExecuteTransaction } =
    useSignAndExecuteTransaction();

  const connectedAddress = account?.address ?? null;
  const isTestnet = ctx.network === "testnet";

  const [owner, setOwner] = useState("");
  const [managerId, setManagerId] = useState("");
  // 50 dUSDC is the safe minimum for the underlying 4-leg compile on live
  // DeepBook Predict markets. Smaller numbers (e.g., "from 5 dUSDC") may not
  // size to mintable quantities, so we default to 50 regardless of the hint.
  // Normal Mode can override via the initialBudgetDUSDC prop.
  const [budget, setBudget] = useState(
    initialBudgetDUSDC && initialBudgetDUSDC.trim() !== ""
      ? initialBudgetDUSDC
      : "50",
  );
  const [autoBandGapK, setAutoBandGapK] = useState("");
  const [customBandsEnabled, setCustomBandsEnabled] = useState(false);
  const [customK1K, setCustomK1K] = useState("");
  const [customK2K, setCustomK2K] = useState("");
  const [customK3K, setCustomK3K] = useState("");
  const [customK4K, setCustomK4K] = useState("");
  const [slippage, setSlippage] = useState("100");
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [portfolioExposureDUSDC, setPortfolioExposureDUSDC] = useState("500");
  const [overHedgeCapBps, setOverHedgeCapBps] = useState("12000");
  const [deadZoneBps, setDeadZoneBps] = useState("200");
  const [convexGammaBps, setConvexGammaBps] = useState("15000");
  const [moonshotRangeWeightBps, setMoonshotRangeWeightBps] = useState("6000");
  const [moonshotTailGammaBps, setMoonshotTailGammaBps] = useState("15000");
  const [upsideNearRangeWeightBps, setUpsideNearRangeWeightBps] = useState("4000");
  const [upsideUpperRangeWeightBps, setUpsideUpperRangeWeightBps] = useState("3500");
  const [upsideTailGammaBps, setUpsideTailGammaBps] = useState("15000");
  const [downsideNearRangeWeightBps, setDownsideNearRangeWeightBps] = useState("4000");
  const [downsideLowerRangeWeightBps, setDownsideLowerRangeWeightBps] = useState("3500");
  const [downsideStepTailGammaBps, setDownsideStepTailGammaBps] =
    useState("15000");
  const [condorCenterWeightBps, setCondorCenterWeightBps] = useState("8000");
  const [barrierSide, setBarrierSide] = useState<"up" | "down">("up");
  const [barrierNearRangeWeightBps, setBarrierNearRangeWeightBps] = useState("7000");
  const [barrierTailGammaBps, setBarrierTailGammaBps] = useState("15000");

  const [compiled, setCompiled] = useState<CompileResponse | null>(null);
  const [compileLoading, setCompileLoading] = useState(false);
  const [compileError, setCompileError] = useState<FriendlyError | null>(null);

  const [managerBalanceRaw, setManagerBalanceRaw] = useState<string | null>(null);
  const [managerBalanceDisplay, setManagerBalanceDisplay] = useState<string | null>(
    null,
  );
  const [managerBalanceError, setManagerBalanceError] = useState<string | null>(
    null,
  );
  const [managerBalanceLoading, setManagerBalanceLoading] = useState(false);

  const [opening, setOpening] = useState(false);
  const [openError, setOpenError] = useState<FriendlyError | null>(null);
  const [audit, setAudit] = useState<AuditResponse | null>(null);

  // Wallet dUSDC balance — needed to know whether we have to deposit any
  // shortfall before mint. Stored as raw u64 (string) like other balances.
  const [walletDusdcRaw, setWalletDusdcRaw] = useState<string | null>(null);

  const [discover, setDiscover] = useState<DiscoverPhase>({ phase: "idle" });
  const [discoverNonce, setDiscoverNonce] = useState(0);

  // Load wallet dUSDC balance whenever the connected address changes. This
  // is best-effort — failure just disables the auto-deposit decision and we
  // fall back to requiring the user to have a pre-funded manager.
  useEffect(() => {
    if (!connectedAddress) {
      setWalletDusdcRaw(null);
      return;
    }
    let cancelled = false;
    (async () => {
      try {
        const { totalRaw } = await fetchWalletDusdcBalance(
          suiClient,
          connectedAddress,
        );
        if (cancelled) return;
        setWalletDusdcRaw(totalRaw.toString());
      } catch {
        if (cancelled) return;
        setWalletDusdcRaw(null);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [connectedAddress, suiClient]);

  const style: StrategyStyle = "balanced";
  const bandLabels = useMemo(() => customBandLabels(strategyId), [strategyId]);
  const compiledBandValues = useMemo(() => {
    if (!compiled) return null;

    const k1 = parseKValue((Number(compiled.strikes.k1) / 1000).toFixed(2));
    const k2 = parseKValue((Number(compiled.strikes.k2) / 1000).toFixed(2));
    const k3 = parseKValue((Number(compiled.strikes.k3) / 1000).toFixed(2));
    const k4 = parseKValue((Number(compiled.strikes.k4) / 1000).toFixed(2));

    if (k1 === null || k2 === null || k3 === null || k4 === null) return null;

    return { k1, k2, k3, k4 };
  }, [compiled]);
  const customBandValues = useMemo(
    () => readCustomBandValues(customK1K, customK2K, customK3K, customK4K),
    [customK1K, customK2K, customK3K, customK4K],
  );
  const hasCompleteCustomBands =
    customK1K.trim() !== "" &&
    customK2K.trim() !== "" &&
    customK3K.trim() !== "" &&
    customK4K.trim() !== "";
  const activeAutoGapK = useMemo(
    () => (compiledBandValues ? averageBandGapK(compiledBandValues) : null),
    [compiledBandValues],
  );
  const activeCustomGapK = useMemo(
    () => (customBandValues ? averageBandGapK(customBandValues) : null),
    [customBandValues],
  );
  const autoGapBounds = useMemo(() => bandGapBounds(activeAutoGapK), [activeAutoGapK]);
  const customGapBounds = useMemo(
    () => bandGapBounds(activeCustomGapK),
    [activeCustomGapK],
  );
  const autoPreviewBands = useMemo(() => {
    if (!compiledBandValues) return null;
    const targetGapK = parseKValue(autoBandGapK);
    if (targetGapK === null) return compiledBandValues;
    return rescaleBandSpacing(compiledBandValues, targetGapK);
  }, [compiledBandValues, autoBandGapK]);
  const autoPreviewBandInputs = useMemo(() => {
    if (!autoPreviewBands) return null;
    return {
      k1: formatKValue(autoPreviewBands.k1),
      k2: formatKValue(autoPreviewBands.k2),
      k3: formatKValue(autoPreviewBands.k3),
      k4: formatKValue(autoPreviewBands.k4),
    };
  }, [autoPreviewBands]);

  const writeCustomBands = useCallback((next: CustomBandValues) => {
    setCustomK1K(formatKValue(next.k1));
    setCustomK2K(formatKValue(next.k2));
    setCustomK3K(formatKValue(next.k3));
    setCustomK4K(formatKValue(next.k4));
  }, []);

  useEffect(() => {
    if (!compiled || customBandsEnabled) return;

    const toK = (value: string) => (Number(value) / 1000).toFixed(2);
    const compiledGapK = formatKValue(
      averageBandGapK({
        k1: Number(toK(compiled.strikes.k1)),
        k2: Number(toK(compiled.strikes.k2)),
        k3: Number(toK(compiled.strikes.k3)),
        k4: Number(toK(compiled.strikes.k4)),
      }),
    );
    setAutoBandGapK((current) => (current.trim() === "" ? compiledGapK : current));
    setCustomK1K(toK(compiled.strikes.k1));
    setCustomK2K(toK(compiled.strikes.k2));
    setCustomK3K(toK(compiled.strikes.k3));
    setCustomK4K(toK(compiled.strikes.k4));
  }, [compiled, customBandsEnabled]);

  useEffect(() => {
    setAutoBandGapK("");
  }, [strategyId]);

  const findHistoricalManagerIds = useCallback(
    async (address: string, signal: AbortSignal): Promise<string[]> => {
      const ids = new Set<string>();
      let cursor: string | null | undefined = null;

      for (let pageIndex = 0; pageIndex < 4; pageIndex += 1) {
        const page = await suiClient.queryTransactionBlocks({
          filter: { FromAddress: address },
          options: { showObjectChanges: true },
          limit: 50,
          cursor,
          order: "descending",
          signal,
        });

        if (signal.aborted) return [];

        for (const tx of page.data) {
          for (const change of tx.objectChanges ?? []) {
            if (
              change.type === "created" &&
              typeof change.objectType === "string" &&
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
    },
    [suiClient],
  );

  // Bind owner to the connected wallet, clear all per-account inputs/results
  // when the connected address changes (so a new wallet never inherits the
  // previous user's PredictManager balance / compile cache / audit), then
  // auto-discover an existing PredictManager for the connected address. We
  // collect candidates from localStorage, the backend store, and this wallet's
  // historical create_manager transactions, then choose the candidate with the
  // highest live manager balance. This prevents a stale cached empty manager
  // from masking an older funded manager.
  //
  // If no valid manager can be recovered anywhere, auto-prompt the wallet to
  // sign predict::create_manager.
  //
  // Important: this is *non-custodial*. StructX never holds keys. The "auto"
  // here only means StructX initiates the transaction immediately; the user's
  // wallet still pops a sign prompt that they must approve.
  useEffect(() => {
    // Wallet just changed: cancel any compile / open the previous wallet
    // started, so their results don't slip into the new wallet's state.
    compileAbortRef.current?.abort();
    openAbortRef.current?.abort();
    setOwner(connectedAddress ?? "");
    setManagerId("");
    setManagerBalanceRaw(null);
    setManagerBalanceDisplay(null);
    setManagerBalanceError(null);
    setCompiled(null);
    setCompileError(null);
    setOpenError(null);
    setAudit(null);
    setDiscover({ phase: "idle" });

    if (!connectedAddress) return;

    const controller = new AbortController();
    const network = ctx.network ?? "testnet";
    const cacheKey = managerStorageKey(connectedAddress, network);
    const sourceRank = (source: ManagerCandidateSource) =>
      source === "cache" ? 0 : source === "backend" ? 1 : 2;

    const writeLocal = (id: string) => {
      try {
        window.localStorage.setItem(cacheKey, id);
      } catch {
        // localStorage may be disabled (private mode); ignore.
      }
    };

    const finish = (id: string, source: "cache" | "backend" | "history") => {
      if (controller.signal.aborted) return;
      writeLocal(id);
      setManagerId(id);
      setDiscover({ phase: "found", source });
    };

    (async () => {
      const candidateSources = new Map<string, ManagerCandidateSource>();
      const addCandidate = (id: string | null | undefined, source: ManagerCandidateSource) => {
        if (!id || !id.startsWith("0x")) return;
        if (!candidateSources.has(id)) candidateSources.set(id, source);
      };

      // Tier 1 — localStorage. Synchronous, no network, instant, but no
      // longer trusted blindly.
      try {
        const cached =
          typeof window !== "undefined"
            ? window.localStorage.getItem(cacheKey)
            : null;
        addCandidate(cached, "cache");
        if (cached) {
          finish(cached, "cache");
        } else {
          setDiscover({ phase: "checking" });
        }
      } catch {
        // ignore — fall through.
        setDiscover({ phase: "checking" });
      }

      // Tier 2 + 3 in parallel. Backend JSON store and historical wallet tx
      // scanning are independent; we merge all discovered candidate ids and
      // then choose the one with the highest live balance.
      const backendP = getStoredManager(connectedAddress, {
        signal: controller.signal,
      }).catch((err) => {
        if (isAbortError(err)) throw err;
        return null;
      });

      const historyP = (async () => {
        try {
          return await findHistoricalManagerIds(
            connectedAddress,
            controller.signal,
          );
        } catch {
          return [];
        }
      })();

      let backendId: string | null = null;
      try {
        backendId = await backendP;
      } catch (err) {
        if (isAbortError(err)) return;
        backendId = null;
      }
      addCandidate(backendId, "backend");
      if (backendId && candidateSources.size === 1) {
        finish(backendId, "backend");
      }

      const historyIds = await historyP;
      if (controller.signal.aborted) return;
      historyIds.forEach((id) => addCandidate(id, "history"));

      const evaluated = (
        await Promise.all(
          Array.from(candidateSources, async ([id, source]) => {
            invalidateManagerBalance(id);
            try {
              const balance = await getManagerBalance(id, {
                signal: controller.signal,
              });
              if (controller.signal.aborted) return null;
              const balanceRaw =
                balance.ok && balance.balanceRaw ? BigInt(balance.balanceRaw) : null;
              return balanceRaw !== null ? { id, source, balanceRaw } : null;
            } catch (err) {
              if (isAbortError(err)) throw err;
              return null;
            }
          }),
        )
      ).filter((candidate): candidate is ManagerCandidate => candidate !== null);

      if (evaluated.length > 0) {
        evaluated.sort((left, right) => {
          if (left.balanceRaw === right.balanceRaw) {
            return sourceRank(left.source) - sourceRank(right.source);
          }
          return left.balanceRaw > right.balanceRaw ? -1 : 1;
        });

        const best = evaluated[0];
        void saveStoredManager(connectedAddress, best.id);
        finish(best.id, best.source);
        return;
      }

      if (candidateSources.size > 0) {
        const [fallbackId, fallbackSource] = Array.from(candidateSources.entries()).sort(
          ([, leftSource], [, rightSource]) => sourceRank(leftSource) - sourceRank(rightSource),
        )[0];
        finish(fallbackId, fallbackSource);
        return;
      }

      // No existing manager anywhere → auto-create. Testnet-only guard.
      if (network !== "testnet") {
        setDiscover({
          phase: "error",
          message:
            "Switch your wallet to Sui Testnet to auto-create a PredictManager.",
        });
        return;
      }

      try {
        setDiscover({ phase: "creating" });
        const tx = buildCreateManagerTransaction(connectedAddress);
        const execution = await signAndExecuteTransaction({
          transaction: tx,
          chain: "sui:testnet",
        });
        if (controller.signal.aborted) return;
        const confirmed = await suiClient.waitForTransaction({
          digest: execution.digest,
          options: { showObjectChanges: true, showEffects: true },
        });
        if (controller.signal.aborted) return;
        if (confirmed.effects?.status?.status !== "success") {
          setDiscover({
            phase: "error",
            message:
              confirmed.effects?.status?.error ??
              "Manager creation transaction failed.",
          });
          return;
        }
        const changes = confirmed.objectChanges ?? [];
        const created = changes.find(
          (c) =>
            c.type === "created" &&
            typeof (c as { objectType?: string }).objectType === "string" &&
            (c as { objectType: string }).objectType.includes(
              "predict_manager::PredictManager",
            ),
        ) as { objectId?: string } | undefined;
        const newId = created?.objectId;
        if (!newId) {
          setDiscover({
            phase: "error",
            message:
              "Transaction succeeded but the new PredictManager id wasn't found in object changes.",
          });
          return;
        }
        writeLocal(newId);
        // Persist to the backend JSON store so any future connect from any
        // browser / device hits the cache path instead of re-creating.
        void saveStoredManager(connectedAddress, newId);
        setManagerId(newId);
        setDiscover({ phase: "created" });
      } catch (err) {
        if (controller.signal.aborted) return;
        if (isAbortError(err)) return;
        const message =
          err instanceof Error
            ? err.message
            : "Couldn't create a PredictManager from your wallet.";
        // Wallet rejection is the common case here — keep tone friendly.
        const friendly = /reject|denied|cancel/i.test(message)
          ? "You declined the signature. Reconnect or retry to create a PredictManager."
          : message;
        setDiscover({ phase: "error", message: friendly });
      }
    })();

    return () => {
      controller.abort();
    };
  }, [
    connectedAddress,
    ctx.network,
    suiClient,
    signAndExecuteTransaction,
    discoverNonce,
  ]);

  const loadBalance = useCallback(
    async (id: string, opts: { fresh?: boolean; signal?: AbortSignal } = {}) => {
      if (!id) {
        setManagerBalanceRaw(null);
        setManagerBalanceDisplay(null);
        return;
      }
      // After a mint/redeem we must skip the cached value — `fresh` busts it.
      if (opts.fresh) invalidateManagerBalance(id);
      setManagerBalanceLoading(true);
      setManagerBalanceError(null);
      try {
        const r = await getManagerBalance(id, { signal: opts.signal });
        if (opts.signal?.aborted) return;
        if (r.ok) {
          setManagerBalanceRaw(r.balanceRaw ?? null);
          setManagerBalanceDisplay(r.balanceDisplay ?? null);
        } else {
          setManagerBalanceError(r.error ?? "Manager balance unavailable");
        }
      } catch (err) {
        if (isAbortError(err)) return;
        const friendly = err instanceof ApiError ? err.body.message : (err as Error).message;
        setManagerBalanceError(friendly ?? "Manager balance unavailable");
      } finally {
        if (!opts.signal?.aborted) setManagerBalanceLoading(false);
      }
    },
    [],
  );

  // Manager balance is now cache+dedup'd in lib/api, so we can fire
  // immediately on managerId change — no debounce needed. AbortController
  // cancels the in-flight balance request if the manager id changes again
  // before this one resolves.
  useEffect(() => {
    if (!managerId) return;
    const controller = new AbortController();
    void loadBalance(managerId, { signal: controller.signal });
    return () => controller.abort();
  }, [managerId, loadBalance]);

  // Each compile/open gets its own AbortController held in a ref. Starting a
  // new compile cancels the previous one mid-flight; unmount / wallet switch
  // also cancels. This keeps the user's perceived latency tied to their
  // *latest* click, not whatever stale request is still resolving.
  const compileAbortRef = useRef<AbortController | null>(null);
  const openAbortRef = useRef<AbortController | null>(null);
  const bandRecompileTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    return () => {
      if (bandRecompileTimeoutRef.current) {
        clearTimeout(bandRecompileTimeoutRef.current);
      }
      compileAbortRef.current?.abort();
      openAbortRef.current?.abort();
    };
  }, []);

  const onCompile = useCallback(async () => {
    if (bandRecompileTimeoutRef.current) {
      clearTimeout(bandRecompileTimeoutRef.current);
      bandRecompileTimeoutRef.current = null;
    }
    compileAbortRef.current?.abort();
    const controller = new AbortController();
    compileAbortRef.current = controller;

    setCompileLoading(true);
    setCompileError(null);
    setCompiled(null);
    setOpenError(null);
    setAudit(null);
    try {
      const json = await compileStrategy(
        {
          owner,
          strategy: strategyId,
          budgetDUSDC: budget,
          style,
          expiryPreference: "nearest_active",
          slippageBps: Number(slippage),
          bucketStepUsd:
            !customBandsEnabled && autoBandGapK.trim() !== ""
              ? Number(autoBandGapK) * 1000
              : undefined,
          customK1Price:
            customBandsEnabled && hasCompleteCustomBands
              ? Number(customK1K) * 1000
              : undefined,
          customK2Price:
            customBandsEnabled && hasCompleteCustomBands
              ? Number(customK2K) * 1000
              : undefined,
          customK3Price:
            customBandsEnabled && hasCompleteCustomBands
              ? Number(customK3K) * 1000
              : undefined,
          customK4Price:
            customBandsEnabled && hasCompleteCustomBands
              ? Number(customK4K) * 1000
              : undefined,
          portfolioExposureDUSDC:
            strategyId === "PORTFOLIO_CRASH_SHIELD" ||
            strategyId === "SMART_BUDGET_SELECTOR"
              ? Number(portfolioExposureDUSDC)
              : undefined,
          overHedgeCapBps:
            strategyId === "PORTFOLIO_CRASH_SHIELD" ||
            strategyId === "SMART_BUDGET_SELECTOR"
              ? Number(overHedgeCapBps)
              : undefined,
          deadZoneBps:
            strategyId === "CONVEX_TAIL_LADDER" ||
            strategyId === "SMART_BUDGET_SELECTOR"
              ? Number(deadZoneBps)
              : undefined,
          convexGammaBps:
            strategyId === "CONVEX_TAIL_LADDER" ||
            strategyId === "SMART_BUDGET_SELECTOR"
              ? Number(convexGammaBps)
              : undefined,
          moonshotRangeWeightBps:
            strategyId === "MOONSHOT_UPSIDE" ||
            strategyId === "SMART_BUDGET_SELECTOR"
              ? Number(moonshotRangeWeightBps)
              : undefined,
          moonshotTailGammaBps:
            strategyId === "MOONSHOT_UPSIDE" ||
            strategyId === "SMART_BUDGET_SELECTOR"
              ? Number(moonshotTailGammaBps)
              : undefined,
          upsideNearRangeWeightBps:
            strategyId === "UPSIDE_STEP_LADDER" ? Number(upsideNearRangeWeightBps) : undefined,
          upsideUpperRangeWeightBps:
            strategyId === "UPSIDE_STEP_LADDER" ? Number(upsideUpperRangeWeightBps) : undefined,
          upsideTailGammaBps:
            strategyId === "UPSIDE_STEP_LADDER" ? Number(upsideTailGammaBps) : undefined,
          downsideNearRangeWeightBps:
            strategyId === "DOWNSIDE_STEP_LADDER"
              ? Number(downsideNearRangeWeightBps)
              : undefined,
          downsideLowerRangeWeightBps:
            strategyId === "DOWNSIDE_STEP_LADDER"
              ? Number(downsideLowerRangeWeightBps)
              : undefined,
          downsideStepTailGammaBps:
            strategyId === "DOWNSIDE_STEP_LADDER"
              ? Number(downsideStepTailGammaBps)
              : undefined,
          condorCenterWeightBps:
            strategyId === "CENTER_BAND_CONDOR" ? Number(condorCenterWeightBps) : undefined,
          barrierSide: strategyId === "NEAR_BARRIER_PROXY" ? barrierSide : undefined,
          barrierNearRangeWeightBps:
            strategyId === "NEAR_BARRIER_PROXY"
              ? Number(barrierNearRangeWeightBps)
              : undefined,
          barrierTailGammaBps:
            strategyId === "NEAR_BARRIER_PROXY" ? Number(barrierTailGammaBps) : undefined,
        },
        { signal: controller.signal },
      );
      if (controller.signal.aborted) return;
      setCompiled(json);
    } catch (err) {
      if (isAbortError(err) || controller.signal.aborted) return;
      setCompileError(mapError(err));
    } finally {
      if (!controller.signal.aborted) setCompileLoading(false);
    }
  }, [
    owner,
    strategyId,
    budget,
    style,
    slippage,
    autoBandGapK,
    customBandsEnabled,
    hasCompleteCustomBands,
    customK1K,
    customK2K,
    customK3K,
    customK4K,
    portfolioExposureDUSDC,
    overHedgeCapBps,
    deadZoneBps,
    convexGammaBps,
    moonshotRangeWeightBps,
    moonshotTailGammaBps,
    upsideNearRangeWeightBps,
    upsideUpperRangeWeightBps,
    upsideTailGammaBps,
    downsideNearRangeWeightBps,
    downsideLowerRangeWeightBps,
    downsideStepTailGammaBps,
    condorCenterWeightBps,
    barrierSide,
    barrierNearRangeWeightBps,
    barrierTailGammaBps,
  ]);

  const scheduleBandRecompile = useCallback(() => {
    if (!compiled) return;
    if (bandRecompileTimeoutRef.current) {
      clearTimeout(bandRecompileTimeoutRef.current);
    }
    bandRecompileTimeoutRef.current = setTimeout(() => {
      void onCompile();
    }, 250);
  }, [compiled, onCompile]);

  // Premium is "ok" if (manager balance) + (wallet dUSDC) >= premium. We
  // accept either source because the open path will auto-deposit any
  // shortfall from the wallet in the same PTB as the mint calls.
  const fundsSnapshot = useMemo(() => {
    if (!compiled) return null;
    try {
      const premium = BigInt(compiled.premiumRequiredRaw);
      const reserve = addSlippageBps(premium, Number(slippage));
      const managerBalance = managerBalanceRaw ? BigInt(managerBalanceRaw) : 0n;
      const walletBalance = walletDusdcRaw ? BigInt(walletDusdcRaw) : 0n;
      const totalAvailable = managerBalance + walletBalance;
      const shortfall = reserve > managerBalance ? reserve - managerBalance : 0n;
      return {
        premium,
        reserve,
        managerBalance,
        walletBalance,
        totalAvailable,
        shortfall,
        sufficient: totalAvailable >= reserve,
      };
    } catch {
      return null;
    }
  }, [compiled, managerBalanceRaw, slippage, walletDusdcRaw]);

  const premiumOk = fundsSnapshot?.sufficient ?? false;

  const budgetOk = useMemo(() => {
    if (!compiled) return false;
    try {
      const budgetRaw = BigInt(compiled.budgetRaw);
      const premiumRaw = BigInt(compiled.premiumRequiredRaw);
      return premiumRaw <= budgetRaw + budgetToleranceRaw(budgetRaw);
    } catch {
      return false;
    }
  }, [compiled]);

  const canOpen =
    Boolean(compiled) &&
    Boolean(connectedAddress) &&
    isTestnet &&
    Boolean(managerId) &&
    premiumOk;

  // True when we'll auto-deposit during sign — used to change the button
  // label from "Sign & open" → "Deposit & open" so the user knows they're
  // authorizing both moves with one signature.
  const willDeposit = Boolean(
    fundsSnapshot && fundsSnapshot.shortfall > 0n && fundsSnapshot.sufficient,
  );

  const onOpen = useCallback(async () => {
    if (!compiled || !connectedAddress) return;
    openAbortRef.current?.abort();
    const controller = new AbortController();
    openAbortRef.current = controller;

    setOpening(true);
    setOpenError(null);
    setAudit(null);
    try {
      const build = await buildOpenStrategy(
        {
          owner: connectedAddress,
          managerId,
          compiledStrategyId: compiled.compiledStrategyId,
          maxPremiumRaw: compiled.premiumRequiredRaw,
          slippageBps: Number(slippage),
        },
        { signal: controller.signal },
      );
      if (controller.signal.aborted) return;

      // Refresh the manager balance at the last possible moment. A cached
      // balance can overestimate what is available and underfund the atomic
      // deposit-and-mint transaction.
      invalidateManagerBalance(managerId);
      const liveBalance = await getManagerBalance(managerId, {
        signal: controller.signal,
      });
      if (controller.signal.aborted) return;
      if (!liveBalance.ok || !liveBalance.balanceRaw) {
        throw new Error(
          liveBalance.error ?? "Could not read the latest PredictManager balance.",
        );
      }
      setManagerBalanceRaw(liveBalance.balanceRaw);
      setManagerBalanceDisplay(liveBalance.balanceDisplay ?? null);

      const managerBalance = BigInt(liveBalance.balanceRaw);
      const requiredReserve = requiredManagerReserveRaw(build);
      const shortfall =
        requiredReserve > managerBalance ? requiredReserve - managerBalance : 0n;

      let walletDusdcCoinIds: string[] = [];
      if (shortfall > 0n) {
        const { totalRaw, coinObjectIds } = await fetchWalletDusdcBalance(
          suiClient,
          connectedAddress,
          shortfall,
        );
        if (controller.signal.aborted) return;
        if (totalRaw < shortfall) {
          setOpenError({
            title: "Not enough dUSDC",
            message: `This strategy needs ${formatDusdcDisplay(shortfall.toString())} from your wallet, including the price-movement allowance. Your wallet currently has ${formatDusdcDisplay(totalRaw.toString())}.`,
            action: "Add test dUSDC to this wallet or lower the amount, then try again.",
            severity: "blocking",
          });
          return;
        }
        walletDusdcCoinIds = coinObjectIds;
      }

      const transactionArgs = {
        payload: build,
        depositRaw: shortfall,
        walletDusdcCoinIds,
      };
      const preflightTx = buildDepositAndOpenStrategyTransaction(transactionArgs);
      const preflight = await suiClient.devInspectTransactionBlock({
        sender: connectedAddress,
        transactionBlock: preflightTx,
      });
      if (controller.signal.aborted) return;
      if (preflight.effects?.status?.status !== "success") {
        setOpenError(
          mapDryRunFailure(
            preflight.effects?.status?.error ??
              "The final transaction check did not succeed.",
          ),
        );
        return;
      }

      const tx = buildDepositAndOpenStrategyTransaction(transactionArgs);
      const execution = await signAndExecuteTransaction({
        transaction: tx,
        chain: "sui:testnet",
      });
      if (controller.signal.aborted) return;
      const confirmed = await suiClient.waitForTransaction({
        digest: execution.digest,
        options: {
          showEffects: true,
          showEvents: true,
          showObjectChanges: true,
        },
      });
      if (controller.signal.aborted) return;
      if (confirmed.effects?.status?.status !== "success") {
        setOpenError({
          title: "Transaction failed",
          message:
            confirmed.effects?.status?.error ??
            "Transaction failed without detailed status.",
          action: "Preview the strategy again and retry.",
          severity: "blocking",
        });
        return;
      }
      void (async () => {
        try {
          const auditJson = await auditOpenStrategy(
            {
              owner: connectedAddress,
              managerId,
              compiledStrategyId: build.compiledStrategyId,
              digest: execution.digest,
              effects: confirmed.effects ?? {},
              events: confirmed.events ?? [],
              objectChanges: confirmed.objectChanges ?? [],
            },
            { signal: controller.signal },
          );
          if (controller.signal.aborted) return;
          setAudit(auditJson);
        } catch (err) {
          if (isAbortError(err) || controller.signal.aborted) return;
          setOpenError({
            title: "Opened, but audit is still syncing",
            message:
              err instanceof Error
                ? err.message
                : "The trade opened, but the post-trade audit has not finished yet.",
            action: "Refresh the page in a few seconds to see the final audit.",
            severity: "caution",
          });
        } finally {
          if (!controller.signal.aborted) {
            await loadBalance(managerId, {
              fresh: true,
              signal: controller.signal,
            });
            // Mint pulled dUSDC out of the wallet (via the deposit step), so
            // refresh the wallet-side balance too — otherwise the next open
            // still sees the pre-deposit total.
            try {
              const { totalRaw } = await fetchWalletDusdcBalance(
                suiClient,
                connectedAddress,
              );
              if (!controller.signal.aborted) {
                setWalletDusdcRaw(totalRaw.toString());
              }
            } catch {
              // best-effort
            }
          }
        }
      })();
    } catch (err) {
      if (isAbortError(err) || controller.signal.aborted) return;
      setOpenError(mapError(err));
    } finally {
      if (!controller.signal.aborted) setOpening(false);
    }
  }, [
    compiled,
    connectedAddress,
    managerId,
    slippage,
    signAndExecuteTransaction,
    suiClient,
    loadBalance,
  ]);

  const compileDisabled =
    compileLoading ||
    !owner ||
    !budget ||
    Number(budget) <= 0 ||
    !Number.isInteger(Number(slippage)) ||
    Number(slippage) < 0 ||
    Number(slippage) > 10_000;

  const disabledReason = useMemo(() => {
    if (!compiled) return "Preview the payoff first.";
    if (!connectedAddress) return "Connect a Sui wallet.";
    if (!isTestnet) return "Switch your wallet to Sui Testnet.";
    if (discover.phase === "checking") {
      return "Still looking up your PredictManager.";
    }
    if (discover.phase === "creating") {
      return "Approve the PredictManager creation in your wallet.";
    }
    if (discover.phase === "error") return discover.message;
    if (!managerId) return "No PredictManager found for this wallet yet.";
    if (managerBalanceLoading) return "Checking manager balance…";
    if (managerBalanceError) return managerBalanceError;
    if (!premiumOk && fundsSnapshot) {
      const need = formatDusdcDisplay(fundsSnapshot.reserve.toString());
      const have = formatDusdcDisplay(fundsSnapshot.totalAvailable.toString());
      return `This strategy needs ${need} across your wallet and manager, including the price-movement allowance. You currently have ${have}.`;
    }
    return null;
  }, [
    compiled,
    connectedAddress,
    discover,
    fundsSnapshot,
    isTestnet,
    managerBalanceError,
    managerBalanceLoading,
    managerId,
    premiumOk,
  ]);

  return (
    <div className="workbench">
      <aside className="workbench-form" aria-label="Strategy inputs">
        <header>
          <h3>Build {displayName}</h3>
        </header>

        <div className="wb-wallet">
          {connectedAddress ? (
            <div className="wb-wallet-row">
              <span>Wallet</span>
              <strong>{shortAddress(connectedAddress)}</strong>
            </div>
          ) : (
            <div className="wb-wallet-row">
              <span>Wallet</span>
              <strong style={{ color: "var(--sx-navy-muted)", fontFamily: "inherit" }}>
                Connect from header
              </strong>
            </div>
          )}
          <div className="wb-wallet-row">
            <span>Network</span>
            <strong className={isTestnet ? "pos" : "net-bad"}>
              {isTestnet ? "Testnet" : (ctx.network ?? "unknown")}
            </strong>
          </div>
          <div className="wb-wallet-row">
            <span>Balance</span>
            <strong>
              {discover.phase === "creating"
                ? "Creating manager…"
                : !managerId
                  ? discover.phase === "checking"
                    ? "Checking…"
                    : "Unavailable"
                  : managerBalanceLoading
                    ? "Checking…"
                    : (managerBalanceDisplay ?? "Unavailable")}
            </strong>
          </div>
        </div>

        <ManagerStatus
          phase={discover}
          managerId={managerId}
          connected={Boolean(connectedAddress)}
          onRetry={() => setDiscoverNonce((n) => n + 1)}
        />

        <div className="wb-field">
          <label className="wb-label" htmlFor="wb-budget">
            Amount
          </label>
          <div className="wb-input-wrap">
            <input
              id="wb-budget"
              className="wb-input with-suffix"
              value={budget}
              onChange={(e) => setBudget(e.target.value)}
              inputMode="decimal"
              placeholder="50"
            />
            <span className="wb-input-suffix">dUSDC</span>
          </div>
        </div>

        <button
          type="button"
          className="wb-advanced-toggle"
          aria-expanded={advancedOpen}
          onClick={() => setAdvancedOpen((v) => !v)}
        >
          {advancedOpen ? "Hide" : "Show"} advanced settings
        </button>

        {advancedOpen && (
          <div className="wb-advanced-panel">
            <div className="wb-field">
              <div className="wb-style-head">
                <span className="wb-label">Customization</span>
                <button
                  type="button"
                  className={`wb-style-toggle ${customBandsEnabled ? "on" : ""}`}
                  onClick={() => setCustomBandsEnabled((value) => !value)}
                  aria-pressed={customBandsEnabled}
                >
                  {customBandsEnabled ? "Use auto bands" : "Customize strategy bands"}
                </button>
              </div>
              <p className="wb-style-default">
                {customBandsEnabled
                  ? "Set the strategy bands directly in K of BTC price. These values are snapped to the nearest valid DeepBook Predict strikes before quoting."
                  : "Using auto-generated strategy bands from the live BTC price. Turn on customization to set the band levels yourself."}
              </p>
            </div>

            {!customBandsEnabled && activeAutoGapK !== null && (
              <>
                <div className="wb-sliders" role="group" aria-label="Auto strike spacing">
                  <div className="wb-slider-row">
                    <div className="wb-slider-head">
                      <span>Strike gap regulator</span>
                      <strong>{autoBandGapK} K</strong>
                    </div>
                    <input
                      type="range"
                      min={autoGapBounds.min}
                      max={autoGapBounds.max}
                      step={autoGapBounds.step}
                      value={Number(autoBandGapK)}
                      onChange={(e) => {
                        setAutoBandGapK(Number(e.target.value).toFixed(2));
                        scheduleBandRecompile();
                      }}
                      aria-label="Strike gap regulator"
                      className="wb-slider"
                    />
                    <div className="wb-bias-labels">
                      <span>Tighter bands</span>
                      <span>Wider bands</span>
                    </div>
                  </div>
                  <p className="wb-style-default">
                    Change the spacing between the live strategy strikes. The next preview will
                    requote using this gap.
                  </p>
                </div>

                {autoPreviewBandInputs && (
                  <BandValueFields bands={autoPreviewBandInputs} labels={bandLabels} readOnly />
                )}
              </>
            )}

            {customBandsEnabled && (
              <>
                {activeCustomGapK !== null && (
                  <div className="wb-sliders" role="group" aria-label="Custom strike spacing">
                    <div className="wb-slider-row">
                      <div className="wb-slider-head">
                        <span>Strike gap regulator</span>
                        <strong>{activeCustomGapK.toFixed(2)} K</strong>
                      </div>
                      <input
                        type="range"
                        min={customGapBounds.min}
                        max={customGapBounds.max}
                        step={customGapBounds.step}
                        value={activeCustomGapK}
                        onChange={(e) => {
                          if (!customBandValues) return;
                          writeCustomBands(
                            rescaleBandSpacing(customBandValues, Number(e.target.value)),
                          );
                          scheduleBandRecompile();
                        }}
                        aria-label="Custom strike gap regulator"
                        className="wb-slider"
                      />
                      <div className="wb-bias-labels">
                        <span>Tighter bands</span>
                        <span>Wider bands</span>
                      </div>
                    </div>
                    <p className="wb-style-default">
                      Regulate the gap between the custom strikes while keeping their relative
                      shape intact.
                    </p>
                  </div>
                )}

                <BandValueFields
                  bands={{
                    k1: customK1K,
                    k2: customK2K,
                    k3: customK3K,
                    k4: customK4K,
                  }}
                  labels={bandLabels}
                  onChange={{
                    k1: (value) => {
                      setCustomK1K(value);
                      scheduleBandRecompile();
                    },
                    k2: (value) => {
                      setCustomK2K(value);
                      scheduleBandRecompile();
                    },
                    k3: (value) => {
                      setCustomK3K(value);
                      scheduleBandRecompile();
                    },
                    k4: (value) => {
                      setCustomK4K(value);
                      scheduleBandRecompile();
                    },
                  }}
                />
              </>
            )}

            {(strategyId === "PORTFOLIO_CRASH_SHIELD" ||
              strategyId === "SMART_BUDGET_SELECTOR") && (
              <div className="wb-grid-2">
                <div className="wb-field">
                  <label className="wb-label" htmlFor="wb-exposure">
                    Portfolio exposure
                  </label>
                  <div className="wb-input-wrap">
                    <input
                      id="wb-exposure"
                      className="wb-input with-suffix"
                      value={portfolioExposureDUSDC}
                      onChange={(e) => setPortfolioExposureDUSDC(e.target.value)}
                      inputMode="decimal"
                    />
                    <span className="wb-input-suffix">dUSDC</span>
                  </div>
                </div>
                <div className="wb-field">
                  <label className="wb-label" htmlFor="wb-overhedge">
                    Max over-hedge
                  </label>
                  <div className="wb-input-wrap">
                    <input
                      id="wb-overhedge"
                      className="wb-input with-suffix"
                      value={overHedgeCapBps}
                      onChange={(e) => setOverHedgeCapBps(e.target.value)}
                      inputMode="numeric"
                    />
                    <span className="wb-input-suffix">bps</span>
                  </div>
                </div>
              </div>
            )}

            {(strategyId === "CONVEX_TAIL_LADDER" ||
              strategyId === "SMART_BUDGET_SELECTOR") && (
              <div className="wb-grid-2">
                <div className="wb-field">
                  <label className="wb-label" htmlFor="wb-deadzone">
                    Dead zone
                  </label>
                  <div className="wb-input-wrap">
                    <input
                      id="wb-deadzone"
                      className="wb-input with-suffix"
                      value={deadZoneBps}
                      onChange={(e) => setDeadZoneBps(e.target.value)}
                      inputMode="numeric"
                    />
                    <span className="wb-input-suffix">bps</span>
                  </div>
                </div>
                <div className="wb-field">
                  <label className="wb-label" htmlFor="wb-convex-gamma">
                    Convexity gamma
                  </label>
                  <div className="wb-input-wrap">
                    <input
                      id="wb-convex-gamma"
                      className="wb-input with-suffix"
                      value={convexGammaBps}
                      onChange={(e) => setConvexGammaBps(e.target.value)}
                      inputMode="numeric"
                    />
                    <span className="wb-input-suffix">bps</span>
                  </div>
                </div>
              </div>
            )}

            {(strategyId === "MOONSHOT_UPSIDE" ||
              strategyId === "SMART_BUDGET_SELECTOR") && (
              <div className="wb-grid-2">
                <div className="wb-field">
                  <label className="wb-label" htmlFor="wb-moonshot-range">
                    Range allocation
                  </label>
                  <div className="wb-input-wrap">
                    <input
                      id="wb-moonshot-range"
                      className="wb-input with-suffix"
                      value={moonshotRangeWeightBps}
                      onChange={(e) => setMoonshotRangeWeightBps(e.target.value)}
                      inputMode="numeric"
                    />
                    <span className="wb-input-suffix">bps</span>
                  </div>
                </div>
                <div className="wb-field">
                  <label className="wb-label" htmlFor="wb-moonshot-gamma">
                    Tail gamma
                  </label>
                  <div className="wb-input-wrap">
                    <input
                      id="wb-moonshot-gamma"
                      className="wb-input with-suffix"
                      value={moonshotTailGammaBps}
                      onChange={(e) => setMoonshotTailGammaBps(e.target.value)}
                      inputMode="numeric"
                    />
                    <span className="wb-input-suffix">bps</span>
                  </div>
                </div>
              </div>
            )}

            {strategyId === "UPSIDE_STEP_LADDER" && (
              <div className="wb-grid-3">
                <div className="wb-field">
                  <label className="wb-label" htmlFor="wb-upside-near">
                    Near upside
                  </label>
                  <div className="wb-input-wrap">
                    <input
                      id="wb-upside-near"
                      className="wb-input with-suffix"
                      value={upsideNearRangeWeightBps}
                      onChange={(e) => setUpsideNearRangeWeightBps(e.target.value)}
                      inputMode="numeric"
                    />
                    <span className="wb-input-suffix">bps</span>
                  </div>
                </div>
                <div className="wb-field">
                  <label className="wb-label" htmlFor="wb-upside-upper">
                    Upper range
                  </label>
                  <div className="wb-input-wrap">
                    <input
                      id="wb-upside-upper"
                      className="wb-input with-suffix"
                      value={upsideUpperRangeWeightBps}
                      onChange={(e) => setUpsideUpperRangeWeightBps(e.target.value)}
                      inputMode="numeric"
                    />
                    <span className="wb-input-suffix">bps</span>
                  </div>
                </div>
                <div className="wb-field">
                  <label className="wb-label" htmlFor="wb-upside-gamma">
                    Tail gamma
                  </label>
                  <div className="wb-input-wrap">
                    <input
                      id="wb-upside-gamma"
                      className="wb-input with-suffix"
                      value={upsideTailGammaBps}
                      onChange={(e) => setUpsideTailGammaBps(e.target.value)}
                      inputMode="numeric"
                    />
                    <span className="wb-input-suffix">bps</span>
                  </div>
                </div>
              </div>
            )}

            {strategyId === "DOWNSIDE_STEP_LADDER" && (
              <div className="wb-grid-3">
                <div className="wb-field">
                  <label className="wb-label" htmlFor="wb-downside-near">
                    Near downside
                  </label>
                  <div className="wb-input-wrap">
                    <input
                      id="wb-downside-near"
                      className="wb-input with-suffix"
                      value={downsideNearRangeWeightBps}
                      onChange={(e) => setDownsideNearRangeWeightBps(e.target.value)}
                      inputMode="numeric"
                    />
                    <span className="wb-input-suffix">bps</span>
                  </div>
                </div>
                <div className="wb-field">
                  <label className="wb-label" htmlFor="wb-downside-lower">
                    Lower range
                  </label>
                  <div className="wb-input-wrap">
                    <input
                      id="wb-downside-lower"
                      className="wb-input with-suffix"
                      value={downsideLowerRangeWeightBps}
                      onChange={(e) => setDownsideLowerRangeWeightBps(e.target.value)}
                      inputMode="numeric"
                    />
                    <span className="wb-input-suffix">bps</span>
                  </div>
                </div>
                <div className="wb-field">
                  <label className="wb-label" htmlFor="wb-downside-gamma">
                    Tail gamma
                  </label>
                  <div className="wb-input-wrap">
                    <input
                      id="wb-downside-gamma"
                      className="wb-input with-suffix"
                      value={downsideStepTailGammaBps}
                      onChange={(e) => setDownsideStepTailGammaBps(e.target.value)}
                      inputMode="numeric"
                    />
                    <span className="wb-input-suffix">bps</span>
                  </div>
                </div>
              </div>
            )}

            {strategyId === "CENTER_BAND_CONDOR" && (
              <div className="wb-field">
                <label className="wb-label" htmlFor="wb-condor-center">
                  Center band weight
                </label>
                <div className="wb-input-wrap">
                  <input
                    id="wb-condor-center"
                    className="wb-input with-suffix"
                    value={condorCenterWeightBps}
                    onChange={(e) => setCondorCenterWeightBps(e.target.value)}
                    inputMode="numeric"
                  />
                  <span className="wb-input-suffix">bps</span>
                </div>
              </div>
            )}

            {strategyId === "NEAR_BARRIER_PROXY" && (
              <div className="wb-grid-3">
                <div className="wb-field">
                  <label className="wb-label" htmlFor="wb-barrier-side">
                    Barrier side
                  </label>
                  <div className="wb-input-wrap">
                    <select
                      id="wb-barrier-side"
                      className="wb-input"
                      value={barrierSide}
                      onChange={(e) => setBarrierSide(e.target.value as "up" | "down")}
                    >
                      <option value="up">Up barrier</option>
                      <option value="down">Down barrier</option>
                    </select>
                  </div>
                </div>
                <div className="wb-field">
                  <label className="wb-label" htmlFor="wb-barrier-near">
                    Near-barrier range weight
                  </label>
                  <div className="wb-input-wrap">
                    <input
                      id="wb-barrier-near"
                      className="wb-input with-suffix"
                      value={barrierNearRangeWeightBps}
                      onChange={(e) => setBarrierNearRangeWeightBps(e.target.value)}
                      inputMode="numeric"
                    />
                    <span className="wb-input-suffix">bps</span>
                  </div>
                </div>
                <div className="wb-field">
                  <label className="wb-label" htmlFor="wb-barrier-gamma">
                    Tail convexity gamma
                  </label>
                  <div className="wb-input-wrap">
                    <input
                      id="wb-barrier-gamma"
                      className="wb-input with-suffix"
                      value={barrierTailGammaBps}
                      onChange={(e) => setBarrierTailGammaBps(e.target.value)}
                      inputMode="numeric"
                    />
                    <span className="wb-input-suffix">bps</span>
                  </div>
                </div>
              </div>
            )}

            <div className="wb-field">
              <label className="wb-label" htmlFor="wb-slip">
                Slippage
              </label>
              <div className="wb-input-wrap">
                <input
                  id="wb-slip"
                  className="wb-input with-suffix"
                  value={slippage}
                  onChange={(e) => setSlippage(e.target.value)}
                  inputMode="numeric"
                />
                <span className="wb-input-suffix">bps</span>
              </div>
            </div>
          </div>
        )}

        <div className="wb-divider" />
        <div className="wb-button-row">
          <button
            type="button"
            className="wb-primary"
            onClick={() => void onCompile()}
            disabled={compileDisabled}
          >
            {compileLoading ? "Updating preview…" : "Preview payoff"}
          </button>

          {compiled && (
            <button
              type="button"
              className="wb-primary"
              onClick={() => void onOpen()}
              disabled={!canOpen || opening}
            >
              {opening
                ? willDeposit
                  ? "Preparing deposit…"
                  : "Preparing transaction…"
                : willDeposit
                  ? "Fund manager and open"
                  : "Review and open"}
            </button>
          )}
        </div>

        {managerBalanceError && (
          <p className="wb-help" style={{ color: "var(--sx-danger)" }}>
            {managerBalanceError}
          </p>
        )}
        {disabledReason && compiled && (
          <p className="wb-help">{disabledReason}</p>
        )}
      </aside>

      <section className="workbench-preview">
        {compileError && (
          <FriendlyAlert tone="danger" error={compileError} />
        )}
        {openError && <FriendlyAlert tone="danger" error={openError} />}

        {!compiled && !compileLoading && !compileError && (
          <div className="wb-empty">
            <h3>Preview your payoff</h3>
            <p>
              Choose an amount on the left, then select{" "}
              <strong>Preview payoff</strong> to see the legs, premium, and
              possible outcomes.
            </p>
          </div>
        )}

        {compileLoading && <WorkbenchPreviewSkeleton />}

        {compiled && <PreviewCards compiled={compiled} />}
        {compiled && <PayoffShape compiled={compiled} />}
        {compiled && <LegsBlock compiled={compiled} />}
        {compiled && <PayoffBlock compiled={compiled} />}

        {audit && <AuditCard audit={audit} />}
      </section>
    </div>
  );
}

function ManagerStatus({
  phase,
  managerId,
  connected,
  onRetry,
}: {
  phase: DiscoverPhase;
  managerId: string;
  connected: boolean;
  onRetry: () => void;
}) {
  if (!connected) {
    return (
      <div className="wb-discover">
        <span className="wb-discover-dot dot-idle" aria-hidden />
        <div className="wb-discover-text">
          <strong>PredictManager</strong>
          <span>Connect your wallet so StructX can find or create one.</span>
        </div>
      </div>
    );
  }

  return (
    <div className="wb-field">
      <div
        className={`wb-discover phase-${phase.phase}`}
        role="status"
        aria-live="polite"
      >
        <span
          className={`wb-discover-dot dot-${phase.phase}`}
          aria-hidden
        />
        <div className="wb-discover-text">
          {phase.phase === "checking" && (
            <>
              <strong>Finding your PredictManager…</strong>
              <span>
                Checking your saved manager and recent wallet activity on Sui
                Testnet.
              </span>
            </>
          )}
          {phase.phase === "found" && (
            <>
              <strong>PredictManager found</strong>
              <span>
                {phase.source === "history"
                  ? "Found in your recent wallet transactions."
                  : phase.source === "backend"
                    ? "Found in your StructX history."
                    : "Loaded from this browser session."}
              </span>
            </>
          )}
          {phase.phase === "creating" && (
            <>
              <strong>Creating a PredictManager…</strong>
              <span>Review and approve the manager creation in your wallet.</span>
            </>
          )}
          {phase.phase === "created" && (
            <>
              <strong>PredictManager ready</strong>
              <span>Your new manager is ready for this wallet.</span>
            </>
          )}
          {phase.phase === "error" && (
            <>
              <strong>PredictManager setup needs attention</strong>
              <span>{phase.message}</span>
            </>
          )}
          {phase.phase === "idle" && (
            <>
              <strong>PredictManager</strong>
              <span>Connect your wallet so StructX can find or create one.</span>
            </>
          )}
        </div>
        {phase.phase === "error" && (
          <button type="button" className="wb-discover-retry" onClick={onRetry}>
            Retry
          </button>
        )}
      </div>

    </div>
  );
}

function FriendlyAlert({
  error,
  tone,
}: {
  error: FriendlyError;
  tone: "info" | "warn" | "danger";
}) {
  return (
    <div className={`wb-alert ${tone}`}>
      <strong>{error.title}</strong>
      <span>{error.message}</span>
      {error.action && <small>{error.action}</small>}
    </div>
  );
}

function PreviewCards({ compiled }: { compiled: CompileResponse }) {
  return (
    <div className="wb-card">
      <div className="wb-card-head">
        <h3>Preview</h3>
        <span>live quote</span>
      </div>
      <div className="wb-stats">
        <div className="wb-stat">
          <label>Premium</label>
          <strong>{formatDusdcDisplayString(compiled.premiumRequiredDisplay)}</strong>
        </div>
        <div className="wb-stat neg">
          <label>Max loss</label>
          <strong>{formatDusdcDisplayString(compiled.maxLossDisplay)}</strong>
        </div>
        <div className="wb-stat pos">
          <label>Max gross payout</label>
          <strong>{formatDusdcDisplayString(compiled.maxGrossPayoutDisplay)}</strong>
        </div>
        <div className="wb-stat pos">
          <label>Max net PnL</label>
          <strong>{formatDusdcDisplayString(compiled.maxNetPayoutDisplay)}</strong>
        </div>
      </div>
    </div>
  );
}

function PayoffShape({ compiled }: { compiled: CompileResponse }) {
  const rows = compiled.payoffTable;
  const strikeLabels = useMemo(() => strikeTokenMap(compiled), [compiled]);
  const grosses = rows.map((row) => {
    try {
      return Number(BigInt(row.grossPayoutRaw));
    } catch {
      return 0;
    }
  });
  const max = Math.max(...grosses, 1);
  const labels = rows.map((row) => compactScenarioLabel(row.condition, strikeLabels));
  const count = Math.max(labels.length, 1);
  return (
    <div className="wb-card">
      <div className="wb-card-head">
        <h3>Payoff shape</h3>
        <span>{rows.length} scenarios</span>
      </div>
      <div
        className="wb-bars"
        style={{ gridTemplateColumns: `repeat(${count}, minmax(0, 1fr))` }}
      >
        {grosses.map((g, i) => {
          const h = Math.max(8, Math.round((g / max) * 96));
          const net = bigIntSafe(rows[i]?.netPnlRaw);
          const tone = net !== null && net < 0n ? "loss" : "win";
          return (
            <div key={i} className="wb-bar-col">
              <div className={`wb-bar tone-${tone}`} style={{ height: `${h}%` }} />
              <span className="wb-bar-label mono">{labels[i]}</span>
            </div>
          );
        })}
      </div>
    </div>
  );
}

function LegsBlock({ compiled }: { compiled: CompileResponse }) {
  return (
    <div className="wb-card">
      <div className="wb-card-head">
        <h3>Legs</h3>
        <span>{compiled.legs.length} positions</span>
      </div>
      <table className="wb-table">
        <thead>
          <tr>
            <th>Kind</th>
            <th>Strike / range</th>
            <th>Quantity</th>
            <th>Premium</th>
          </tr>
        </thead>
        <tbody>
          {compiled.legs.map((leg, i) => {
            const kindCls = leg.kind === "DOWN" ? "down" : leg.kind === "UP" ? "up" : "range";
            const strike =
              leg.kind === "RANGE"
                ? `${formatPriceCompact(leg.lower)} → ${formatPriceCompact(leg.upper)}`
                : formatPriceCompact(leg.strike);
            return (
              <tr key={`${leg.kind}-${leg.role}-${i}`}>
                <td>
                  <span className={`wb-kind ${kindCls}`}>{leg.kind}</span>
                </td>
                <td className="mono">{strike}</td>
                <td className="mono">{leg.quantityDisplay}</td>
                <td className="mono">{formatDusdcDisplayString(leg.premiumDisplay)}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function PayoffBlock({ compiled }: { compiled: CompileResponse }) {
  const strikeLabels = useMemo(() => strikeTokenMap(compiled), [compiled]);

  return (
    <div className="wb-card">
      <div className="wb-card-head">
        <h3>Scenario payoff</h3>
        <span>net PnL after premium</span>
      </div>
      <table className="wb-table">
        <thead>
          <tr>
            <th>Scenario</th>
            <th>Gross</th>
            <th>Net PnL</th>
          </tr>
        </thead>
        <tbody>
          {compiled.payoffTable.map((row, i) => {
            const net = bigIntSafe(row.netPnlRaw);
            const cls = net === null ? "" : net > 0n ? "pos" : net < 0n ? "neg" : "";
            return (
              <tr key={row.condition}>
                <td className="mono">{compactScenarioLabel(row.condition, strikeLabels)}</td>
                <td className="mono">{formatDusdcDisplayString(row.grossPayoutDisplay)}</td>
                <td className={`mono ${cls}`}>{formatDusdcDisplayString(row.netPnlDisplay)}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function AuditCard({ audit }: { audit: AuditResponse }) {
  const verification = audit.positionVerification;
  const partial = verification?.status === "partial";
  const tone: "info" | "warn" | "danger" =
    audit.ok ? (partial ? "warn" : "info") : "danger";
  return (
    <div className="wb-card">
      <div className="wb-card-head">
        <h3>Audit</h3>
        <span>
          {audit.ok ? (partial ? "Partial" : "Accepted") : "Failed"}
        </span>
      </div>
      <div className={`wb-alert ${tone}`}>
        <strong>
          {audit.ok
            ? partial
              ? "Audit accepted with caution"
              : "Audit accepted"
            : "Audit failed"}
        </strong>
        <span>Execution status: {audit.executionStatus ?? "unknown"}</span>
        {audit.explorerUrl && (
          <small>
            <a
              href={audit.explorerUrl}
              target="_blank"
              rel="noreferrer noopener"
              style={{ color: "inherit", textDecoration: "underline" }}
            >
              Open in explorer ↗
            </a>
            {"  ·  "}
            <a
              href="/positions"
              style={{ color: "inherit", textDecoration: "underline" }}
            >
              View live position →
            </a>
          </small>
        )}
      </div>
      <div className="wb-stats" style={{ marginTop: 14 }}>
        <div className="wb-stat">
          <label>Total premium</label>
          <strong>{formatDusdcDisplayString(audit.totalCostDisplay) ?? "Unavailable"}</strong>
        </div>
        <div className="wb-stat">
          <label>Manager after</label>
          <strong>
            {formatDusdcDisplayString(audit.managerBalanceDisplay) ?? "Unavailable"}
          </strong>
        </div>
        <div className="wb-stat">
          <label>Verified</label>
          <strong>
            {verification
              ? `${verification.verifiedCount} / ${verification.verifiedCount + verification.mismatchCount}`
              : "Unavailable"}
          </strong>
        </div>
        <div className="wb-stat">
          <label>Digest</label>
          <strong style={{ fontSize: 13 }}>{shortAddress(audit.digest ?? "")}</strong>
        </div>
      </div>
    </div>
  );
}
