"use client";

import {
  useCurrentAccount,
  useSignAndExecuteTransaction,
  useSuiClient,
  useSuiClientContext,
} from "@mysten/dapp-kit";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { AuditReceipt } from "@/components/audit/AuditReceipt";
import { EmptyState } from "@/components/common/EmptyState";
import { SkeletonCard } from "@/components/common/Skeleton";
import { Toast } from "@/components/common/Toast";
import { ExecutionPanel } from "@/components/execution/ExecutionPanel";
import { NormalModeView } from "@/components/guided/NormalModeView";
import { ErrorNotice } from "@/components/ErrorNotice";
import { CategoryNav } from "@/components/layout/CategoryNav";
import { DeepBookPredictAttribution } from "@/components/layout/DeepBookPredictAttribution";
import { Header } from "@/components/layout/Header";
import { ModeToggle } from "@/components/mode/ModeToggle";
import { PortfolioDashboard } from "@/components/portfolio/PortfolioDashboard";
import { LegsTable } from "@/components/preview/LegsTable";
import { PayoffTable } from "@/components/preview/PayoffTable";
import { PayoffVisualization } from "@/components/preview/PayoffVisualization";
import { PreviewSummary } from "@/components/preview/PreviewSummary";
import { SmartSelectorPanel } from "@/components/preview/SmartSelectorPanel";
import { StrategyBuilder } from "@/components/strategy/StrategyBuilder";
import { StrategyGrid } from "@/components/strategy/StrategyGrid";
import { WarningsPanel } from "@/components/WarningsPanel";
import {
  ApiError,
  auditOpenStrategy,
  buildOpenStrategy,
  compileStrategy,
  getManagerBalance,
} from "@/lib/api";
import { mapDryRunFailure, mapError, type FriendlyError } from "@/lib/errors";
import { appendPortfolioHistory, readPortfolioHistory } from "@/lib/portfolioHistory";
import {
  CATEGORY_TABS,
  filterStrategies,
  findCatalogEntryById,
  findCatalogEntryByStrategyId,
  STRATEGY_CATALOG,
  type CategoryTab,
} from "@/lib/strategyCatalog";
import {
  buildCreateManagerTransaction,
  PREDICT_MANAGER_TYPE,
} from "@/lib/tx";
import { buildOpenStrategyTransaction } from "@/lib/tx";
import type {
  AppMode,
  AuditResponse,
  CompileResponse,
  ExecutionStage,
  GuidedCompileResponse,
  ManagerBalanceResponse,
  ParsedIntentSuccess,
  PortfolioTradeRecord,
  StageStatus,
  StrategyStyle,
  WorkspaceView,
} from "@/types/structx";

const LIVE_STRATEGY_ID = "breakout-protection";

type StageMap = Record<ExecutionStage, StageStatus>;
type ManagerNotice = { tone: "info" | "error"; message: string } | null;

const INITIAL_STAGES: StageMap = {
  configure: "active",
  preview: "pending",
  preflight: "pending",
  dryRun: "pending",
  signature: "pending",
  submitted: "pending",
  audited: "pending",
};

function extractPredictManagerIdFromObjectChanges(
  objectChanges: Array<{ type?: string; objectType?: string; objectId?: string }> | null | undefined,
): string | null {
  if (!objectChanges) return null;

  for (const change of objectChanges) {
    if (
      change.type === "created" &&
      change.objectType === PREDICT_MANAGER_TYPE &&
      change.objectId
    ) {
      return change.objectId;
    }
  }

  return null;
}

function budgetToleranceRaw(budgetRaw: bigint): bigint {
  const percent = budgetRaw / 50n; // 2%
  const floor = 50_000n; // 0.05 dUSDC
  return percent > floor ? percent : floor;
}

export default function HomePage() {
  const account = useCurrentAccount();
  const suiClient = useSuiClient();
  const { mutateAsync: signAndExecuteTransaction } =
    useSignAndExecuteTransaction();
  const suiCtx = useSuiClientContext();

  const connectedAddress = account?.address ?? null;
  const isTestnet = suiCtx.network === "testnet";

  const [mode, setMode] = useState<AppMode>("normal");
  const [workspaceView, setWorkspaceView] = useState<WorkspaceView>("strategies");

  // Marketplace state
  const [activeTab, setActiveTab] = useState<CategoryTab>("All");
  const [search, setSearch] = useState("");
  const [activeStrategyId, setActiveStrategyId] = useState<string | null>(null);
  const [portfolioHistory, setPortfolioHistory] = useState<PortfolioTradeRecord[]>([]);

  // Builder state
  const [owner, setOwner] = useState("");
  const [managerId, setManagerId] = useState("");
  const [budgetDUSDC, setBudgetDUSDC] = useState("250");
  const [style, setStyle] = useState<StrategyStyle>("balanced");
  const [slippageBps, setSlippageBps] = useState("100");
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
  const [downsideStepTailGammaBps, setDownsideStepTailGammaBps] = useState("15000");
  const [condorCenterWeightBps, setCondorCenterWeightBps] = useState("8000");
  const [barrierSide, setBarrierSide] = useState<"up" | "down">("up");
  const [barrierNearRangeWeightBps, setBarrierNearRangeWeightBps] = useState("7000");
  const [barrierTailGammaBps, setBarrierTailGammaBps] = useState("15000");

  // Compile state
  const [compiled, setCompiled] = useState<CompileResponse | null>(null);
  const [compileLoading, setCompileLoading] = useState(false);
  const [compileError, setCompileError] = useState<FriendlyError | null>(null);

  // Balance state
  const [managerBalance, setManagerBalance] =
    useState<ManagerBalanceResponse | null>(null);
  const [managerBalanceLoading, setManagerBalanceLoading] = useState(false);
  const [managerDiscovering, setManagerDiscovering] = useState(false);
  const [creatingManager, setCreatingManager] = useState(false);
  const [managerNotice, setManagerNotice] = useState<ManagerNotice>(null);

  // Execution state
  const [dryRunning, setDryRunning] = useState(false);
  const [dryRunOk, setDryRunOk] = useState(false);
  const [dryRunError, setDryRunError] = useState<FriendlyError | null>(null);
  const [opening, setOpening] = useState(false);
  const [openError, setOpenError] = useState<FriendlyError | null>(null);
  const [audit, setAudit] = useState<AuditResponse | null>(null);

  // Stepper + toast
  const [stages, setStages] = useState<StageMap>(INITIAL_STAGES);
  const [toast, setToast] = useState<string | null>(null);
  const toastTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const builderRef = useRef<HTMLDivElement | null>(null);
  const previewRef = useRef<HTMLDivElement | null>(null);

  const flashToast = useCallback((message: string) => {
    setToast(message);
    if (toastTimeoutRef.current) clearTimeout(toastTimeoutRef.current);
    toastTimeoutRef.current = setTimeout(() => setToast(null), 1800);
  }, []);

  const onCopied = useCallback(
    (label: string) => flashToast(`Copied ${label}`),
    [flashToast],
  );

  useEffect(() => {
    return () => {
      if (toastTimeoutRef.current) clearTimeout(toastTimeoutRef.current);
    };
  }, []);

  const findManagerForWallet = useCallback(
    async (address: string): Promise<string | null> => {
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
          const managerId = extractPredictManagerIdFromObjectChanges(
            tx.objectChanges ?? [],
          );
          if (managerId) return managerId;
        }

        if (!page.hasNextPage || !page.nextCursor) break;
        cursor = page.nextCursor;
      }

      return null;
    },
    [suiClient],
  );

  useEffect(() => {
    const savedManagerId = connectedAddress
      ? window.localStorage.getItem(`structx.manager.${connectedAddress}`) ?? ""
      : "";

    setOwner(connectedAddress ?? "");
    setManagerId(savedManagerId);
    setManagerBalance(null);
    setCompiled(null);
    setCompileError(null);
    setDryRunOk(false);
    setDryRunError(null);
    setOpenError(null);
    setAudit(null);
    setStages({ ...INITIAL_STAGES });
    setManagerNotice(null);

    if (!connectedAddress || !isTestnet) {
      return;
    }

    if (savedManagerId) {
      setManagerNotice({
        tone: "info",
        message: "Loaded saved PredictManager for this wallet.",
      });
      return;
    }

    let cancelled = false;
    setManagerDiscovering(true);

    void (async () => {
      try {
        const discoveredManagerId = await findManagerForWallet(connectedAddress);
        if (cancelled) return;

        if (discoveredManagerId) {
          setManagerId(discoveredManagerId);
          setManagerNotice({
            tone: "info",
            message: "Found an existing PredictManager created by this wallet.",
          });
        } else {
          setManagerNotice({
            tone: "info",
            message:
              "No PredictManager found for this wallet yet. Create one before funding or opening a strategy.",
          });
        }
      } catch (err) {
        if (cancelled) return;
        setManagerNotice({
          tone: "error",
          message:
            err instanceof Error
              ? `Could not auto-discover a PredictManager: ${err.message}`
              : "Could not auto-discover a PredictManager.",
        });
      } finally {
        if (!cancelled) setManagerDiscovering(false);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [connectedAddress, findManagerForWallet, isTestnet]);

  useEffect(() => {
    if (!connectedAddress) {
      setPortfolioHistory([]);
      return;
    }
    setPortfolioHistory(readPortfolioHistory(connectedAddress));
  }, [connectedAddress]);

  useEffect(() => {
    const stored = window.localStorage.getItem("structx.mode");
    if (stored === "normal" || stored === "advanced") {
      setMode(stored);
    }
  }, []);

  const updateMode = useCallback((nextMode: AppMode) => {
    setMode(nextMode);
    window.localStorage.setItem("structx.mode", nextMode);
  }, []);

  const updateWorkspaceView = useCallback((nextView: WorkspaceView) => {
    setWorkspaceView(nextView);
    setSearch("");
    if (nextView !== "positions") {
      setActiveTab("All");
    }
    // We used to force `mode = "advanced"` when entering the positions
    // workspace and persist it to localStorage. That had a nasty side
    // effect: the moment a user viewed positions once, every subsequent
    // launch landed in Advanced Mode regardless of intent — because the
    // localStorage read on mount kept reinstating it. The positions view
    // doesn't render either Normal or Advanced (`showNormalMode` already
    // gates on `!showPositionsWorkspace`), so the mode value is dormant
    // while positions is active and shouldn't be touched here.
  }, []);

  useEffect(() => {
    if (!connectedAddress) return;

    const key = `structx.manager.${connectedAddress}`;
    if (managerId) {
      window.localStorage.setItem(key, managerId);
    } else {
      window.localStorage.removeItem(key);
    }
  }, [connectedAddress, managerId]);

  const updateStage = useCallback((stage: ExecutionStage, status: StageStatus) => {
    setStages((prev) => ({ ...prev, [stage]: status }));
  }, []);

  const loadManagerBalance = useCallback(async (id: string) => {
    if (!id) {
      setManagerBalance(null);
      return;
    }
    setManagerBalanceLoading(true);
    setManagerBalance(null);
    try {
      const json = await getManagerBalance(id);
      setManagerBalance(json);
    } catch (err) {
      if (err instanceof ApiError) {
        setManagerBalance({
          ok: false,
          error: err.body.message ?? err.body.error ?? err.message,
        });
      } else if (err instanceof Error) {
        setManagerBalance({ ok: false, error: err.message });
      }
    } finally {
      setManagerBalanceLoading(false);
    }
  }, []);

  const onCreateManager = useCallback(async () => {
    if (!connectedAddress) {
      setManagerNotice({
        tone: "error",
        message: "Connect a wallet before creating a PredictManager.",
      });
      return;
    }

    if (!isTestnet) {
      setManagerNotice({
        tone: "error",
        message: "Switch to Sui Testnet before creating a PredictManager.",
      });
      return;
    }

    setCreatingManager(true);
    setManagerNotice(null);

    try {
      const tx = buildCreateManagerTransaction(connectedAddress);
      const execution = await signAndExecuteTransaction({
        transaction: tx,
        chain: "sui:testnet",
      });

      const confirmed = await suiClient.waitForTransaction({
        digest: execution.digest,
        options: {
          showEffects: true,
          showObjectChanges: true,
        },
      });

      if (confirmed.effects?.status?.status !== "success") {
        throw new Error(
          confirmed.effects?.status?.error ??
            "Create PredictManager transaction failed.",
        );
      }

      const createdManagerId = extractPredictManagerIdFromObjectChanges(
        confirmed.objectChanges ?? [],
      );

      if (!createdManagerId) {
        throw new Error(
          "PredictManager transaction succeeded, but no new manager id was found in object changes.",
        );
      }

      setManagerId(createdManagerId);
      setManagerNotice({
        tone: "info",
        message: "PredictManager created for the connected wallet.",
      });
      flashToast("PredictManager created");
      await loadManagerBalance(createdManagerId);
    } catch (err) {
      const friendly = mapError(err);
      setManagerNotice({
        tone: "error",
        message: friendly.message,
      });
    } finally {
      setCreatingManager(false);
    }
  }, [
    connectedAddress,
    flashToast,
    isTestnet,
    loadManagerBalance,
    signAndExecuteTransaction,
    suiClient,
  ]);

  useEffect(() => {
    if (managerId) void loadManagerBalance(managerId);
  }, [managerId, loadManagerBalance]);

  // Strategy filtering
  const filteredStrategies = useMemo(
    () => filterStrategies(STRATEGY_CATALOG, activeTab, search),
    [activeTab, search],
  );
  const categoryCounts = useMemo(() => {
    const counts: Partial<Record<CategoryTab, number>> = {};
    for (const tab of CATEGORY_TABS) {
      counts[tab.id] = filterStrategies(STRATEGY_CATALOG, tab.id, search).length;
    }
    return counts;
  }, [search]);

  const onSelectStrategy = useCallback((id: string) => {
    const entry = STRATEGY_CATALOG.find((s) => s.id === id);
    if (!entry) return;
    updateMode("advanced");
    setWorkspaceView("strategies");
    setActiveStrategyId(id);
    // Reset any in-flight result so a fresh preview can run for the new preset.
    setCompiled(null);
    setCompileError(null);
    setDryRunOk(false);
    setDryRunError(null);
    setOpenError(null);
    setAudit(null);
    // Scroll to builder
    setTimeout(() => {
      builderRef.current?.scrollIntoView({ behavior: "smooth", block: "start" });
    }, 50);
  }, [updateMode]);

  const activeEntry = useMemo(
    () => findCatalogEntryById(activeStrategyId),
    [activeStrategyId],
  );

  const onCompile = useCallback(async () => {
    setCompileLoading(true);
    setCompileError(null);
    setCompiled(null);
    setAudit(null);
    setDryRunOk(false);
    setDryRunError(null);
    setOpenError(null);
    updateStage("preview", "active");
    updateStage("preflight", "pending");
    updateStage("dryRun", "pending");
    updateStage("signature", "pending");
    updateStage("submitted", "pending");
    updateStage("audited", "pending");

    try {
      const json = await compileStrategy({
        owner,
        strategy: activeEntry?.strategyId ?? "BREAKOUT_PROTECTION",
        budgetDUSDC,
        style,
        expiryPreference: "nearest_active",
        slippageBps: Number(slippageBps),
        ...(activeEntry?.strategyId === "PORTFOLIO_CRASH_SHIELD" ||
        activeEntry?.strategyId === "SMART_BUDGET_SELECTOR"
          ? {
              portfolioExposureDUSDC: Number(portfolioExposureDUSDC),
              overHedgeCapBps: Number(overHedgeCapBps),
            }
          : {}),
        ...(activeEntry?.strategyId === "CONVEX_TAIL_LADDER" ||
        activeEntry?.strategyId === "SMART_BUDGET_SELECTOR"
          ? {
              deadZoneBps: Number(deadZoneBps),
              convexGammaBps: Number(convexGammaBps),
            }
          : {}),
        ...(activeEntry?.strategyId === "MOONSHOT_UPSIDE" ||
        activeEntry?.strategyId === "SMART_BUDGET_SELECTOR"
          ? {
              moonshotRangeWeightBps: Number(moonshotRangeWeightBps),
              moonshotTailGammaBps: Number(moonshotTailGammaBps),
            }
          : {}),
        ...(activeEntry?.strategyId === "UPSIDE_STEP_LADDER"
          ? {
              upsideNearRangeWeightBps: Number(upsideNearRangeWeightBps),
              upsideUpperRangeWeightBps: Number(upsideUpperRangeWeightBps),
              upsideTailGammaBps: Number(upsideTailGammaBps),
            }
          : {}),
        ...(activeEntry?.strategyId === "DOWNSIDE_STEP_LADDER"
          ? {
              downsideNearRangeWeightBps: Number(downsideNearRangeWeightBps),
              downsideLowerRangeWeightBps: Number(downsideLowerRangeWeightBps),
              downsideStepTailGammaBps: Number(downsideStepTailGammaBps),
            }
          : {}),
        ...(activeEntry?.strategyId === "CENTER_BAND_CONDOR"
          ? {
              condorCenterWeightBps: Number(condorCenterWeightBps),
            }
          : {}),
        ...(activeEntry?.strategyId === "NEAR_BARRIER_PROXY"
          ? {
              barrierSide,
              barrierNearRangeWeightBps: Number(barrierNearRangeWeightBps),
              barrierTailGammaBps: Number(barrierTailGammaBps),
            }
          : {}),
      });
      setCompiled(json);
      updateStage("preview", "success");
      updateStage("preflight", "active");
      setTimeout(() => {
        previewRef.current?.scrollIntoView({ behavior: "smooth", block: "start" });
      }, 80);
    } catch (err) {
      setCompileError(mapError(err));
      updateStage("preview", "failed");
    } finally {
      setCompileLoading(false);
    }
  }, [
    owner,
    budgetDUSDC,
    style,
    slippageBps,
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
    activeEntry,
    updateStage,
  ]);

  const premiumOk = useMemo(() => {
    if (!compiled || !managerBalance?.balanceRaw) return false;
    try {
      return (
        BigInt(managerBalance.balanceRaw) >= BigInt(compiled.premiumRequiredRaw)
      );
    } catch {
      return false;
    }
  }, [compiled, managerBalance]);

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

  useEffect(() => {
    if (!compiled) return;
    const allGood =
      Boolean(connectedAddress) &&
      isTestnet &&
      Boolean(managerId) &&
      Boolean(managerBalance?.ok) &&
      premiumOk;
    updateStage("preflight", allGood ? "success" : "active");
  }, [
    compiled,
    connectedAddress,
    isTestnet,
    managerId,
    managerBalance,
    premiumOk,
    updateStage,
  ]);

  const preflightWarnings = useMemo(() => {
    const warnings: string[] = [];
    if (!connectedAddress)
      warnings.push("Connect a Sui wallet before opening the strategy.");
    if (!isTestnet) warnings.push("Wrong network. Switch wallet to Sui Testnet.");
    if (!managerId) warnings.push("Create or enter a PredictManager ID.");
    if (compiled && connectedAddress && compiled.owner !== connectedAddress) {
      warnings.push(
        "Compiled owner does not match connected wallet. Re-preview with connected wallet.",
      );
    }
    if (compiled && managerBalance?.balanceRaw) {
      try {
        if (
          BigInt(managerBalance.balanceRaw) <
          BigInt(compiled.premiumRequiredRaw)
        ) {
          warnings.push(
            `Insufficient selected manager balance. Required ${compiled.premiumRequiredDisplay}, available ${managerBalance.balanceDisplay ?? managerBalance.balanceRaw}.`,
          );
        }
      } catch {
        // ignored
      }
    }
    if (compiled && !managerBalance?.ok) {
      warnings.push("Selected manager balance could not be verified.");
    }
    return warnings;
  }, [
    compiled,
    connectedAddress,
    isTestnet,
    managerId,
    managerBalance,
  ]);

  const displayWarnings = useMemo(() => {
    if (!compiled) return preflightWarnings;
    const baseWarnings = compiled.warnings.filter((w) => {
      const norm = w.toLowerCase();
      return !(
        norm.includes("testnet-only") || norm.includes("skipped oracle ")
      );
    });
    return [...baseWarnings, ...preflightWarnings];
  }, [compiled, preflightWarnings]);

  const canDryRun =
    Boolean(compiled) &&
    Boolean(connectedAddress) &&
    isTestnet &&
    Boolean(managerId) &&
      Boolean(managerBalance?.ok) &&
    premiumOk;

  const canOpen = canDryRun && dryRunOk;

  const onDryRun = useCallback(async () => {
    if (!compiled || !connectedAddress) return;
    setDryRunning(true);
    setDryRunError(null);
    setDryRunOk(false);
    updateStage("dryRun", "active");
    try {
      const build = await buildOpenStrategy({
        owner: connectedAddress,
        managerId,
        compiledStrategyId: compiled.compiledStrategyId,
        maxPremiumRaw: compiled.premiumRequiredRaw,
        slippageBps: Number(slippageBps),
      });
      const tx = buildOpenStrategyTransaction(build);
      const result = await suiClient.devInspectTransactionBlock({
        sender: connectedAddress,
        transactionBlock: tx,
      });
      const status = result.effects?.status;
      if (status?.status === "success") {
        setDryRunOk(true);
        updateStage("dryRun", "success");
      } else {
        setDryRunOk(false);
        setDryRunError(mapDryRunFailure(status?.error ?? "dry-run failed"));
        updateStage("dryRun", "failed");
      }
    } catch (err) {
      setDryRunOk(false);
      setDryRunError(mapError(err));
      updateStage("dryRun", "failed");
    } finally {
      setDryRunning(false);
    }
  }, [
    compiled,
    connectedAddress,
    managerId,
    slippageBps,
    suiClient,
    updateStage,
  ]);

  const onOpenStrategy = useCallback(async () => {
    if (!compiled || !connectedAddress) return;
    setOpening(true);
    setOpenError(null);
    setAudit(null);
    updateStage("signature", "active");
    updateStage("submitted", "pending");
    updateStage("audited", "pending");
    try {
      const build = await buildOpenStrategy({
        owner: connectedAddress,
        managerId,
        compiledStrategyId: compiled.compiledStrategyId,
        maxPremiumRaw: compiled.premiumRequiredRaw,
        slippageBps: Number(slippageBps),
      });
      const tx = buildOpenStrategyTransaction(build);
      const execution = await signAndExecuteTransaction({
        transaction: tx,
        chain: "sui:testnet",
      });
      updateStage("signature", "success");
      updateStage("submitted", "active");
      const confirmed = await suiClient.waitForTransaction({
        digest: execution.digest,
        options: {
          showEffects: true,
          showEvents: true,
          showObjectChanges: true,
        },
      });
      if (confirmed.effects?.status?.status !== "success") {
        updateStage("submitted", "failed");
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
      updateStage("submitted", "success");
      updateStage("audited", "active");
      const auditJson = await auditOpenStrategy({
        owner: connectedAddress,
        managerId,
        compiledStrategyId: build.compiledStrategyId,
        digest: execution.digest,
        effects: confirmed.effects ?? {},
        events: confirmed.events ?? [],
        objectChanges: confirmed.objectChanges ?? [],
      });
      setAudit(auditJson);
      const actualStrategy = compiled.selectedStrategy ?? compiled.strategy;
      const actualEntry = findCatalogEntryByStrategyId(actualStrategy);
      const nextHistory = appendPortfolioHistory(connectedAddress, {
        id: execution.digest,
        owner: connectedAddress,
        managerId,
        strategy: actualStrategy,
        requestedStrategy:
          compiled.selectedStrategy && compiled.selectedStrategy !== compiled.strategy
            ? compiled.strategy
            : undefined,
        displayName: actualEntry?.displayName ?? "StructX strategy",
        compiledStrategyId: build.compiledStrategyId,
        digest: execution.digest,
        explorerUrl: auditJson.explorerUrl,
        openedAt: new Date().toISOString(),
        expiry: compiled.expiry,
        premiumPaidRaw: auditJson.totalCostRaw ?? compiled.premiumRequiredRaw,
        premiumPaidDisplay:
          auditJson.totalCostDisplay ?? compiled.premiumRequiredDisplay,
        maxLossRaw: compiled.maxLossRaw,
        maxLossDisplay: compiled.maxLossDisplay,
        maxGrossPayoutRaw: compiled.maxGrossPayoutRaw,
        maxGrossPayoutDisplay: compiled.maxGrossPayoutDisplay,
        maxNetPayoutRaw: compiled.maxNetPayoutRaw,
        maxNetPayoutDisplay: compiled.maxNetPayoutDisplay,
        managerBalanceRaw: auditJson.managerBalanceRaw ?? null,
        managerBalanceDisplay: auditJson.managerBalanceDisplay ?? null,
        executionStatus: auditJson.executionStatus,
        auditOk: auditJson.ok,
        legCount: auditJson.mintedLegs?.length ?? compiled.legs.length,
        mintedLegs: auditJson.mintedLegs ?? [],
        categories: actualEntry?.categories,
        riskLabel: actualEntry?.riskLabel,
      });
      setPortfolioHistory(nextHistory);
      flashToast("Saved to positions");
      updateStage("audited", auditJson.ok ? "success" : "failed");
      await loadManagerBalance(managerId);
    } catch (err) {
      const friendly = mapError(err);
      setOpenError(friendly);
      if (friendly.title === "Signature rejected") {
        updateStage("signature", "failed");
      } else {
        updateStage("submitted", "failed");
      }
    } finally {
      setOpening(false);
    }
  }, [
    compiled,
    connectedAddress,
    managerId,
    slippageBps,
    signAndExecuteTransaction,
    suiClient,
    updateStage,
    loadManagerBalance,
    flashToast,
  ]);

  const compileDisabled =
    compileLoading || !owner || !budgetDUSDC || Number(budgetDUSDC) <= 0;
  const managerBalanceDisplay = managerBalance?.ok
    ? (managerBalance.balanceDisplay ?? null)
    : null;

  const disabledReason = useMemo(() => {
    if (!compiled) return "Preview the payoff first.";
    if (!connectedAddress) return "Connect a Sui wallet.";
    if (!isTestnet) return "Switch wallet to Sui Testnet.";
    if (managerDiscovering) return "Still looking up your PredictManager.";
    if (creatingManager) {
      return "Approve the PredictManager creation in your wallet.";
    }
    if (!managerId) return "Create or enter a PredictManager first.";
    if (managerBalanceLoading) return "Checking selected manager balance.";
    if (!managerBalance?.ok) {
      return managerBalance?.error ?? "Selected manager balance unavailable.";
    }
    if (!premiumOk) return "Selected manager balance is below required premium.";
    if (!dryRunOk) return "Run the dry-run successfully before opening.";
    return null;
  }, [
    compiled,
    connectedAddress,
    creatingManager,
    isTestnet,
    managerBalance,
    managerBalanceLoading,
    managerDiscovering,
    managerId,
    premiumOk,
    dryRunOk,
  ]);

  const phase: "preflight" | "ready-dryrun" | "ready-open" | "running" | "done" =
    !canDryRun
      ? "preflight"
      : audit
        ? "done"
        : opening || dryRunning
          ? "running"
          : canOpen
            ? "ready-open"
            : "ready-dryrun";

  const showPositionsWorkspace = workspaceView === "positions";
  const showNormalMode = mode === "normal" && !showPositionsWorkspace;
  const showHero = !compiled && !showPositionsWorkspace;
  const builderVisible =
    !showPositionsWorkspace && (Boolean(activeStrategyId) || Boolean(compiled));
  const builderEntry =
    activeEntry ?? findCatalogEntryById(LIVE_STRATEGY_ID) ?? STRATEGY_CATALOG[0];

  const onUseRecommendation = useCallback(
    (
      nextCompiled: GuidedCompileResponse,
      parsedIntent: ParsedIntentSuccess,
      action: "open" | "customize",
    ) => {
      setOwner(connectedAddress ?? parsedIntent.owner);
      setBudgetDUSDC(parsedIntent.budgetDUSDC);
      setStyle(parsedIntent.recommendedStyle as StrategyStyle);
      setActiveTab("All");
      setSearch("");
      setWorkspaceView("strategies");
      setActiveStrategyId(
        findCatalogEntryByStrategyId(nextCompiled.strategy)?.id ?? LIVE_STRATEGY_ID,
      );
      setCompiled(nextCompiled);
      setCompileError(null);
      setDryRunOk(false);
      setDryRunError(null);
      setOpenError(null);
      setAudit(null);
      setStages({
        ...INITIAL_STAGES,
        configure: "success",
        preview: "success",
        preflight: "active",
      });
      updateMode("advanced");
      setTimeout(() => {
        const target = action === "open" ? previewRef.current : builderRef.current;
        target?.scrollIntoView({ behavior: "smooth", block: "start" });
      }, 100);
    },
    [connectedAddress, updateMode],
  );

  return (
    <main className="page" id="top">
      <Header
        searchValue={search}
        onSearchChange={setSearch}
        onCopied={onCopied}
        currentView={workspaceView}
        onViewChange={updateWorkspaceView}
        managerBalance={managerBalanceDisplay}
        searchPlaceholder={
          workspaceView === "positions"
            ? "Search positions, trades, digests, manager…"
            : "Search strategies, payoff types, BTC…"
        }
      />

      <ModeToggle mode={mode} onChange={updateMode} />

      {showNormalMode ? (
        <div className="page-body">
          <NormalModeView
            owner={owner}
            connectedAddress={connectedAddress}
            managerId={managerId}
            managerBalance={managerBalanceDisplay}
            managerBalanceLoading={managerBalanceLoading}
            managerDiscovering={managerDiscovering}
            managerNotice={managerNotice?.message ?? null}
            managerNoticeTone={managerNotice?.tone ?? null}
            creatingManager={creatingManager}
            onRefreshBalance={() => void loadManagerBalance(managerId)}
            onCreateManager={() => void onCreateManager()}
            onUseRecommendation={onUseRecommendation}
            onCopied={onCopied}
          />

          <footer className="page-foot">
            <DeepBookPredictAttribution variant="footer" />
          </footer>
        </div>
      ) : (
        <>
          {!showPositionsWorkspace && (
            <CategoryNav
              active={activeTab}
              onChange={setActiveTab}
              counts={categoryCounts}
            />
          )}

          <div className="page-body">
            {showPositionsWorkspace ? (
              <PortfolioDashboard
                connectedAddress={connectedAddress}
                managerId={managerId}
                managerBalance={managerBalance}
                managerBalanceLoading={managerBalanceLoading}
                history={portfolioHistory}
                query={search}
                onShowStrategies={() => updateWorkspaceView("strategies")}
                onCopied={onCopied}
              />
            ) : (
              <>
            {showHero && (
              <section className="hero-band">
                <div className="hero-text">
                  <h1>Compile BTC payoff strategies with Struct X</h1>
                  <p>
                    Struct X is a payoff compiler built on DeepBook Predict. It
                    turns pre-designed strategy ideas into cleaner trade tickets, so
                    you can preview the payoff shape, sign from your wallet, and
                    audit what was minted on Sui Testnet.
                  </p>
                  <DeepBookPredictAttribution variant="hero" />
                  <div className="hero-ctas">
                    <button
                      type="button"
                      className="primary-button compact"
                      onClick={() => onSelectStrategy(LIVE_STRATEGY_ID)}
                    >
                      Build Breakout Protection
                    </button>
                    <button
                      type="button"
                      className="ghost-button"
                      onClick={() => void loadManagerBalance(managerId)}
                    >
                      View selected manager balance
                    </button>
                  </div>
                </div>
                <div className="hero-card">
                  <p className="eyebrow">Recommended</p>
                  <h3>Breakout Protection</h3>
                  <p className="muted">
                    Two-sided protection for large expiry moves. DOWN + RANGE legs
                    + UP, all wallet-signed.
                  </p>
                  <button
                    type="button"
                    className="primary-button compact"
                    onClick={() => onSelectStrategy(LIVE_STRATEGY_ID)}
                  >
                    Build strategy →
                  </button>
                </div>
              </section>
            )}

            <section className="section">
              <div className="section-head">
                <h2>
                  {activeTab === "All" ? "All strategies" : activeTab}
                  {search && <span className="muted"> · for “{search}”</span>}
                </h2>
                <p className="muted">
                  {filteredStrategies.length}{" "}
                  {filteredStrategies.length === 1 ? "strategy" : "strategies"}
                </p>
              </div>
              <StrategyGrid
                entries={filteredStrategies}
                activeId={activeStrategyId}
                onSelect={onSelectStrategy}
                query={search}
                onClearFilters={() => {
                  setSearch("");
                  setActiveTab("All");
                }}
              />
            </section>

            {builderVisible && (
              <section className="section" ref={builderRef}>
                <div className="section-head">
                  <h2>Trade ticket</h2>
                </div>
                <div className="builder-layout">
                  <StrategyBuilder
                    strategyId={builderEntry.strategyId}
                    displayName={builderEntry.displayName}
                    legHint={builderEntry.legHint}
                    status={builderEntry.status}
                    owner={owner}
                    onOwnerChange={setOwner}
                    walletAddress={connectedAddress}
                    managerId={managerId}
                    onManagerIdChange={setManagerId}
                    budgetDUSDC={budgetDUSDC}
                    onBudgetChange={setBudgetDUSDC}
                    slippageBps={slippageBps}
                    onSlippageChange={setSlippageBps}
                    style={style}
                    onStyleChange={setStyle}
                    portfolioExposureDUSDC={portfolioExposureDUSDC}
                    onPortfolioExposureChange={setPortfolioExposureDUSDC}
                    overHedgeCapBps={overHedgeCapBps}
                    onOverHedgeCapBpsChange={setOverHedgeCapBps}
                    deadZoneBps={deadZoneBps}
                    onDeadZoneBpsChange={setDeadZoneBps}
                    convexGammaBps={convexGammaBps}
                    onConvexGammaBpsChange={setConvexGammaBps}
                    moonshotRangeWeightBps={moonshotRangeWeightBps}
                    onMoonshotRangeWeightBpsChange={setMoonshotRangeWeightBps}
                    moonshotTailGammaBps={moonshotTailGammaBps}
                    onMoonshotTailGammaBpsChange={setMoonshotTailGammaBps}
                    upsideNearRangeWeightBps={upsideNearRangeWeightBps}
                    onUpsideNearRangeWeightBpsChange={setUpsideNearRangeWeightBps}
                    upsideUpperRangeWeightBps={upsideUpperRangeWeightBps}
                    onUpsideUpperRangeWeightBpsChange={setUpsideUpperRangeWeightBps}
                    upsideTailGammaBps={upsideTailGammaBps}
                    onUpsideTailGammaBpsChange={setUpsideTailGammaBps}
                    downsideNearRangeWeightBps={downsideNearRangeWeightBps}
                    onDownsideNearRangeWeightBpsChange={setDownsideNearRangeWeightBps}
                    downsideLowerRangeWeightBps={downsideLowerRangeWeightBps}
                    onDownsideLowerRangeWeightBpsChange={setDownsideLowerRangeWeightBps}
                    downsideStepTailGammaBps={downsideStepTailGammaBps}
                    onDownsideStepTailGammaBpsChange={setDownsideStepTailGammaBps}
                    condorCenterWeightBps={condorCenterWeightBps}
                    onCondorCenterWeightBpsChange={setCondorCenterWeightBps}
                    barrierSide={barrierSide}
                    onBarrierSideChange={setBarrierSide}
                    barrierNearRangeWeightBps={barrierNearRangeWeightBps}
                    onBarrierNearRangeWeightBpsChange={setBarrierNearRangeWeightBps}
                    barrierTailGammaBps={barrierTailGammaBps}
                    onBarrierTailGammaBpsChange={setBarrierTailGammaBps}
                    managerBalance={managerBalanceDisplay}
                    managerBalanceLoading={managerBalanceLoading}
                    managerDiscovering={managerDiscovering}
                    managerNotice={managerNotice?.message ?? null}
                    managerNoticeTone={managerNotice?.tone ?? null}
                    creatingManager={creatingManager}
                    onRefreshBalance={() => void loadManagerBalance(managerId)}
                    onCreateManager={() => void onCreateManager()}
                    onCompile={() => void onCompile()}
                    compileDisabled={compileDisabled}
                    compiling={compileLoading}
                    disabledReason={
                      compileDisabled
                        ? !owner
                          ? "Owner address required"
                          : Number(budgetDUSDC) <= 0
                            ? "Set a positive budget"
                            : null
                        : null
                    }
                    onCopied={onCopied}
                  />

                  <div className="preview-column" ref={previewRef}>
                    {compileError && <ErrorNotice error={compileError} />}
                    {compileLoading && <SkeletonCard lines={5} />}
                    {!compiled && !compileLoading && !compileError && (
                      <EmptyState
                        title="Preview your payoff"
                        body="Fill in the trade ticket and tap Preview payoff to see the legs, payoff bands, and dry-run options."
                      />
                    )}

                    {compiled && (
                      <>
                        <PreviewSummary
                          compiled={compiled}
                          displayName={builderEntry.displayName}
                          onCopied={onCopied}
                        />
                        {compiled.smartSelector && (
                          <SmartSelectorPanel info={compiled.smartSelector} />
                        )}
                        <PayoffVisualization compiled={compiled} />
                        <LegsTable legs={compiled.legs} />
                        <PayoffTable
                          rows={compiled.payoffTable}
                          strikes={compiled.strikes}
                        />

                        {displayWarnings.length > 0 && (
                          <WarningsPanel warnings={displayWarnings} />
                        )}

                        <ExecutionPanel
                          stages={stages}
                          checks={[
                            {
                              ok: Boolean(connectedAddress),
                              label: "Wallet connected",
                              detail: connectedAddress
                                ? "Connected"
                                : "Click Connect Wallet in the header.",
                            },
                            {
                              ok: isTestnet,
                              label: "Sui Testnet selected",
                              detail: isTestnet
                                ? "Network OK"
                                : "Switch wallet to Sui Testnet.",
                            },
                            {
                              ok: Boolean(managerId),
                              label: "PredictManager ID set",
                              detail: managerId ? "Set" : "Required",
                            },
                            {
                              ok: Boolean(managerBalance?.ok),
                              label: managerBalanceDisplay
                                ? `Selected manager balance: ${managerBalanceDisplay}`
                                : "Selected manager balance verified",
                              detail: managerBalance?.ok
                                ? "Verified on-chain"
                                : "Unable to verify balance",
                            },
                            {
                              ok: Boolean(compiled),
                              label: "Strategy compiled",
                              detail: compiled ? "Plan ready" : "Click Preview payoff",
                            },
                            {
                              ok: true,
                              label: "Budget target",
                              detail: compiled
                                ? `${compiled.premiumRequiredDisplay} vs target ${compiled.budgetDisplay}`
                                : "—",
                            },
                            {
                              ok: premiumOk,
                              label: "Selected manager balance covers premium",
                              detail:
                                compiled && managerBalanceDisplay
                                  ? `Need ${compiled.premiumRequiredDisplay}, have ${managerBalanceDisplay}`
                                  : "Refresh balance",
                            },
                          ]}
                          phase={phase}
                          dryRunning={dryRunning}
                          opening={opening}
                          onDryRun={() => void onDryRun()}
                          onOpen={() => void onOpenStrategy()}
                          onRefreshBalance={() => void loadManagerBalance(managerId)}
                          disabledReason={disabledReason}
                        />

                        {dryRunError && <ErrorNotice error={dryRunError} />}
                        {openError && <ErrorNotice error={openError} />}

                        {audit && <AuditReceipt audit={audit} onCopied={onCopied} />}
                      </>
                    )}
                  </div>
                </div>
              </section>
            )}
              </>
            )}

            <footer className="page-foot">
              <DeepBookPredictAttribution variant="footer" />
            </footer>
          </div>
        </>
      )}

      <Toast message={toast} />

      <Styles />
    </main>
  );
}

function Styles() {
  return (
    <style jsx global>{`
      * {
        box-sizing: border-box;
      }
      :root {
        --bg: #0d0e12;
        --bg-2: #161821;
        --surface: #1c1e26;
        --surface-2: #232631;
        --surface-3: #2d3140;
        --border: #262934;
        --border-strong: #353a48;
        --text: #ffffff;
        --text-dim: #c5cad6;
        --text-muted: #858ea0;
        --text-faint: #5a6273;
        --accent: #2962ff;
        --accent-hover: #1d4eda;
        --accent-soft: rgba(41, 98, 255, 0.16);
        --success: #00d27a;
        --success-soft: rgba(0, 210, 122, 0.16);
        --danger: #ef4444;
        --danger-soft: rgba(239, 68, 68, 0.16);
        --warning: #f97316;
        --warning-soft: rgba(249, 115, 22, 0.16);
        --violet: #a78bfa;
        --violet-soft: rgba(167, 139, 250, 0.16);
        --radius-sm: 6px;
        --radius: 10px;
        --radius-lg: 14px;
        --radius-pill: 999px;
        --shadow-card: 0 1px 0 rgba(255, 255, 255, 0.02) inset;
        --shadow-pop: 0 18px 40px rgba(0, 0, 0, 0.5);
      }
      html, body { margin: 0; padding: 0; }
      body {
        background: var(--bg);
        color: var(--text);
        font-family: var(--font-sans), "Inter", ui-sans-serif, system-ui,
          -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
        font-size: 14px;
        font-weight: 400;
        line-height: 1.5;
        letter-spacing: -0.005em;
        -webkit-font-smoothing: antialiased;
        text-rendering: optimizeLegibility;
      }
      button, input, select, textarea {
        font: inherit;
        font-family: inherit;
      }
      h1, h2, h3, h4 {
        font-weight: 700;
        letter-spacing: -0.02em;
      }
      a { color: inherit; }
      code,
      .mono,
      .num,
      td.mono,
      .receipt-row-value code {
        font-family: var(--font-plex-mono), ui-monospace, SFMono-Regular,
          "IBM Plex Mono", Menlo, Consolas, monospace;
        font-feature-settings: "zero" 1, "ss01" 1;
        font-variant-numeric: tabular-nums;
      }
      .page { min-height: 100vh; background: var(--bg); }
      .page-body {
        max-width: 1280px;
        margin: 0 auto;
        padding: 24px 24px 64px;
        display: grid;
        gap: 28px;
      }
      .mode-toggle-wrap {
        max-width: 1280px;
        margin: 0 auto;
        padding: 18px 24px 0;
      }
      .mode-toggle {
        display: inline-grid;
        grid-template-columns: repeat(2, minmax(0, 1fr));
        gap: 6px;
        padding: 6px;
        background: var(--surface);
        border: 1px solid var(--border);
        border-radius: var(--radius-pill);
      }
      .mode-toggle button {
        all: unset;
        cursor: pointer;
        display: grid;
        gap: 2px;
        min-width: 160px;
        padding: 10px 16px;
        border-radius: var(--radius-pill);
        color: var(--text-muted);
        font-size: 13px;
        font-weight: 800;
      }
      .mode-toggle button span {
        font-size: 11px;
        font-weight: 600;
        color: var(--text-faint);
      }
      .mode-toggle button.active {
        background: linear-gradient(135deg, #38bdf8, #a78bfa);
        color: var(--bg);
      }
      .mode-toggle button.active span {
        color: rgba(13, 14, 18, 0.78);
      }
      .normal-layout {
        display: grid;
        grid-template-columns: 420px minmax(0, 1fr);
        gap: 20px;
        align-items: start;
      }
      .guided-panel textarea {
        width: 100%;
        resize: vertical;
        background: var(--bg-2);
        color: var(--text);
        border: 1px solid var(--border);
        border-radius: var(--radius-sm);
        padding: 11px 12px;
        outline: none;
        line-height: 1.5;
      }
      .guided-panel textarea:focus {
        border-color: var(--accent);
        box-shadow: 0 0 0 3px var(--accent-soft);
      }
      .intent-chips {
        display: flex;
        flex-wrap: wrap;
        gap: 8px;
        margin: 18px 0 4px;
      }
      .intent-chips button {
        border: 1px solid var(--border);
        background: var(--bg-2);
        color: var(--text-dim);
        border-radius: var(--radius-pill);
        padding: 8px 10px;
        cursor: pointer;
        font-size: 11px;
        font-weight: 700;
        line-height: 1.35;
      }
      .intent-chips button:hover {
        border-color: var(--border-strong);
        color: var(--text);
      }
      .guided-disclaimer {
        margin-top: 12px;
        padding: 12px;
        border-radius: var(--radius-sm);
        border: 1px solid rgba(41, 98, 255, 0.28);
        background: rgba(41, 98, 255, 0.08);
        color: #c7d2fe;
        font-size: 12px;
        line-height: 1.5;
      }
      .normal-reason {
        margin-top: 14px;
      }
      .recommendation-card {
        border-color: rgba(0, 210, 122, 0.24);
      }
      .top-space {
        margin-top: 12px;
      }

      /* ===== Header ===== */
      .app-header {
        position: sticky;
        top: 0;
        z-index: 30;
        background: rgba(13, 14, 18, 0.85);
        backdrop-filter: saturate(140%) blur(14px);
        border-bottom: 1px solid var(--border);
      }
      .app-header-inner {
        max-width: 1280px;
        margin: 0 auto;
        padding: 12px 24px;
        display: grid;
        grid-template-columns: auto 1fr auto;
        align-items: center;
        gap: 18px;
      }
      .brand {
        display: inline-flex;
        align-items: center;
        gap: 8px;
        text-decoration: none;
        color: var(--text);
      }
      .brand-mark {
        width: 28px;
        height: 28px;
        border-radius: 8px;
        background: linear-gradient(135deg, #6e3afe 0%, #4a1ed8 100%);
        color: white;
        display: grid;
        place-items: center;
      }
      .brand-text {
        font-size: 17px;
        font-weight: 800;
        letter-spacing: -0.02em;
        color: var(--text);
      }
      .search-wrap {
        position: relative;
        display: flex;
        align-items: center;
        background: var(--surface-2);
        border: 1px solid var(--border);
        border-radius: var(--radius-pill);
        padding: 0 12px 0 38px;
        height: 40px;
        max-width: 720px;
        margin: 0 auto;
        width: 100%;
      }
      .search-wrap:focus-within {
        border-color: var(--border-strong);
        background: var(--surface-3);
      }
      .search-wrap input {
        flex: 1;
        background: transparent;
        border: 0;
        outline: none;
        color: var(--text);
        font-size: 13px;
        height: 100%;
      }
      .search-wrap input::placeholder {
        color: var(--text-muted);
      }
      .search-icon {
        position: absolute;
        left: 12px;
        color: var(--text-muted);
        pointer-events: none;
      }
      .search-kbd {
        font-size: 11px;
        color: var(--text-muted);
        background: var(--surface);
        border: 1px solid var(--border);
        border-radius: 4px;
        padding: 1px 6px;
        font-family: inherit;
      }
      .header-right {
        display: inline-flex;
        align-items: center;
        gap: 14px;
      }
      .header-stat {
        display: grid;
        line-height: 1.05;
        text-align: right;
      }
      .header-stat-label {
        color: var(--text-muted);
        font-size: 11px;
        font-weight: 500;
      }
      .header-stat-value {
        font-size: 14px;
        font-weight: 700;
        margin-top: 2px;
      }
      .header-stat-value.pos { color: var(--success); }
      .header-stat-value.neg { color: var(--danger); }

      .connect-slot button,
      .connect-slot [role="button"] {
        background: var(--accent) !important;
        color: white !important;
        border-radius: var(--radius) !important;
        font-weight: 600 !important;
        padding: 9px 16px !important;
        border: 0 !important;
        font-size: 13px !important;
      }
      .connect-slot button:hover {
        background: var(--accent-hover) !important;
      }

      .avatar-menu {
        position: relative;
      }
      .avatar-trigger {
        display: inline-flex;
        align-items: center;
        gap: 10px;
        min-height: 42px;
        padding: 5px 10px 5px 6px;
        border-radius: var(--radius-pill);
        border: 1px solid var(--border);
        background: var(--surface-2);
        color: var(--text);
        font-size: 13px;
        font-weight: 700;
        cursor: pointer;
        transition: background 0.12s ease, border-color 0.12s ease;
      }
      .avatar-trigger:hover,
      .avatar-trigger.open {
        background: var(--surface-3);
        border-color: var(--border-strong);
      }
      .avatar-trigger-label {
        max-width: 124px;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
      }
      .avatar-chevron {
        color: var(--text-muted);
        transition: transform 0.12s ease;
      }
      .avatar-chevron.open {
        transform: rotate(180deg);
      }
      .avatar-orb {
        display: inline-block;
        width: 32px;
        height: 32px;
        border-radius: 50%;
        background: conic-gradient(
          from 180deg at 50% 50%,
          #6ee7b7,
          #a78bfa,
          #f472b6,
          #fbbf24,
          #6ee7b7
        );
        cursor: pointer;
        border: 1px solid var(--border-strong);
      }
      .avatar-orb.small {
        width: 28px;
        height: 28px;
      }
      .avatar-pop {
        display: none;
        position: absolute;
        right: 0;
        top: 52px;
        background: var(--surface);
        border: 1px solid var(--border-strong);
        border-radius: var(--radius);
        padding: 14px;
        min-width: 300px;
        box-shadow: var(--shadow-pop);
        z-index: 40;
      }
      .avatar-pop.open {
        display: grid;
        gap: 12px;
      }
      .avatar-pop-head {
        display: flex;
        gap: 10px;
        align-items: center;
        padding-bottom: 10px;
        border-bottom: 1px solid var(--border);
      }
      .avatar-pop-head strong {
        display: block;
        font-size: 14px;
      }
      .avatar-addr {
        font-family: var(--font-plex-mono), ui-monospace, monospace;
        font-size: 12px;
        color: var(--text-muted);
      }
      .avatar-balance-line {
        display: flex;
        align-items: baseline;
        justify-content: space-between;
        gap: 12px;
        font-size: 12px;
        color: var(--text-muted);
      }
      .avatar-balance-line strong {
        color: var(--text);
        font-size: 13px;
      }
      .avatar-pop-nav {
        display: grid;
        gap: 8px;
        padding: 4px 0;
      }
      .avatar-menu-item {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 10px;
        width: 100%;
        background: transparent;
        border: 1px solid var(--border);
        border-radius: 14px;
        color: var(--text-dim);
        padding: 11px 12px;
        font-size: 13px;
        font-weight: 600;
        cursor: pointer;
        text-align: left;
      }
      .avatar-menu-item:hover {
        background: var(--surface-2);
        color: var(--text);
      }
      .avatar-menu-item.active {
        background: var(--accent-soft);
        border-color: rgba(41, 98, 255, 0.28);
        color: #aecdff;
      }
      .avatar-pop-actions {
        display: flex;
        gap: 8px;
        flex-wrap: wrap;
      }

      /* ===== Category Nav (Polymarket-style text tabs) ===== */
      .category-nav {
        position: sticky;
        top: 65px;
        z-index: 25;
        background: rgba(13, 14, 18, 0.85);
        backdrop-filter: blur(12px);
        border-bottom: 1px solid var(--border);
      }
      .category-nav-inner {
        max-width: 1280px;
        margin: 0 auto;
        padding: 6px 24px;
        display: flex;
        gap: 4px;
        overflow-x: auto;
        scrollbar-width: none;
      }
      .category-nav-inner::-webkit-scrollbar { display: none; }
      .category-tab {
        all: unset;
        cursor: pointer;
        display: inline-flex;
        align-items: center;
        gap: 6px;
        padding: 12px 14px;
        font-size: 14px;
        font-weight: 500;
        color: var(--text-muted);
        white-space: nowrap;
        position: relative;
        border-bottom: 2px solid transparent;
        transition: color 0.12s ease;
      }
      .category-tab:hover {
        color: var(--text);
      }
      .category-tab.active {
        color: var(--text);
        font-weight: 600;
        border-bottom-color: var(--accent);
      }
      .category-tab .category-icon {
        font-size: 14px;
        opacity: 0.85;
      }
      .category-tab.active .category-icon {
        opacity: 1;
        color: var(--warning);
      }
      .category-count {
        background: var(--surface-3);
        color: var(--text-muted);
        font-size: 11px;
        padding: 1px 7px;
        border-radius: var(--radius-pill);
        font-weight: 600;
      }
      .category-tab.active .category-count {
        background: var(--accent-soft);
        color: var(--accent);
      }

      /* ===== Hero band ===== */
      .hero-band {
        display: grid;
        grid-template-columns: 1fr 360px;
        gap: 22px;
        align-items: stretch;
        background: var(--surface);
        border: 1px solid var(--border);
        border-radius: var(--radius-lg);
        padding: 28px;
        position: relative;
        overflow: hidden;
      }
      .hero-band::before {
        content: "";
        position: absolute;
        top: -40%;
        left: -10%;
        width: 60%;
        height: 180%;
        background: radial-gradient(
          ellipse at center,
          rgba(41, 98, 255, 0.16),
          transparent 60%
        );
        pointer-events: none;
      }
      .hero-text {
        position: relative;
        z-index: 1;
      }
      .hero-text h1 {
        margin: 0;
        font-size: clamp(28px, 4vw, 40px);
        letter-spacing: -0.03em;
        line-height: 1.05;
      }
      .hero-text p {
        margin: 12px 0 20px;
        color: var(--text-dim);
        font-size: 15px;
        max-width: 560px;
      }
      .hero-ctas {
        display: inline-flex;
        gap: 8px;
        flex-wrap: wrap;
      }
      .hero-trust {
        margin: 22px 0 0;
        padding: 0;
        list-style: none;
        display: inline-flex;
        gap: 6px;
        flex-wrap: wrap;
      }
      .hero-trust li {
        padding: 5px 10px;
        background: var(--surface-2);
        border: 1px solid var(--border);
        border-radius: var(--radius-pill);
        font-size: 11px;
        color: var(--text-muted);
        font-weight: 600;
      }
      .hero-card {
        background: var(--surface-2);
        border: 1px solid var(--border);
        border-radius: var(--radius);
        padding: 18px;
        display: grid;
        gap: 10px;
        align-content: start;
        position: relative;
        z-index: 1;
      }
      .hero-card h3 {
        margin: 0;
        font-size: 18px;
      }

      /* ===== Section ===== */
      .section {
        display: grid;
        gap: 14px;
      }
      .section-head {
        display: flex;
        align-items: baseline;
        justify-content: space-between;
        gap: 12px;
      }
      .section-head h2 {
        margin: 0;
        font-size: 20px;
        font-weight: 700;
        letter-spacing: -0.01em;
      }
      .portfolio-head {
        align-items: flex-start;
      }
      .portfolio-head-sub {
        margin: 6px 0 0;
      }
      .portfolio-stat-grid {
        display: grid;
        grid-template-columns: repeat(3, minmax(0, 1fr));
        gap: 14px;
      }
      .portfolio-stat-card {
        background: var(--surface);
        border: 1px solid var(--border);
        border-radius: var(--radius);
        padding: 18px;
        display: grid;
        gap: 8px;
      }
      .portfolio-stat-card span {
        color: var(--text-muted);
        font-size: 11px;
        text-transform: uppercase;
        letter-spacing: 0.08em;
        font-weight: 600;
      }
      .portfolio-stat-card strong {
        font-size: 26px;
        line-height: 1;
        letter-spacing: -0.03em;
      }
      .portfolio-stat-card small {
        color: var(--text-muted);
        font-size: 12px;
        line-height: 1.45;
      }
      .portfolio-stat-card .stat-pos { color: var(--success); }
      .portfolio-stat-card .stat-neg { color: var(--danger); }
      .portfolio-card-grid {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
        gap: 14px;
      }
      .portfolio-card {
        background: var(--surface);
        border: 1px solid var(--border);
        border-radius: var(--radius);
        padding: 18px;
        display: grid;
        gap: 14px;
      }
      .portfolio-card-top {
        display: flex;
        justify-content: space-between;
        gap: 14px;
        align-items: flex-start;
      }
      .portfolio-card-kicker {
        margin: 0 0 6px;
        color: var(--text-muted);
        font-size: 11px;
        letter-spacing: 0.08em;
        text-transform: uppercase;
        font-weight: 700;
      }
      .portfolio-card h3 {
        margin: 0;
        font-size: 20px;
        line-height: 1.1;
        letter-spacing: -0.02em;
      }
      .portfolio-card-sub {
        margin: 6px 0 0;
        color: var(--text-muted);
        font-size: 13px;
      }
      .portfolio-pill-row,
      .portfolio-leg-row {
        display: flex;
        gap: 8px;
        flex-wrap: wrap;
      }
      .portfolio-pill,
      .portfolio-leg-pill {
        padding: 4px 10px;
        border-radius: var(--radius-pill);
        border: 1px solid var(--border);
        background: var(--surface-2);
        color: var(--text-dim);
        font-size: 11px;
        font-weight: 600;
      }
      .portfolio-pill.subtle {
        color: var(--text-muted);
      }
      .portfolio-card-metrics {
        display: grid;
        grid-template-columns: repeat(2, minmax(0, 1fr));
        gap: 10px;
        padding: 14px;
        border-radius: 16px;
        background: var(--surface-2);
      }
      .portfolio-card-metrics span,
      .portfolio-card-foot span,
      .portfolio-trade-metric span {
        display: block;
        color: var(--text-muted);
        font-size: 10.5px;
        text-transform: uppercase;
        letter-spacing: 0.08em;
        font-weight: 600;
      }
      .portfolio-card-metrics strong,
      .portfolio-card-foot strong,
      .portfolio-trade-metric strong {
        display: block;
        margin-top: 4px;
        font-size: 15px;
        color: var(--text);
      }
      .portfolio-card-metrics .metric-pos {
        color: var(--success);
      }
      .portfolio-card-foot {
        display: grid;
        grid-template-columns: 1fr 1fr;
        gap: 12px;
        align-items: end;
      }
      .portfolio-card-actions {
        display: flex;
        gap: 8px;
        flex-wrap: wrap;
        justify-content: flex-end;
      }
      .portfolio-trades-panel {
        display: grid;
        gap: 14px;
      }
      .portfolio-trade-list {
        display: grid;
        gap: 10px;
      }
      .portfolio-trade-row {
        display: grid;
        grid-template-columns: 1.2fr repeat(3, minmax(0, 1fr));
        gap: 12px;
        align-items: center;
        background: var(--surface);
        border: 1px solid var(--border);
        border-radius: 18px;
        padding: 16px 18px;
      }
      .portfolio-trade-main {
        display: grid;
        gap: 4px;
      }
      .portfolio-trade-main strong {
        font-size: 15px;
      }
      .portfolio-trade-main span {
        color: var(--text-muted);
        font-size: 12px;
      }

      /* ===== Strategy Grid (Polymarket cards) ===== */
      .strategy-grid {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
        gap: 14px;
      }
      .strategy-card {
        background: var(--surface);
        border: 1px solid var(--border);
        border-radius: var(--radius);
        padding: 16px;
        display: grid;
        gap: 12px;
        transition:
          background 0.12s ease,
          border-color 0.12s ease;
        position: relative;
      }
      .strategy-card:hover {
        background: var(--surface-2);
        border-color: var(--border-strong);
      }
      .strategy-card.active {
        border-color: var(--accent);
        box-shadow: 0 0 0 2px var(--accent-soft);
      }
      .strategy-card-head {
        display: flex;
        justify-content: space-between;
        align-items: flex-start;
        gap: 10px;
      }
      .strategy-card-id {
        display: flex;
        gap: 10px;
        align-items: flex-start;
      }
      .strategy-card-glyph {
        width: 36px;
        height: 36px;
        border-radius: 8px;
        background: var(--surface-3);
        color: var(--text);
        display: grid;
        place-items: center;
        font-weight: 800;
        font-size: 16px;
        flex: 0 0 36px;
      }
      .strategy-card.accent-blue .strategy-card-glyph {
        background: linear-gradient(135deg, #1e6fa6, #2962ff);
        color: white;
      }
      .strategy-card.accent-violet .strategy-card-glyph {
        background: linear-gradient(135deg, #6e3afe, #a78bfa);
        color: white;
      }
      .strategy-card.accent-emerald .strategy-card-glyph {
        background: linear-gradient(135deg, #0d8f53, #00d27a);
        color: white;
      }
      .strategy-card.accent-amber .strategy-card-glyph {
        background: linear-gradient(135deg, #c2410c, #f97316);
        color: white;
      }
      .strategy-card-title {
        margin: 0;
        font-size: 15px;
        font-weight: 700;
        line-height: 1.25;
      }
      .strategy-card-desc {
        margin: 2px 0 0;
        color: var(--text-muted);
        font-size: 12.5px;
        line-height: 1.4;
      }
      .strategy-card-tags {
        display: flex;
        gap: 6px;
        flex-wrap: wrap;
      }
      .strategy-card-tag {
        font-size: 11px;
        color: var(--text-muted);
        background: var(--surface-2);
        border: 1px solid var(--border);
        padding: 2px 8px;
        border-radius: var(--radius-pill);
        font-weight: 500;
      }
      .strategy-card-meta {
        display: grid;
        grid-template-columns: 1fr 1fr;
        gap: 12px;
        padding: 10px 12px;
        background: var(--bg-2);
        border: 1px solid var(--border);
        border-radius: var(--radius-sm);
      }
      .strategy-meta-label {
        display: block;
        color: var(--text-muted);
        font-size: 10px;
        text-transform: uppercase;
        letter-spacing: 0.06em;
        font-weight: 600;
        margin-bottom: 2px;
      }
      .strategy-meta-value {
        font-size: 12px;
        font-weight: 700;
        color: var(--text);
      }
      .strategy-meta-value.muted-strong {
        color: var(--text-dim);
        font-weight: 600;
      }
      .strategy-card-actions {
        display: grid;
      }

      /* Polymarket-style Yes / No / Disabled buttons */
      .poly-button {
        all: unset;
        cursor: pointer;
        text-align: center;
        padding: 9px 14px;
        border-radius: var(--radius-sm);
        font-size: 13px;
        font-weight: 700;
        transition: background 0.12s ease, color 0.12s ease;
      }
      .poly-yes {
        background: var(--success-soft);
        color: var(--success);
        border: 1px solid rgba(0, 210, 122, 0.0);
      }
      .poly-yes:hover {
        background: var(--success);
        color: white;
      }
      .poly-no {
        background: var(--danger-soft);
        color: var(--danger);
      }
      .poly-no:hover {
        background: var(--danger);
        color: white;
      }
      .poly-disabled {
        background: var(--surface-2);
        color: var(--text-muted);
        cursor: not-allowed;
      }
      .poly-beta {
        background: var(--accent-soft);
        color: var(--accent);
        border: 1px solid rgba(41, 98, 255, 0.32);
      }
      .poly-beta:hover {
        background: var(--accent);
        color: white;
      }

      .strategy-card.filler {
        cursor: default;
        border: 1px dashed var(--border-strong);
        background: transparent;
        opacity: 0.75;
        align-content: center;
        text-align: center;
        min-height: 200px;
      }
      .strategy-card.filler:hover {
        background: transparent;
      }
      .filler-inner { display: grid; gap: 6px; }
      .filler-eyebrow {
        margin: 0;
        font-size: 10px;
        color: var(--text-muted);
        text-transform: uppercase;
        letter-spacing: 0.1em;
      }
      .filler-text { margin: 0; color: var(--text-dim); font-size: 13px; }

      /* ===== Status Pill ===== */
      .status-pill {
        display: inline-flex;
        align-items: center;
        gap: 6px;
        padding: 3px 9px;
        border-radius: var(--radius-pill);
        font-size: 10px;
        font-weight: 700;
        text-transform: uppercase;
        letter-spacing: 0.08em;
        background: var(--surface-2);
        color: var(--text-muted);
        border: 1px solid var(--border);
        flex: 0 0 auto;
      }
      .status-pill .status-dot {
        width: 6px;
        height: 6px;
        border-radius: 50%;
        background: currentColor;
        animation: pulse-dot 1.6s ease-in-out infinite;
      }
      @keyframes pulse-dot {
        0%, 100% { opacity: 1; }
        50% { opacity: 0.55; }
      }
      .status-pill.tone-live {
        color: var(--success);
        background: var(--success-soft);
        border-color: transparent;
      }
      .status-pill.tone-soon {
        color: var(--text-muted);
      }
      .status-pill.tone-ok {
        color: var(--success);
        background: var(--success-soft);
        border-color: transparent;
      }
      .status-pill.tone-warn {
        color: var(--warning);
        background: var(--warning-soft);
        border-color: transparent;
      }
      .status-pill.tone-danger {
        color: var(--danger);
        background: var(--danger-soft);
        border-color: transparent;
      }

      /* ===== Buttons ===== */
      .primary-button,
      .secondary-button,
      .ghost-button,
      .mini-button {
        cursor: pointer;
        font-weight: 600;
        border-radius: var(--radius);
        transition:
          background 0.12s ease,
          border-color 0.12s ease,
          opacity 0.12s ease;
      }
      .primary-button {
        background: var(--accent);
        color: white;
        border: 0;
        padding: 11px 14px;
        font-size: 14px;
        width: 100%;
      }
      .primary-button.compact {
        width: auto;
        padding: 10px 16px;
        font-size: 13px;
      }
      .primary-button:hover:not(:disabled) {
        background: var(--accent-hover);
      }
      .primary-button:disabled,
      .secondary-button:disabled,
      .ghost-button:disabled,
      .mini-button:disabled {
        opacity: 0.55;
        cursor: not-allowed;
      }
      .secondary-button {
        background: var(--surface-2);
        color: var(--text);
        border: 1px solid var(--border-strong);
        padding: 10px 14px;
        font-size: 13px;
        width: 100%;
      }
      .secondary-button:hover:not(:disabled) {
        background: var(--surface-3);
      }
      .ghost-button {
        background: transparent;
        color: var(--text-dim);
        border: 1px solid var(--border-strong);
        padding: 8px 12px;
        font-size: 12px;
      }
      .ghost-button:hover:not(:disabled) {
        color: var(--text);
        background: var(--surface-2);
      }
      .mini-button {
        background: var(--surface-2);
        color: var(--text-dim);
        border: 1px solid var(--border);
        padding: 6px 10px;
        font-size: 11px;
        border-radius: var(--radius-sm);
      }
      .mini-button:hover:not(:disabled) {
        color: var(--text);
        border-color: var(--border-strong);
      }
      .copy-button {
        all: unset;
        cursor: pointer;
        display: inline-flex;
        align-items: center;
        gap: 4px;
        color: var(--text-muted);
        background: var(--surface-2);
        border: 1px solid var(--border);
        padding: 4px 8px;
        border-radius: var(--radius-sm);
        font-size: 11px;
      }
      .copy-button.tight { padding: 4px; }
      .copy-button:hover {
        color: var(--text);
        border-color: var(--border-strong);
        background: var(--surface-3);
      }
      .link-button {
        text-decoration: none;
        display: inline-flex;
        align-items: center;
        gap: 4px;
        font-size: 12px;
        color: var(--accent);
        background: var(--accent-soft);
        border: 1px solid transparent;
        padding: 4px 10px;
        border-radius: var(--radius-sm);
        font-weight: 600;
      }
      .link-button:hover {
        color: white;
        background: var(--accent);
      }
      .advanced-toggle {
        all: unset;
        cursor: pointer;
        color: var(--text-muted);
        font-size: 12px;
        text-decoration: underline;
        text-underline-offset: 2px;
      }
      .advanced-toggle:hover { color: var(--text); }

      /* ===== Panel ===== */
      .panel {
        background: var(--surface);
        border: 1px solid var(--border);
        border-radius: var(--radius);
        padding: 18px;
        box-shadow: var(--shadow-card);
      }
      .panel-header {
        display: grid;
        gap: 4px;
        margin-bottom: 14px;
      }
      .panel-header h2 {
        margin: 0;
        font-size: 16px;
        font-weight: 700;
      }
      .eyebrow {
        margin: 0;
        font-size: 10px;
        color: var(--accent);
        letter-spacing: 0.14em;
        text-transform: uppercase;
        font-weight: 800;
      }
      .muted {
        color: var(--text-muted);
        font-size: 12px;
        line-height: 1.5;
        margin: 0;
      }
      .hint {
        color: var(--text-muted);
        font-size: 12px;
        margin: 6px 0 0;
      }

      /* ===== Builder ticket ===== */
      .builder-layout {
        display: grid;
        grid-template-columns: 380px minmax(0, 1fr);
        gap: 20px;
        align-items: start;
      }
      .ticket {
        position: sticky;
        top: 124px;
        display: grid;
        gap: 14px;
      }
      .ticket-head h2 {
        margin: 4px 0 4px;
      }
      .ticket-row { display: grid; gap: 14px; }
      .field { display: grid; gap: 6px; }
      .field-label {
        font-size: 11px;
        color: var(--text-muted);
        text-transform: uppercase;
        letter-spacing: 0.08em;
        font-weight: 700;
      }
      .field-help {
        font-size: 11px;
        color: var(--text-faint);
      }
      .field-help.danger,
      .muted.danger {
        color: var(--danger);
      }
      input, select {
        width: 100%;
        background: var(--bg-2);
        color: var(--text);
        border: 1px solid var(--border);
        border-radius: var(--radius-sm);
        padding: 11px 12px;
        outline: none;
        font-size: 13px;
      }
      input:focus, select:focus {
        border-color: var(--accent);
        box-shadow: 0 0 0 3px var(--accent-soft);
      }
      .input-suffix {
        display: flex;
        align-items: center;
        background: var(--bg-2);
        border: 1px solid var(--border);
        border-radius: var(--radius-sm);
        overflow: hidden;
      }
      .input-suffix:focus-within {
        border-color: var(--accent);
        box-shadow: 0 0 0 3px var(--accent-soft);
      }
      .input-suffix input { border: 0; border-radius: 0; background: transparent; }
      .input-suffix span {
        padding: 0 12px;
        color: var(--text-muted);
        font-size: 12px;
        white-space: nowrap;
      }
      .readonly-row {
        display: flex;
        gap: 8px;
        align-items: center;
        background: var(--bg-2);
        border: 1px solid var(--border);
        border-radius: var(--radius-sm);
        padding: 9px 10px;
      }
      .readonly-row code { flex: 1; font-size: 13px; }
      .balance-pill {
        display: flex;
        justify-content: space-between;
        align-items: center;
        gap: 10px;
        background: var(--bg-2);
        border: 1px solid var(--border);
        border-radius: var(--radius-sm);
        padding: 10px 12px;
      }
      .balance-pill span {
        display: block;
        color: var(--text-muted);
        font-size: 10px;
        text-transform: uppercase;
        letter-spacing: 0.08em;
        margin-bottom: 2px;
      }
      .balance-pill strong {
        color: var(--text);
        font-size: 14px;
      }
      .balance-actions {
        display: flex;
        gap: 8px;
        align-items: center;
        flex-wrap: wrap;
        justify-content: flex-end;
      }
      .trust-row {
        margin: 0;
        display: flex;
        gap: 6px;
        flex-wrap: wrap;
        color: var(--text-muted);
        font-size: 10px;
      }
      .trust-row span {
        background: var(--bg-2);
        border: 1px solid var(--border);
        padding: 3px 8px;
        border-radius: var(--radius-pill);
        text-transform: uppercase;
        letter-spacing: 0.08em;
      }

      /* ===== Segmented control ===== */
      .seg-wrap { display: grid; gap: 6px; }
      .seg {
        display: grid;
        grid-template-columns: repeat(3, 1fr);
        background: var(--bg-2);
        border: 1px solid var(--border);
        border-radius: var(--radius-sm);
        padding: 3px;
        gap: 3px;
      }
      .seg-item {
        all: unset;
        cursor: pointer;
        text-align: center;
        padding: 9px 8px;
        font-size: 12px;
        font-weight: 600;
        color: var(--text-muted);
        border-radius: 6px;
      }
      .seg-item:hover { color: var(--text); }
      .seg-item.active {
        background: var(--surface-3);
        color: var(--text);
      }
      .seg-help {
        margin: 0;
        font-size: 11px;
        color: var(--text-faint);
      }

      /* ===== Preview column ===== */
      .preview-column {
        display: grid;
        gap: 16px;
        min-width: 0;
      }

      /* ===== Stats / meta ===== */
      .stats-grid {
        display: grid;
        grid-template-columns: repeat(4, minmax(0, 1fr));
        gap: 10px;
      }
      .stat {
        background: var(--bg-2);
        border: 1px solid var(--border);
        border-radius: var(--radius-sm);
        padding: 12px;
      }
      .stat label {
        display: block;
        color: var(--text-muted);
        font-size: 10px;
        margin-bottom: 6px;
        text-transform: uppercase;
        letter-spacing: 0.08em;
      }
      .stat strong {
        font-size: 16px;
        letter-spacing: -0.01em;
        font-variant-numeric: tabular-nums;
      }
      .stat.tone-success strong { color: var(--success); }
      .stat.tone-danger strong { color: var(--danger); }

      .meta-grid {
        display: grid;
        grid-template-columns: repeat(2, minmax(0, 1fr));
        gap: 10px;
        margin-top: 14px;
      }
      .meta-item {
        background: var(--bg-2);
        border: 1px solid var(--border);
        border-radius: var(--radius-sm);
        padding: 12px;
        overflow-wrap: anywhere;
      }
      .meta-item label {
        display: block;
        color: var(--text-muted);
        font-size: 10px;
        margin-bottom: 4px;
        text-transform: uppercase;
        letter-spacing: 0.08em;
      }
      .meta-copy {
        display: inline-flex;
        align-items: center;
        gap: 8px;
        flex-wrap: wrap;
      }
      .meta-item.span-2 { grid-column: span 2; }

      /* ===== Payoff vis ===== */
      .payoff-vis { display: grid; gap: 6px; margin-top: 8px; }
      .payoff-row {
        display: grid;
        grid-template-columns: repeat(5, minmax(0, 1fr));
        gap: 4px;
      }
      .payoff-cell {
        background: var(--bg-2);
        border: 1px solid var(--border);
        border-radius: var(--radius-sm);
        padding: 14px 10px;
        text-align: left;
        display: grid;
        gap: 4px;
        min-height: 90px;
      }
      .payoff-cell.tone-positive {
        background: linear-gradient(180deg, rgba(0, 210, 122, 0.10), rgba(0, 210, 122, 0.02));
        border-color: rgba(0, 210, 122, 0.32);
      }
      .payoff-cell.tone-negative {
        background: linear-gradient(180deg, rgba(239, 68, 68, 0.10), rgba(239, 68, 68, 0.02));
        border-color: rgba(239, 68, 68, 0.32);
      }
      .payoff-cell.tone-neutral {
        background: linear-gradient(180deg, rgba(249, 115, 22, 0.10), rgba(249, 115, 22, 0.02));
        border-color: rgba(249, 115, 22, 0.32);
      }
      .payoff-label {
        margin: 0;
        font-size: 11px;
        color: var(--text-muted);
        text-transform: uppercase;
        letter-spacing: 0.06em;
        font-weight: 700;
      }
      .payoff-net { margin: 0; font-size: 14px; font-weight: 700; }
      .payoff-cell.tone-positive .payoff-net { color: var(--success); }
      .payoff-cell.tone-negative .payoff-net { color: var(--danger); }
      .payoff-gross { margin: 0; font-size: 11px; color: var(--text-muted); }
      .payoff-axis {
        display: grid;
        grid-template-columns: repeat(6, minmax(0, 1fr));
        gap: 4px;
        font-size: 10px;
        color: var(--text-muted);
        padding-top: 4px;
        border-top: 1px dashed var(--border);
      }
      .payoff-axis span:first-child,
      .payoff-axis span:last-child { color: var(--text-faint); }

      /* ===== Tables ===== */
      .table-wrap { overflow-x: auto; }
      table {
        width: 100%;
        border-collapse: collapse;
        min-width: 560px;
      }
      th, td {
        padding: 10px 12px;
        border-bottom: 1px solid var(--border);
        text-align: left;
        font-size: 13px;
        vertical-align: top;
      }
      th {
        color: var(--text-muted);
        font-weight: 700;
        text-transform: uppercase;
        letter-spacing: 0.08em;
        font-size: 10px;
        background: var(--bg-2);
      }
      td { color: var(--text); }
      td.mono {
        font-family: var(--font-plex-mono), ui-monospace, monospace;
        font-size: 13px;
      }
      td.pnl-up { color: var(--success); }
      td.pnl-down { color: var(--danger); }

      .kind-pill {
        display: inline-flex;
        padding: 3px 8px;
        border-radius: var(--radius-pill);
        background: var(--accent-soft);
        color: var(--accent);
        font-weight: 700;
        font-size: 11px;
      }
      .kind-pill.down { background: var(--danger-soft); color: var(--danger); }
      .kind-pill.up { background: var(--success-soft); color: var(--success); }
      .kind-pill.range { background: var(--violet-soft); color: var(--violet); }
      .kind-pill.subtle { background: var(--surface-3); color: var(--text-dim); }

      /* ===== Warnings ===== */
      .warnings { display: grid; gap: 8px; }
      .warning-item {
        display: grid;
        grid-template-columns: 80px 1fr;
        gap: 10px;
        align-items: start;
        border-radius: var(--radius-sm);
        padding: 10px 12px;
        line-height: 1.45;
        font-size: 13px;
        border: 1px solid;
      }
      .warning-item.severity-info {
        border-color: rgba(41, 98, 255, 0.3);
        background: var(--accent-soft);
        color: var(--text-dim);
      }
      .warning-item.severity-caution {
        border-color: rgba(249, 115, 22, 0.32);
        background: var(--warning-soft);
        color: #fed7aa;
      }
      .warning-item.severity-blocking {
        border-color: rgba(239, 68, 68, 0.4);
        background: var(--danger-soft);
        color: #fecaca;
      }
      .warning-item p { margin: 0; }
      .warning-tag {
        display: inline-flex;
        padding: 3px 8px;
        border-radius: var(--radius-pill);
        background: rgba(0, 0, 0, 0.25);
        color: inherit;
        font-weight: 800;
        font-size: 9px;
        text-transform: uppercase;
        letter-spacing: 0.08em;
        align-self: start;
      }

      /* ===== Stepper ===== */
      .stepper {
        list-style: none;
        margin: 0 0 14px;
        padding: 14px;
        background: var(--bg-2);
        border: 1px solid var(--border);
        border-radius: var(--radius-sm);
        display: grid;
        grid-template-columns: repeat(7, 1fr);
        gap: 0;
        position: relative;
      }
      .stepper-step {
        display: flex;
        flex-direction: column;
        align-items: center;
        gap: 6px;
        position: relative;
        text-align: center;
      }
      .stepper-bubble {
        width: 24px;
        height: 24px;
        border-radius: 50%;
        background: var(--surface-3);
        color: var(--text-muted);
        display: grid;
        place-items: center;
        font-size: 11px;
        font-weight: 800;
        border: 1px solid var(--border);
        position: relative;
        z-index: 1;
      }
      .stepper-text strong {
        display: block;
        font-size: 11px;
        color: var(--text-dim);
      }
      .stepper-text span {
        display: block;
        font-size: 10px;
        color: var(--text-muted);
      }
      .stepper-step.active .stepper-bubble {
        color: var(--accent);
        border-color: var(--accent);
        background: var(--accent-soft);
      }
      .stepper-step.success .stepper-bubble {
        color: white;
        background: var(--success);
        border-color: var(--success);
      }
      .stepper-step.failed .stepper-bubble {
        color: white;
        background: var(--danger);
        border-color: var(--danger);
      }
      .stepper-step.active .stepper-text strong { color: var(--text); }
      .stepper-bar {
        position: absolute;
        top: 12px;
        left: 50%;
        right: -50%;
        height: 2px;
        background: var(--border);
        z-index: 0;
      }
      .stepper-step.success ~ .stepper-step .stepper-bar,
      .stepper-step.success .stepper-bar {
        background: var(--success);
      }

      /* ===== Preflight / exec actions ===== */
      .exec-split {
        display: grid;
        grid-template-columns: 1fr 280px;
        gap: 16px;
        align-items: start;
      }
      .preflight {
        list-style: none;
        margin: 0;
        padding: 0;
        display: grid;
        gap: 6px;
      }
      .preflight-item {
        display: grid;
        grid-template-columns: 22px 1fr;
        gap: 10px;
        align-items: center;
        padding: 8px 10px;
        background: var(--bg-2);
        border: 1px solid var(--border);
        border-radius: var(--radius-sm);
      }
      .preflight-icon {
        display: grid;
        place-items: center;
        width: 22px;
        height: 22px;
        border-radius: 50%;
        font-weight: 900;
        font-size: 12px;
      }
      .preflight-item.ok .preflight-icon {
        color: var(--success);
        background: var(--success-soft);
      }
      .preflight-item.bad .preflight-icon {
        color: var(--warning);
        background: var(--warning-soft);
      }
      .preflight-body strong { display: block; font-size: 12.5px; }
      .preflight-detail {
        display: block;
        font-size: 11px;
        color: var(--text-muted);
      }
      .exec-actions { display: grid; gap: 8px; }

      /* ===== Error notice ===== */
      .error-notice {
        border-radius: var(--radius-sm);
        padding: 12px 14px;
        display: grid;
        gap: 6px;
        border: 1px solid;
      }
      .error-notice.severity-info {
        border-color: rgba(41, 98, 255, 0.32);
        background: var(--accent-soft);
        color: var(--text-dim);
      }
      .error-notice.severity-caution {
        border-color: rgba(249, 115, 22, 0.32);
        background: var(--warning-soft);
        color: #fed7aa;
      }
      .error-notice.severity-blocking {
        border-color: rgba(239, 68, 68, 0.45);
        background: var(--danger-soft);
        color: #fecaca;
      }
      .error-head {
        display: flex;
        justify-content: space-between;
        gap: 12px;
        align-items: center;
      }
      .severity-pill {
        font-size: 9px;
        text-transform: uppercase;
        letter-spacing: 0.08em;
        padding: 3px 7px;
        border-radius: var(--radius-pill);
        background: rgba(0, 0, 0, 0.3);
      }
      .error-message { margin: 0; font-size: 13px; line-height: 1.5; }
      .error-action { margin: 0; font-size: 12px; opacity: 0.85; }
      .debug-toggle { margin-top: 6px; }
      .debug-pre {
        white-space: pre-wrap;
        overflow-x: auto;
        margin: 6px 0 0;
        padding: 10px;
        font-size: 11px;
        line-height: 1.45;
        color: var(--text-dim);
        background: var(--bg-2);
        border-radius: var(--radius-sm);
        border: 1px solid var(--border);
        max-height: 280px;
        overflow-y: auto;
      }

      /* ===== Receipt ===== */
      .receipt-head {
        display: flex;
        justify-content: space-between;
        gap: 12px;
        align-items: flex-start;
        margin-bottom: 14px;
      }
      .receipt-stats {
        display: grid;
        grid-template-columns: repeat(3, minmax(0, 1fr));
        gap: 10px;
        margin-bottom: 14px;
      }
      .receipt-rows {
        display: grid;
        gap: 6px;
        margin-bottom: 14px;
      }
      .receipt-row {
        display: grid;
        grid-template-columns: 140px 1fr;
        gap: 12px;
        align-items: center;
        background: var(--bg-2);
        border: 1px solid var(--border);
        border-radius: var(--radius-sm);
        padding: 10px 12px;
      }
      .receipt-row-label {
        font-size: 11px;
        text-transform: uppercase;
        letter-spacing: 0.08em;
        color: var(--text-muted);
      }
      .receipt-row-value {
        display: inline-flex;
        align-items: center;
        gap: 8px;
        flex-wrap: wrap;
      }
      .debug-grid { display: grid; gap: 10px; margin-top: 10px; }
      .debug-block {
        background: var(--bg-2);
        border: 1px solid var(--border);
        border-radius: var(--radius-sm);
        padding: 10px;
      }
      .debug-label {
        margin: 0 0 6px;
        font-size: 10px;
        text-transform: uppercase;
        letter-spacing: 0.08em;
        color: var(--text-muted);
      }

      /* ===== Empty state ===== */
      .empty-state-card {
        background: var(--surface);
        border: 1px dashed var(--border-strong);
        border-radius: var(--radius);
        padding: 28px;
        display: grid;
        gap: 8px;
        justify-items: start;
      }
      .empty-state-card h3 { margin: 0; font-size: 16px; }
      .empty-state-card p { margin: 0; color: var(--text-muted); font-size: 13px; }

      /* ===== Skeleton ===== */
      .skeleton {
        background: linear-gradient(
          90deg,
          rgba(148, 163, 184, 0.06) 0%,
          rgba(148, 163, 184, 0.16) 50%,
          rgba(148, 163, 184, 0.06) 100%
        );
        background-size: 400% 100%;
        animation: shimmer 1.2s linear infinite;
        border-radius: var(--radius-sm);
      }
      .skeleton-title { height: 18px; width: 180px; }
      .skeleton-line { height: 12px; width: 100%; margin-top: 6px; }
      .skeleton-line.short { width: 60%; }
      .skeleton-pill { height: 16px; width: 70px; border-radius: var(--radius-pill); }
      .skeleton-card {
        background: var(--surface);
        border: 1px solid var(--border);
        border-radius: var(--radius);
        padding: 18px;
        display: grid;
        gap: 6px;
      }
      @keyframes shimmer {
        0% { background-position: 100% 0; }
        100% { background-position: -100% 0; }
      }

      /* ===== Toast ===== */
      .toast {
        position: fixed;
        bottom: 22px;
        left: 50%;
        transform: translateX(-50%);
        display: inline-flex;
        align-items: center;
        gap: 8px;
        background: var(--surface-3);
        color: var(--text);
        border: 1px solid var(--border-strong);
        border-radius: var(--radius-pill);
        padding: 8px 14px;
        font-size: 12px;
        font-weight: 700;
        box-shadow: var(--shadow-pop);
        z-index: 100;
      }
      .toast-dot {
        width: 6px;
        height: 6px;
        border-radius: 50%;
        background: var(--success);
      }

      .page-foot {
        margin-top: 20px;
        padding-top: 20px;
        border-top: 1px solid var(--border);
      }

      /* ===== Responsive ===== */
      @media (max-width: 1100px) {
        .normal-layout { grid-template-columns: 1fr; }
        .builder-layout { grid-template-columns: 1fr; }
        .ticket { position: static; }
        .stats-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
        .receipt-stats { grid-template-columns: repeat(3, minmax(0, 1fr)); }
        .stepper { grid-template-columns: repeat(7, 1fr); font-size: 10px; }
        .exec-split { grid-template-columns: 1fr; }
        .portfolio-stat-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
        .portfolio-trade-row { grid-template-columns: repeat(2, minmax(0, 1fr)); }
      }
      @media (max-width: 820px) {
        .hero-band { grid-template-columns: 1fr; }
        .meta-grid { grid-template-columns: 1fr; }
        .receipt-stats { grid-template-columns: 1fr; }
        .receipt-row { grid-template-columns: 1fr; gap: 4px; }
        .stepper { grid-template-columns: repeat(4, 1fr); gap: 8px; padding: 12px; }
        .stepper-step:nth-child(n + 5) .stepper-bar { display: none; }
        .portfolio-card-metrics,
        .portfolio-card-foot,
        .portfolio-trade-row {
          grid-template-columns: 1fr;
        }
        .app-header-inner {
          grid-template-columns: auto auto;
          row-gap: 8px;
        }
        .header-right { grid-column: 1 / -1; justify-content: space-between; }
        .search-wrap { grid-column: 1 / -1; }
        .header-stat { display: none; }
        .category-nav { top: 116px; }
        .mode-toggle button { min-width: 0; }
      }
      @media (max-width: 520px) {
        .page-body { padding: 16px 14px 48px; }
        .mode-toggle-wrap { padding: 14px 14px 0; }
        .mode-toggle {
          width: 100%;
          display: grid;
        }
        .stats-grid { grid-template-columns: 1fr; }
        .portfolio-stat-grid,
        .portfolio-card-grid {
          grid-template-columns: 1fr;
        }
        .portfolio-card-actions {
          justify-content: flex-start;
        }
        .avatar-trigger-label { max-width: 78px; }
        .avatar-pop {
          right: -10px;
          min-width: min(320px, calc(100vw - 28px));
        }
        .payoff-row { grid-template-columns: 1fr; }
        .stepper { grid-template-columns: repeat(2, 1fr); }
        .stepper-bar { display: none; }
      }
    `}</style>
  );
}
