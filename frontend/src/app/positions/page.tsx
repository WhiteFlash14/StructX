// /positions — read-only view of the user's open and closed StructX
// positions, sourced from the disk-backed ledger on the backend.
//
// What this page is NOT (yet): live PnL refresh, redeem preview/sign, audit
// history detail. Those land in the next slice. This page renders fast on
// page load (single GET, no Sui RPC) and is the home for "View live
// position" links from the audit success state.

"use client";

import {
  useCurrentAccount,
  useSignAndExecuteTransaction,
  useSuiClient,
  useSuiClientContext,
} from "@mysten/dapp-kit";
import Link from "next/link";
import { useCallback, useEffect, useMemo, useState } from "react";

import {
  LandingFooter,
  LandingHeader,
  LandingStyles,
} from "@/app/_landing-shared";
import {
  auditRedeemPosition,
  invalidateManagerBalance,
  isAbortError,
  listPositions,
  PositionRecord,
  PositionsResponse,
  PositionsSummary,
  syncPositionsFromAudits,
  syncPositionsFromChain,
} from "@/lib/api";
import { fetchOnChainPositionEvents } from "@/lib/positionsChain";
import { formatDusdcDisplay, formatPriceCompact, shortAddress } from "@/lib/format";
import { Skeleton } from "@/components/ui/Skeleton";
import { getStoredManager } from "@/lib/api";
import {
  buildRedeemPositionTransaction,
  PREDICT_MANAGER_TYPE,
  readRedeemPayoutFromEvents,
} from "@/lib/tx";

function formatExpiryCountdown(expiryMs: string): string {
  const exp = Number(expiryMs);
  if (!Number.isFinite(exp) || exp <= 0) return "—";
  const diffMs = exp - Date.now();
  if (diffMs <= 0) return "Expired";
  const days = Math.floor(diffMs / 86_400_000);
  const hours = Math.floor((diffMs % 86_400_000) / 3_600_000);
  if (days > 0) return `${days}d ${hours}h`;
  const minutes = Math.floor((diffMs % 3_600_000) / 60_000);
  return `${hours}h ${minutes}m`;
}

function strikeText(p: PositionRecord): string {
  if (p.kind === "RANGE" && p.lowerRaw && p.upperRaw) {
    return `${formatPriceCompact(p.lowerRaw)} → ${formatPriceCompact(p.upperRaw)}`;
  }
  if (p.strikeRaw) return formatPriceCompact(p.strikeRaw);
  return "—";
}

function formatPreviewDusdc(
  raw: string | null | undefined,
  previewAtUnix: number | null | undefined,
): string {
  if (!previewAtUnix || previewAtUnix <= 0) return "—";
  return formatDusdcDisplay(raw);
}

type LivePreview = {
  payoutRaw: bigint;
  pnlRaw: bigint;
  isSettled: boolean;
  error?: string;
};

function pnlClassname(raw: string | null | undefined): string {
  if (!raw) return "";
  try {
    const v = BigInt(raw);
    if (v > 0n) return "pnl-up";
    if (v < 0n) return "pnl-down";
  } catch {
    // ignore
  }
  return "";
}

export default function PositionsPage() {
  const account = useCurrentAccount();
  const ctx = useSuiClientContext();
  const connectedAddress = account?.address ?? null;
  const isTestnet = ctx.network === "testnet";

  const [managerId, setManagerId] = useState("");
  const [data, setData] = useState<PositionsResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [syncing, setSyncing] = useState(false);

  // Redeem modal: when set, the modal is visible for this position with this
  // quantityBps fraction (10000 = 100%, 5000 = 50%, etc.).
  const [redeemTarget, setRedeemTarget] = useState<{
    position: PositionRecord;
    quantityBps: number;
  } | null>(null);

  // Auto-refreshed devInspect previews keyed by positionId. These overlay
  // the persisted ledger values so every open position always shows a fresh
  // "If you close now" number instead of the stale 0 or em-dash.
  const [livePreviews, setLivePreviews] = useState<Record<string, LivePreview>>({});

  const suiClient = useSuiClient();

  // Auto-discover the PredictManager for this wallet — match the workbench
  // exactly. Look in (1) localStorage (instant, same key shape the workbench
  // writes), (2) the backend JSON store, (3) on-chain owned objects. First
  // non-null wins. Never asks the user to type anything.
  //
  // Unlike the workbench we do NOT auto-prompt to create a manager when
  // none is found — the positions page is read-only; the workbench is the
  // place to mint, and that's where the create-manager signature happens.
  const [discoverPhase, setDiscoverPhase] = useState<
    "idle" | "checking" | "found" | "none"
  >("idle");

  useEffect(() => {
    if (!connectedAddress) {
      setManagerId("");
      setDiscoverPhase("idle");
      return;
    }
    let cancelled = false;
    const network = ctx.network ?? "testnet";
    const cacheKey = `structx:manager:${network}:${connectedAddress.toLowerCase()}`;
    setDiscoverPhase("checking");

    (async () => {
      // Tier 1 — localStorage.
      try {
        const cached =
          typeof window !== "undefined"
            ? window.localStorage.getItem(cacheKey)
            : null;
        if (cached && cached.startsWith("0x")) {
          if (cancelled) return;
          setManagerId(cached);
          setDiscoverPhase("found");
          return;
        }
      } catch {
        // ignore
      }

      // Tier 2 + 3 in parallel: backend store and on-chain owned objects.
      const backendP = getStoredManager(connectedAddress).catch(() => null);
      const chainP = (async () => {
        try {
          const owned = await suiClient.getOwnedObjects({
            owner: connectedAddress,
            filter: { StructType: PREDICT_MANAGER_TYPE },
            options: { showType: true },
          });
          return owned.data?.[0]?.data?.objectId ?? null;
        } catch {
          return null;
        }
      })();

      const backendId = await backendP;
      if (cancelled) return;
      if (backendId && backendId.startsWith("0x")) {
        try {
          window.localStorage.setItem(cacheKey, backendId);
        } catch {
          // ignore
        }
        setManagerId(backendId);
        setDiscoverPhase("found");
        return;
      }
      const chainId = await chainP;
      if (cancelled) return;
      if (chainId && chainId.startsWith("0x")) {
        try {
          window.localStorage.setItem(cacheKey, chainId);
        } catch {
          // ignore
        }
        setManagerId(chainId);
        setDiscoverPhase("found");
        return;
      }
      setManagerId("");
      setDiscoverPhase("none");
    })();

    return () => {
      cancelled = true;
    };
  }, [connectedAddress, ctx.network, suiClient]);

  const refresh = useCallback(
    async (owner: string, id: string) => {
      if (!owner || !id) {
        setData(null);
        return;
      }
      setLoading(true);
      setError(null);
      const controller = new AbortController();
      try {
        const r = await listPositions(
          { owner, managerId: id },
          { signal: controller.signal },
        );
        setData(r);
      } catch (err) {
        if (isAbortError(err)) return;
        setError(err instanceof Error ? err.message : String(err));
      } finally {
        setLoading(false);
      }
    },
    [],
  );

  // Chain sync: walk this wallet's recent transactions, pull every
  // PositionMinted/RangeMinted/PositionRedeemed/RangeRedeemed event for the
  // current manager, and post to the backend. Backend dedups by
  // sourceDigest, so re-running is a no-op. This fills in positions that
  // were minted before the audit-open ledger hook landed.
  const [chainSyncing, setChainSyncing] = useState(false);
  const [chainSyncError, setChainSyncError] = useState<string | null>(null);
  const chainSync = useCallback(
    async (owner: string, id: string): Promise<boolean> => {
      setChainSyncing(true);
      setChainSyncError(null);
      try {
        const { mintedLegs, redeemedLegs } = await fetchOnChainPositionEvents(
          suiClient,
          owner,
          id,
        );
        // Always POST — even with zero legs, backend returns the current
        // ledger snapshot, which is cheap.
        const synced = await syncPositionsFromChain({
          owner,
          managerId: id,
          mintedLegs,
          redeemedLegs,
        });
        setData(synced);
        return synced.positions.length > 0;
      } catch (err) {
        setChainSyncError(err instanceof Error ? err.message : String(err));
        return false;
      } finally {
        setChainSyncing(false);
      }
    },
    [suiClient],
  );

  // First load: pull ledger, then ALWAYS attempt a chain sync. If the
  // ledger was empty we definitely need it; if not, sync still catches up
  // anything opened from another device or before the hook landed.
  useEffect(() => {
    if (!connectedAddress || !managerId) return;
    let cancelled = false;
    (async () => {
      await refresh(connectedAddress, managerId);
      if (cancelled) return;
      await chainSync(connectedAddress, managerId);
    })();
    return () => {
      cancelled = true;
    };
  }, [connectedAddress, managerId, refresh, chainSync]);

  // Auto-refresh valuations: for every open position in the current ledger,
  // build a 100% redeem PTB, devInspect it (no signature, no on-chain state
  // change), pull payout from the redeem event, compute the pro-rata pnl.
  //
  // Why client-side: devInspect needs an authenticated sender. Routing
  // through the backend would mean reaching the user's RPC from the server
  // (extra trust + complexity). The frontend already has dApp Kit's
  // SuiClient, so the round-trip stays in one hop.
  //
  // Concurrency: capped at 4 in-flight requests so we don't hammer the
  // RPC node on wallets with lots of positions. Each finished request
  // updates state independently so the UI fills in progressively.
  useEffect(() => {
    if (!connectedAddress || !data) return;
    const open = data.positions.filter(
      (p) => p.status === "open" && BigInt(p.remainingQuantityRaw || "0") > 0n,
    );
    if (open.length === 0) return;
    const controller = new AbortController();

    const buildTxFor = (p: PositionRecord) => {
      const common = {
        owner: connectedAddress,
        managerId,
        oracleId: p.oracleId,
        expiryMs: p.expiryMs,
        predictObjectId:
          "0xc8736204d12f0a7277c86388a68bf8a194b0a14c5538ad13f22cbd8e2a38028a",
        clockObjectId: "0x6",
        quantityRaw: p.remainingQuantityRaw,
      };
      if (p.kind === "RANGE") {
        return buildRedeemPositionTransaction({
          ...common,
          kind: "RANGE",
          lowerRaw: p.lowerRaw ?? "",
          upperRaw: p.upperRaw ?? "",
        });
      }
      return buildRedeemPositionTransaction({
        ...common,
        kind: p.kind,
        strikeRaw: p.strikeRaw ?? "",
      });
    };

    const previewOne = async (p: PositionRecord) => {
      if (controller.signal.aborted) return;
      try {
        const tx = buildTxFor(p);
        const r = await suiClient.devInspectTransactionBlock({
          sender: connectedAddress,
          transactionBlock: tx,
        });
        if (controller.signal.aborted) return;
        if (r.effects?.status?.status !== "success") {
          setLivePreviews((prev) => ({
            ...prev,
            [p.positionId]: {
              payoutRaw: 0n,
              pnlRaw: 0n,
              isSettled: false,
              error: r.effects?.status?.error ?? "Preview unavailable",
            },
          }));
          return;
        }
        const { payoutRaw, isSettled } = readRedeemPayoutFromEvents(
          (r.events ?? []) as Array<{ type?: string; parsedJson?: unknown }>,
        );
        let pnlRaw = 0n;
        try {
          const premium = BigInt(p.premiumPaidRaw || "0");
          const original = BigInt(p.originalQuantityRaw || "0");
          const remaining = BigInt(p.remainingQuantityRaw || "0");
          const basis =
            original === 0n ? 0n : (premium * remaining) / original;
          pnlRaw = payoutRaw - basis;
        } catch {
          // ignore
        }
        setLivePreviews((prev) => ({
          ...prev,
          [p.positionId]: { payoutRaw, pnlRaw, isSettled },
        }));
      } catch (err) {
        if (controller.signal.aborted) return;
        setLivePreviews((prev) => ({
          ...prev,
          [p.positionId]: {
            payoutRaw: 0n,
            pnlRaw: 0n,
            isSettled: false,
            error: err instanceof Error ? err.message : String(err),
          },
        }));
      }
    };

    // Simple concurrency limiter: workers pull from a queue.
    const queue = [...open];
    const CONCURRENCY = 4;
    const workers = Array.from({ length: Math.min(CONCURRENCY, queue.length) }, async () => {
      while (queue.length > 0 && !controller.signal.aborted) {
        const next = queue.shift();
        if (!next) break;
        await previewOne(next);
      }
    });
    void Promise.all(workers);

    return () => controller.abort();
  }, [connectedAddress, managerId, data, suiClient]);

  const onSync = useCallback(async () => {
    if (!connectedAddress || !managerId) return;
    setSyncing(true);
    setError(null);
    try {
      const r = await syncPositionsFromAudits({
        owner: connectedAddress,
        managerId,
      });
      setData(r);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSyncing(false);
    }
  }, [connectedAddress, managerId]);

  // Compute a fresh summary that overlays livePreviews on top of the disk
  // ledger. The backend summary uses persisted lastPreview* values, which
  // are 0 for positions that have never been previewed; we always prefer
  // the live devInspect numbers when available so the top-of-page cards
  // match what's in each row.
  const liveSummary = useMemo<PositionsSummary | null>(() => {
    if (!data) return null;
    let totalEstimated = 0n;
    let totalUnrealized = 0n;
    for (const p of data.positions) {
      if (p.status !== "open") continue;
      const live = livePreviews[p.positionId];
      if (live && !live.error) {
        totalEstimated += live.payoutRaw;
        totalUnrealized += live.pnlRaw;
      } else {
        try {
          totalEstimated += BigInt(p.lastPreviewPayoutRaw || "0");
          totalUnrealized += BigInt(p.lastPreviewPnlRaw || "0");
        } catch {
          // ignore
        }
      }
    }
    return {
      ...data.summary,
      totalEstimatedRedeemRaw: totalEstimated.toString(),
      totalUnrealizedPnlRaw: totalUnrealized.toString(),
    };
  }, [data, livePreviews]);

  // "We have valuations" means at least one open position has *either* a
  // live preview or a persisted preview. While we're still fetching, show
  // the friendlier "Checking…" copy in the summary cards.
  const hasPreviewValuations = useMemo(() => {
    if (!data) return false;
    return data.positions.some(
      (p) =>
        p.status === "open" &&
        (livePreviews[p.positionId] !== undefined ||
          Number(p.lastPreviewAtUnix) > 0),
    );
  }, [data, livePreviews]);

  return (
    <main className="landing">
      <LandingHeader />
      <section className="positions-shell">
        <header className="positions-head">
          <div>
            <p className="strategies-eyebrow">
              <span className="strategies-eyebrow-dot" aria-hidden />
              Your positions
            </p>
            <h1>What you have open</h1>
            <p className="positions-sub">
              Every position you opened from a strategy, with how much it
              paid in, what it&apos;s worth right now, and a one-click close.
            </p>
          </div>
          <div className="positions-actions">
            <button
              type="button"
              onClick={() => connectedAddress && void refresh(connectedAddress, managerId)}
              disabled={loading || !connectedAddress || !managerId}
              className="positions-btn"
            >
              {loading ? "Refreshing…" : "Refresh"}
            </button>
            <button
              type="button"
              onClick={() =>
                connectedAddress && void chainSync(connectedAddress, managerId)
              }
              disabled={chainSyncing || !connectedAddress || !managerId}
              className="positions-btn"
              title="Walk on-chain mint/redeem events for this wallet and apply them to the ledger."
            >
              {chainSyncing ? "Scanning chain…" : "Sync from chain"}
            </button>
            <button
              type="button"
              onClick={() => void onSync()}
              disabled={syncing || !connectedAddress || !managerId}
              className="positions-btn"
            >
              {syncing ? "Syncing…" : "Sync from audits"}
            </button>
          </div>
        </header>

        {error && <p className="positions-error">{error}</p>}
        {chainSyncError && (
          <p className="positions-error">Chain sync: {chainSyncError}</p>
        )}

        {data && liveSummary && (
          <SummaryCards
            summary={liveSummary}
            hasPreviewValuations={hasPreviewValuations}
          />
        )}

        {!data && (loading || chainSyncing) && <PositionsSummarySkeleton />}

        {data && (
          <>
            <OpenPositions
              positions={data.positions.filter((p) => p.status === "open")}
              livePreviews={livePreviews}
              onPreviewClose={(position, quantityBps) =>
                setRedeemTarget({ position, quantityBps })
              }
            />
            <ClosedPositions
              positions={data.positions.filter((p) => p.status === "closed")}
            />
          </>
        )}

        {redeemTarget && connectedAddress && managerId && (
          <RedeemModal
            owner={connectedAddress}
            managerId={managerId}
            position={redeemTarget.position}
            quantityBps={redeemTarget.quantityBps}
            onClose={() => setRedeemTarget(null)}
            onRedeemed={() => {
              setRedeemTarget(null);
              invalidateManagerBalance(managerId);
              void refresh(connectedAddress, managerId);
            }}
          />
        )}

        {!data && !loading && (
          <div className="positions-empty">
            <h3>No positions yet</h3>
            <p>
              Open a strategy from the{" "}
              <Link href="/strategies">strategy library</Link>. Once your
              wallet signs and the audit accepts, your position will appear
              here.
            </p>
          </div>
        )}
      </section>
      <LandingFooter />
      <LandingStyles />
      <style>{POSITIONS_CSS}</style>
    </main>
  );
}

/**
 * Skeleton replacement for the summary cards row while the initial
 * /api/positions request (or the on-chain sync) is in flight. Six cells
 * matching the live SummaryCards grid so the layout doesn't reflow when
 * data arrives.
 */
function PositionsSummarySkeleton() {
  return (
    <div className="positions-summary">
      {Array.from({ length: 6 }).map((_, i) => (
        <div key={i} className="positions-card ui-skel-card">
          <Skeleton width={92} height={10} />
          <Skeleton width={110} height={20} style={{ marginTop: 8 }} />
        </div>
      ))}
    </div>
  );
}

function SummaryCards({
  summary,
  hasPreviewValuations,
}: {
  summary: PositionsSummary;
  hasPreviewValuations: boolean;
}) {
  return (
    <div className="positions-summary">
      <Card label="Open" value={String(summary.openCount)} />
      <Card label="Closed" value={String(summary.closedCount)} />
      <Card
        label="Total paid in"
        value={formatDusdcDisplay(summary.totalPremiumPaidRaw)}
      />
      <Card
        label="Worth if closed now"
        value={
          hasPreviewValuations
            ? formatDusdcDisplay(summary.totalEstimatedRedeemRaw)
            : "Checking…"
        }
        hint={
          hasPreviewValuations
            ? "Sum of every open position's live estimate."
            : "Pricing your open positions from the live market."
        }
      />
      <Card
        label="Open profit / loss"
        value={
          hasPreviewValuations
            ? formatDusdcDisplay(summary.totalUnrealizedPnlRaw)
            : "Checking…"
        }
        toneClass={pnlClassname(summary.totalUnrealizedPnlRaw)}
      />
      <Card
        label="Realized profit / loss"
        value={formatDusdcDisplay(summary.totalRealizedPnlRaw)}
        toneClass={pnlClassname(summary.totalRealizedPnlRaw)}
      />
    </div>
  );
}

function Card({
  label,
  value,
  toneClass,
  hint,
}: {
  label: string;
  value: string;
  toneClass?: string;
  hint?: string;
}) {
  return (
    <div className="positions-card">
      <span className="positions-card-label">{label}</span>
      <strong className={`positions-card-value mono ${toneClass ?? ""}`}>
        {value}
      </strong>
      {hint && <span className="positions-card-hint">{hint}</span>}
    </div>
  );
}

function OpenPositions({
  positions,
  livePreviews,
  onPreviewClose,
}: {
  positions: PositionRecord[];
  livePreviews: Record<string, LivePreview>;
  onPreviewClose: (position: PositionRecord, quantityBps: number) => void;
}) {
  if (positions.length === 0) {
    return (
      <div className="positions-empty">
        <h3>Nothing open yet</h3>
        <p>
          The positions you open from a strategy will show up here while
          they&apos;re live.
        </p>
      </div>
    );
  }
  return (
    <div className="positions-table-wrap">
      <h2 className="positions-section-h">Open</h2>
      <table className="positions-table">
        <thead>
          <tr>
            <th>Strategy</th>
            <th>Type</th>
            <th>Strike / band</th>
            <th>Open size</th>
            <th>Paid</th>
            <th>If you close now</th>
            <th>Profit / loss</th>
            <th>Expires</th>
            <th>Close</th>
          </tr>
        </thead>
        <tbody>
          {positions.map((p) => {
            const live = livePreviews[p.positionId];
            const payout = renderPayoutCell(p, live);
            const pnl = renderPnlCell(p, live);
            return (
              <tr key={p.positionId}>
                <td>{p.strategy ?? prettyStrategyFallback(p)}</td>
                <td>
                  <span className={`kind-pill subtle ${p.kind.toLowerCase()}`}>
                    {p.kind}
                  </span>
                </td>
                <td className="mono">{strikeText(p)}</td>
                <td className="mono">
                  {formatDusdcDisplay(p.remainingQuantityRaw)}
                </td>
                <td className="mono">{formatDusdcDisplay(p.premiumPaidRaw)}</td>
                <td className={`mono ${payout.cls}`}>{payout.text}</td>
                <td className={`mono ${pnl.cls}`}>{pnl.text}</td>
                <td className="mono">{formatExpiryCountdown(p.expiryMs)}</td>
                <td>
                  <div className="redeem-actions">
                    <button
                      type="button"
                      className="redeem-btn"
                      onClick={() => onPreviewClose(p, 2500)}
                    >
                      25%
                    </button>
                    <button
                      type="button"
                      className="redeem-btn"
                      onClick={() => onPreviewClose(p, 5000)}
                    >
                      50%
                    </button>
                    <button
                      type="button"
                      className="redeem-btn"
                      onClick={() => onPreviewClose(p, 10000)}
                    >
                      100%
                    </button>
                  </div>
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function prettyStrategyFallback(p: PositionRecord): string {
  // Older positions synced from chain don't carry strategy metadata; show
  // a plain-English placeholder rather than a raw em-dash.
  if (p.kind === "UP") return "Upside leg";
  if (p.kind === "DOWN") return "Downside leg";
  return "Range leg";
}

// Map a devInspect error string to a short, user-readable status. We look
// for the Move abort codes that `predict::redeem` can hit on chain — this
// avoids dumping a raw "VMVerificationOrDeserializationError, ..." into a
// production table cell.
function previewStatusFromError(
  p: PositionRecord,
  rawError: string,
): string {
  const msg = rawError.toLowerCase();
  const exp = Number(p.expiryMs);
  const isExpired = Number.isFinite(exp) && exp > 0 && exp <= Date.now();

  if (
    msg.includes("not_quoteable") ||
    msg.includes("quoteable_oracle") ||
    msg.includes("notlive") ||
    msg.includes("eoraclenotlive") ||
    isExpired
  ) {
    return "Awaiting settlement";
  }
  if (
    msg.includes("einsufficientposition") ||
    msg.includes("insufficient_position")
  ) {
    return "No longer held";
  }
  if (msg.includes("ezeroquantity")) {
    return "Closed";
  }
  return "Live preview blocked";
}

function renderPayoutCell(
  p: PositionRecord,
  live: LivePreview | undefined,
): { text: string; cls: string } {
  if (live) {
    if (live.error) return { text: previewStatusFromError(p, live.error), cls: "muted" };
    return { text: formatDusdcDisplay(live.payoutRaw.toString()), cls: "" };
  }
  // Before live data arrives: if we already know the position is expired,
  // skip the optimistic "Checking…" and surface the real state up front.
  const exp = Number(p.expiryMs);
  const isExpired = Number.isFinite(exp) && exp > 0 && exp <= Date.now();
  if (isExpired) return { text: "Awaiting settlement", cls: "muted" };
  if (p.lastPreviewAtUnix > 0 && p.lastPreviewPayoutRaw) {
    return { text: formatDusdcDisplay(p.lastPreviewPayoutRaw), cls: "" };
  }
  return { text: "Checking…", cls: "muted" };
}

function renderPnlCell(
  p: PositionRecord,
  live: LivePreview | undefined,
): { text: string; cls: string } {
  if (live) {
    if (live.error) return { text: previewStatusFromError(p, live.error), cls: "muted" };
    const v = live.pnlRaw;
    const cls = v > 0n ? "pnl-up" : v < 0n ? "pnl-down" : "";
    return { text: formatDusdcDisplay(v.toString()), cls };
  }
  const exp = Number(p.expiryMs);
  const isExpired = Number.isFinite(exp) && exp > 0 && exp <= Date.now();
  if (isExpired) return { text: "Awaiting settlement", cls: "muted" };
  if (p.lastPreviewAtUnix > 0 && p.lastPreviewPnlRaw) {
    return {
      text: formatDusdcDisplay(p.lastPreviewPnlRaw),
      cls: pnlClassname(p.lastPreviewPnlRaw),
    };
  }
  return { text: "Checking…", cls: "muted" };
}

function RedeemModal({
  owner,
  managerId,
  position,
  quantityBps,
  onClose,
  onRedeemed,
}: {
  owner: string;
  managerId: string;
  position: PositionRecord;
  quantityBps: number;
  onClose: () => void;
  onRedeemed: () => void;
}) {
  const suiClient = useSuiClient();
  const { mutateAsync: signAndExecuteTransaction } =
    useSignAndExecuteTransaction();

  const redeemQuantityRaw = useMemo(() => {
    try {
      const remaining = BigInt(position.remainingQuantityRaw);
      const q = (remaining * BigInt(quantityBps)) / 10000n;
      // Quantity must be > 0 — on-chain `redeem` asserts EZeroQuantity.
      return q > 0n ? q : 0n;
    } catch {
      return 0n;
    }
  }, [position.remainingQuantityRaw, quantityBps]);

  const [preview, setPreview] = useState<{
    payoutRaw: bigint;
    pnlRaw: bigint;
    isSettled: boolean;
  } | null>(null);
  const [previewing, setPreviewing] = useState(false);
  const [previewError, setPreviewError] = useState<string | null>(null);

  const [signing, setSigning] = useState(false);
  const [signError, setSignError] = useState<string | null>(null);
  const [signedDigest, setSignedDigest] = useState<string | null>(null);

  const buildRedeemTx = useCallback(() => {
    if (position.kind === "RANGE") {
      return buildRedeemPositionTransaction({
        owner,
        managerId,
        oracleId: position.oracleId,
        expiryMs: position.expiryMs,
        predictObjectId:
          "0xc8736204d12f0a7277c86388a68bf8a194b0a14c5538ad13f22cbd8e2a38028a",
        clockObjectId: "0x6",
        quantityRaw: redeemQuantityRaw.toString(),
        kind: "RANGE",
        lowerRaw: position.lowerRaw ?? "",
        upperRaw: position.upperRaw ?? "",
      });
    }
    return buildRedeemPositionTransaction({
      owner,
      managerId,
      oracleId: position.oracleId,
      expiryMs: position.expiryMs,
      predictObjectId:
        "0xc8736204d12f0a7277c86388a68bf8a194b0a14c5538ad13f22cbd8e2a38028a",
      clockObjectId: "0x6",
      quantityRaw: redeemQuantityRaw.toString(),
      kind: position.kind,
      strikeRaw: position.strikeRaw ?? "",
    });
  }, [owner, managerId, position, redeemQuantityRaw]);

  const onPreview = useCallback(async () => {
    if (redeemQuantityRaw === 0n) {
      setPreviewError("Nothing to close: redeem quantity is zero.");
      return;
    }
    setPreviewing(true);
    setPreviewError(null);
    try {
      const tx = buildRedeemTx();
      const result = await suiClient.devInspectTransactionBlock({
        sender: owner,
        transactionBlock: tx,
      });
      if (result.effects?.status?.status !== "success") {
        const msg =
          result.effects?.status?.error ?? "devInspect did not succeed";
        setPreviewError(msg);
        return;
      }
      const { payoutRaw, isSettled } = readRedeemPayoutFromEvents(
        (result.events ?? []) as Array<{ type?: string; parsedJson?: unknown }>,
      );

      // Pro-rata premium basis for the slice we're closing.
      let pnlRaw = 0n;
      try {
        const premium = BigInt(position.premiumPaidRaw);
        const original = BigInt(position.originalQuantityRaw);
        const basis =
          original === 0n ? 0n : (premium * redeemQuantityRaw) / original;
        pnlRaw = payoutRaw - basis;
      } catch {
        // ignore
      }
      setPreview({ payoutRaw, pnlRaw, isSettled });
    } catch (err) {
      setPreviewError(err instanceof Error ? err.message : String(err));
    } finally {
      setPreviewing(false);
    }
  }, [redeemQuantityRaw, buildRedeemTx, suiClient, owner, position]);

  useEffect(() => {
    if (!owner || !managerId || redeemQuantityRaw === 0n || previewing || preview) return;
    void onPreview();
  }, [
    owner,
    managerId,
    redeemQuantityRaw,
    previewing,
    preview,
    onPreview,
  ]);

  const onSign = useCallback(async () => {
    if (!preview) return;
    setSigning(true);
    setSignError(null);
    try {
      const tx = buildRedeemTx();
      const exec = await signAndExecuteTransaction({
        transaction: tx,
        chain: "sui:testnet",
      });
      const confirmed = await suiClient.waitForTransaction({
        digest: exec.digest,
        options: {
          showEffects: true,
          showEvents: true,
          showObjectChanges: true,
        },
      });
      if (confirmed.effects?.status?.status !== "success") {
        setSignError(
          confirmed.effects?.status?.error ?? "Redeem transaction failed.",
        );
        return;
      }
      setSignedDigest(exec.digest);

      // Hand the receipt to the backend so it updates the ledger.
      await auditRedeemPosition({
        owner,
        managerId,
        positionId: position.positionId,
        digest: exec.digest,
        effects: confirmed.effects ?? {},
        events: confirmed.events ?? [],
        objectChanges: confirmed.objectChanges ?? [],
      });

      // Give the user a second to see the success state.
      setTimeout(onRedeemed, 1200);
    } catch (err) {
      setSignError(err instanceof Error ? err.message : String(err));
    } finally {
      setSigning(false);
    }
  }, [preview, buildRedeemTx, signAndExecuteTransaction, suiClient, owner, managerId, position, onRedeemed]);

  const pct = quantityBps / 100;
  const canSign = Boolean(preview) && !signing && !signedDigest;

  return (
    <div className="redeem-modal-backdrop" onClick={onClose}>
      <div
        className="redeem-modal"
        role="dialog"
        aria-modal="true"
        onClick={(e) => e.stopPropagation()}
      >
        <header>
          <h3>Close {pct}% of position</h3>
          <button
            type="button"
            className="redeem-modal-close"
            onClick={onClose}
            aria-label="Close"
          >
            ×
          </button>
        </header>

        <dl className="redeem-modal-grid">
          <dt>Strategy</dt>
          <dd>{position.strategy ?? "—"}</dd>
          <dt>Kind</dt>
          <dd className="mono">{position.kind}</dd>
          <dt>Strike / range</dt>
          <dd className="mono">{strikeText(position)}</dd>
          <dt>Remaining</dt>
          <dd className="mono">
            {formatDusdcDisplay(position.remainingQuantityRaw)}
          </dd>
          <dt>Redeem quantity</dt>
          <dd className="mono">{formatDusdcDisplay(redeemQuantityRaw.toString())}</dd>
        </dl>

        {preview && (
          <div className="redeem-modal-preview">
            <div>
              <span>Estimated payout</span>
              <strong className="mono">
                {formatDusdcDisplay(preview.payoutRaw.toString())}
              </strong>
            </div>
            <div>
              <span>Estimated PnL on this slice</span>
              <strong
                className={`mono ${pnlClassname(preview.pnlRaw.toString())}`}
              >
                {formatDusdcDisplay(preview.pnlRaw.toString())}
              </strong>
            </div>
            {preview.isSettled && (
              <p className="redeem-modal-hint">
                The oracle has settled. This payout is final, not a live
                preview.
              </p>
            )}
          </div>
        )}

        <p className="redeem-modal-disclaimer">
          Closing this position is a wallet-signed transaction. The estimate
          above comes from an unsigned simulation. The actual payout can move
          if the market shifts before you sign. Funds are deposited into your{" "}
          <strong>PredictManager</strong>, not directly into your wallet.
        </p>

        {previewError && <p className="redeem-modal-error">{previewError}</p>}
        {signError && <p className="redeem-modal-error">{signError}</p>}
        {signedDigest && (
          <p className="redeem-modal-success">
            Redeem accepted · digest {shortAddress(signedDigest)}
          </p>
        )}

        <div className="redeem-modal-actions">
          <button
            type="button"
            className="redeem-btn"
            onClick={() => void onPreview()}
            disabled={previewing || signing || redeemQuantityRaw === 0n}
          >
            {previewing ? "Previewing…" : preview ? "Re-preview" : "Preview close"}
          </button>
          <button
            type="button"
            className="redeem-btn primary"
            onClick={() => void onSign()}
            disabled={!canSign}
          >
            {signing ? "Signing…" : signedDigest ? "Signed" : "Sign close"}
          </button>
        </div>
      </div>
    </div>
  );
}

function ClosedPositions({ positions }: { positions: PositionRecord[] }) {
  if (positions.length === 0) return null;
  return (
    <div className="positions-table-wrap" id="closed">
      <h2 className="positions-section-h">Closed</h2>
      <table className="positions-table">
        <thead>
          <tr>
            <th>Strategy</th>
            <th>Type</th>
            <th>Strike / band</th>
            <th>Original size</th>
            <th>Paid</th>
            <th>Got back</th>
            <th>Profit / loss</th>
          </tr>
        </thead>
        <tbody>
          {positions.map((p) => (
            <tr key={p.positionId}>
              <td>{p.strategy ?? prettyStrategyFallback(p)}</td>
              <td>
                <span className={`kind-pill subtle ${p.kind.toLowerCase()}`}>
                  {p.kind}
                </span>
              </td>
              <td className="mono">{strikeText(p)}</td>
              <td className="mono">
                {formatDusdcDisplay(p.originalQuantityRaw)}
              </td>
              <td className="mono">{formatDusdcDisplay(p.premiumPaidRaw)}</td>
              <td className="mono">{formatDusdcDisplay(p.realizedPayoutRaw)}</td>
              <td className={`mono ${pnlClassname(p.realizedPnlRaw)}`}>
                {formatDusdcDisplay(p.realizedPnlRaw)}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

const POSITIONS_CSS = `
.positions-shell {
  max-width: 1180px;
  margin: 0 auto;
  padding: 56px 28px 96px;
  display: grid;
  gap: 28px;
}
.positions-head {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 24px;
  flex-wrap: wrap;
}
.positions-head h1 {
  margin: 18px 0 10px;
  font-size: clamp(36px, 4.6vw, 52px);
  letter-spacing: -0.03em;
  font-weight: 600;
  color: var(--sx-navy);
}
.positions-sub {
  max-width: 560px;
  color: var(--sx-navy-muted);
  font-size: 15px;
  line-height: 1.55;
  margin: 0;
}
.positions-actions {
  display: inline-flex;
  gap: 8px;
  align-items: center;
  margin-top: 8px;
}
.positions-btn {
  background: var(--sx-surface);
  color: var(--sx-navy);
  border: 1px solid var(--sx-border-strong);
  border-radius: 999px;
  padding: 9px 16px;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  transition: background 0.15s ease;
}
.positions-btn:hover:not(:disabled) {
  background: var(--sx-surface-soft);
}
.positions-btn:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}
.positions-error {
  margin: 0;
  padding: 14px 18px;
  background: #fee2e2;
  border: 1px solid rgba(239, 68, 68, 0.25);
  border-radius: 12px;
  color: var(--sx-danger);
  font-size: 13px;
}
.positions-summary {
  display: grid;
  grid-template-columns: repeat(6, minmax(0, 1fr));
  gap: 12px;
}
.positions-card {
  background: var(--sx-surface);
  border: 1px solid var(--sx-border);
  border-radius: 16px;
  padding: 14px 16px;
  display: grid;
  gap: 4px;
}
.positions-card-label {
  font-family: var(--font-plex-mono), ui-monospace, monospace;
  font-size: 10px;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--sx-muted);
  font-weight: 500;
}
.positions-card-value {
  font-size: 17px;
  font-weight: 500;
  color: var(--sx-navy);
  letter-spacing: -0.01em;
  font-variant-numeric: tabular-nums;
}
.positions-card-value.pnl-up { color: var(--sx-teal-dark); }
.positions-card-value.pnl-down { color: var(--sx-danger); }
.positions-card-hint {
  font-size: 11px;
  color: var(--sx-muted);
}
.positions-table-wrap {
  background: var(--sx-surface);
  border: 1px solid var(--sx-border);
  border-radius: 16px;
  overflow: hidden;
}
.positions-section-h {
  margin: 0;
  padding: 16px 20px;
  font-size: 15px;
  font-weight: 600;
  letter-spacing: -0.01em;
  color: var(--sx-navy);
  border-bottom: 1px solid var(--sx-border);
}
.positions-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 13px;
}
.positions-table th {
  text-align: left;
  padding: 12px 20px;
  font-family: var(--font-plex-mono), ui-monospace, monospace;
  font-size: 10.5px;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--sx-muted);
  font-weight: 500;
  background: var(--sx-bg);
  border-bottom: 1px solid var(--sx-border);
}
.positions-table td {
  padding: 14px 20px;
  border-bottom: 1px solid var(--sx-border);
  color: var(--sx-navy);
}
.positions-table tr:last-child td { border-bottom: 0; }
.positions-table .mono { font-family: var(--font-plex-mono), ui-monospace, monospace; font-variant-numeric: tabular-nums; }
.positions-table .pnl-up { color: var(--sx-teal-dark); font-weight: 500; }
.positions-table .pnl-down { color: var(--sx-danger); font-weight: 500; }
.positions-table .muted { color: var(--sx-muted); font-weight: 500; }
.positions-empty {
  background: var(--sx-surface);
  border: 1px dashed var(--sx-border);
  border-radius: 16px;
  padding: 32px 24px;
  text-align: center;
  color: var(--sx-navy-muted);
}
.positions-empty h3 { margin: 0 0 6px; font-size: 17px; font-weight: 600; color: var(--sx-navy); }
.positions-empty p { margin: 0; font-size: 14px; }
.positions-empty a { color: var(--sx-teal-dark); font-weight: 600; }
/* Redeem action buttons inside the open-positions row */
.redeem-actions {
  display: inline-flex;
  gap: 4px;
}
.redeem-btn {
  font-family: var(--font-plex-mono), ui-monospace, monospace;
  font-size: 11px;
  font-weight: 600;
  padding: 6px 9px;
  border-radius: 8px;
  background: var(--sx-surface);
  color: var(--sx-navy);
  border: 1px solid var(--sx-border-strong);
  cursor: pointer;
  transition: background 0.15s ease, border-color 0.15s ease;
}
.redeem-btn:hover:not(:disabled) {
  background: var(--sx-surface-soft);
  border-color: var(--sx-teal-dark);
  color: var(--sx-teal-dark);
}
.redeem-btn:disabled { opacity: 0.5; cursor: not-allowed; }
.redeem-btn.primary {
  background: var(--sx-navy);
  color: var(--sx-surface);
  border-color: var(--sx-navy);
}
.redeem-btn.primary:hover:not(:disabled) {
  background: #0b1d36;
  color: var(--sx-surface);
}

/* Modal */
.redeem-modal-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(10, 22, 44, 0.42);
  backdrop-filter: blur(2px);
  z-index: 50;
  display: grid;
  place-items: center;
  padding: 20px;
}
.redeem-modal {
  width: 100%;
  max-width: 480px;
  background: var(--sx-surface);
  border-radius: 20px;
  border: 1px solid var(--sx-border);
  box-shadow: 0 24px 80px rgba(8, 18, 36, 0.22);
  padding: 24px 24px 20px;
  display: grid;
  gap: 18px;
}
.redeem-modal header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.redeem-modal h3 {
  margin: 0;
  font-size: 18px;
  font-weight: 600;
  letter-spacing: -0.015em;
  color: var(--sx-navy);
}
.redeem-modal-close {
  background: transparent;
  border: 0;
  font-size: 24px;
  color: var(--sx-muted);
  cursor: pointer;
  padding: 0 6px;
  line-height: 1;
}
.redeem-modal-close:hover { color: var(--sx-navy); }
.redeem-modal-grid {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 8px 18px;
  margin: 0;
  font-size: 13px;
}
.redeem-modal-grid dt {
  color: var(--sx-muted);
  font-family: var(--font-plex-mono), ui-monospace, monospace;
  font-size: 10.5px;
  text-transform: uppercase;
  letter-spacing: 0.08em;
}
.redeem-modal-grid dd {
  margin: 0;
  color: var(--sx-navy);
  font-weight: 500;
}
.redeem-modal-preview {
  display: grid;
  gap: 10px;
  padding: 14px 16px;
  background: var(--sx-surface-soft);
  border: 1px solid var(--sx-border);
  border-radius: 12px;
}
.redeem-modal-preview > div {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  gap: 12px;
}
.redeem-modal-preview span {
  font-family: var(--font-plex-mono), ui-monospace, monospace;
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--sx-muted);
}
.redeem-modal-preview strong {
  font-size: 16px;
  color: var(--sx-navy);
  letter-spacing: -0.005em;
  font-weight: 500;
}
.redeem-modal-preview .pnl-up { color: var(--sx-teal-dark); }
.redeem-modal-preview .pnl-down { color: var(--sx-danger); }
.redeem-modal-hint {
  margin: 0;
  padding: 8px 12px;
  background: var(--sx-blue-soft);
  border-radius: 8px;
  color: var(--sx-navy);
  font-size: 12px;
}
.redeem-modal-disclaimer {
  margin: 0;
  color: var(--sx-navy-muted);
  font-size: 12px;
  line-height: 1.55;
}
.redeem-modal-error {
  margin: 0;
  padding: 10px 12px;
  background: #fee2e2;
  border-radius: 8px;
  color: var(--sx-danger);
  font-size: 12.5px;
}
.redeem-modal-success {
  margin: 0;
  padding: 10px 12px;
  background: var(--sx-teal-soft);
  border-radius: 8px;
  color: var(--sx-teal-dark);
  font-size: 12.5px;
  font-weight: 600;
}
.redeem-modal-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
.redeem-modal-actions .redeem-btn {
  padding: 10px 16px;
  font-size: 13px;
}

@media (max-width: 980px) {
  .positions-summary { grid-template-columns: repeat(3, minmax(0, 1fr)); }
}
@media (max-width: 640px) {
  .positions-summary { grid-template-columns: repeat(2, minmax(0, 1fr)); }
  .positions-table th, .positions-table td { padding: 12px; }
}
`;
