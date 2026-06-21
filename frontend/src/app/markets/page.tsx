// /markets — DeepBook Predict market directory.
//
// Backed by GET /api/markets. The API serves a disk-backed snapshot first, then
// revalidates against the DeepBook Predict registry in the background. We keep
// the frontend parser intentionally loose so additive Rust fields do not break
// the page.

"use client";

import Link from "next/link";
import { useCallback, useEffect, useState } from "react";

import {
  LandingFooter,
  LandingHeader,
  LandingStyles,
} from "@/app/_landing-shared";
import {
  API_BASE,
  isAbortError,
} from "@/lib/api";
import { formatPriceCompact, shortAddress } from "@/lib/format";

// The API emits MarketSnapshot serialised almost verbatim; we use loose types
// because Rust enum tags don't always survive a round-trip without a custom
// schema. Each accessor below normalizes what it pulls.
type RawMarket = {
  list_item?: { oracle_id?: string; underlying_asset?: string; expiry_ms?: number | string };
  state?: { oracle_id?: string; underlying_asset?: string; expiry_ms?: number | string; status?: string };
  latest_price?: { value?: string | number; price?: string | number; timestamp_ms?: number | string } | null;
  latest_svi?: { timestamp_ms?: number | string } | null;
  structx_status?: unknown;
};

type MarketsEnvelope = {
  ok: boolean;
  asset?: string;
  network?: string;
  totalCount?: number;
  usableCount?: number;
  deepbookOnlyCount?: number;
  warningsCount?: number;
  assetCount?: number;
  structxSupportedAsset?: string;
  cachedAtUnix?: number;
  markets?: RawMarket[];
  error?: string;
  stderr?: string;
  stdout?: string;
};

type CachedMarketsEnvelope = {
  cachedAt: number;
  data: MarketsEnvelope;
};

type MarketRow = {
  oracleId: string;
  underlying: string;
  expiryMs: string;
  spotRaw: string | null;
  support: "StructX" | "DeepBook" | "Unavailable";
  reasons: string[];
  description: string;
  deepbookUrl: string;
};

const DEEPBOOK_TRADER_HUB_URL = "https://www.deepbook.tech/trader-hub";
const MARKETS_CACHE_KEYS = ["structx:markets:all", "structx:markets:btc"];
const REVALIDATE_MS = 45_000;
const COUNTDOWN_TICK_MS = 30_000;
const ASSET_PRIORITY = ["BTC", "ETH", "SOL", "SUI"];

function canonicalAsset(value: string | null | undefined): string {
  const trimmed = value?.trim();
  return trimmed ? trimmed.toUpperCase() : "Unknown";
}

function supportSummaryFromStatus(structxStatus: unknown): {
  support: MarketRow["support"];
  reasons: string[];
} {
  if (structxStatus === "Usable") {
    return { support: "StructX", reasons: [] };
  }

  if (structxStatus && typeof structxStatus === "object") {
    const obj = structxStatus as Record<string, unknown>;
    if ("UsableWithWarnings" in obj) {
      return { support: "StructX", reasons: [] };
    }
    if ("Rejected" in obj) {
      const rejected = obj.Rejected as Record<string, unknown> | undefined;
      const reasons = Array.isArray(rejected?.reasons)
        ? rejected.reasons.map((reason) => String(reason))
        : [];
      if (reasons.length > 0 && reasons.every((reason) => reason === "NonBtc")) {
        return { support: "DeepBook", reasons };
      }
      return { support: "Unavailable", reasons };
    }
  }

  return { support: "Unavailable", reasons: [] };
}

function marketDescription(
  underlying: string,
  expiryMs: string,
  support: MarketRow["support"],
): string {
  const expiry = formatExpiry(expiryMs);
  switch (support) {
    case "StructX":
      return `${underlying} terminal market settling at ${expiry}. Available in StructX and on DeepBook Predict.`;
    case "DeepBook":
      return `${underlying} terminal market settling at ${expiry}. Visible here for discovery, then opened on DeepBook Predict directly.`;
    default:
      return `${underlying} terminal market settling at ${expiry}. This market is listed in the live registry and is currently available for viewing.`;
  }
}

function readMarket(m: RawMarket): MarketRow {
  const oracleId =
    m.state?.oracle_id ?? m.list_item?.oracle_id ?? "0x0";
  const underlying = canonicalAsset(
    m.state?.underlying_asset ?? m.list_item?.underlying_asset,
  );
  const expiryMs = String(
    m.state?.expiry_ms ?? m.list_item?.expiry_ms ?? 0,
  );
  const spotRaw =
    m.latest_price?.value != null
      ? String(m.latest_price.value)
      : m.latest_price?.price != null
        ? String(m.latest_price.price)
        : null;

  const { support, reasons } = supportSummaryFromStatus(m.structx_status);

  return {
    oracleId,
    underlying,
    expiryMs,
    spotRaw,
    support,
    reasons,
    description: marketDescription(underlying, expiryMs, support),
    deepbookUrl: DEEPBOOK_TRADER_HUB_URL,
  };
}

function formatExpiry(expiryMs: string): string {
  const n = Number(expiryMs);
  if (!Number.isFinite(n) || n <= 0) return "Unavailable";
  const date = new Date(n);
  return date.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function formatCountdown(expiryMs: string): string {
  const n = Number(expiryMs);
  if (!Number.isFinite(n) || n <= 0) return "Unavailable";
  const diff = n - Date.now();
  if (diff <= 0) return "Expired";
  const days = Math.floor(diff / 86_400_000);
  const hours = Math.floor((diff % 86_400_000) / 3_600_000);
  if (days > 0) return `${days}d ${hours}h`;
  const minutes = Math.floor((diff % 3_600_000) / 60_000);
  return `${hours}h ${minutes}m`;
}

function expiryBucket(expiryMs: string): string {
  const n = Number(expiryMs);
  if (!Number.isFinite(n) || n <= 0) return "Unknown";
  const diff = n - Date.now();
  if (diff <= 0) return "Expired";
  if (diff < 3_600_000) return "Sub-hour";
  if (diff < 86_400_000) return "Same day";
  if (diff < 7 * 86_400_000) return "This week";
  return "Longer dated";
}

function isStructxSupported(row: MarketRow): boolean {
  return row.support === "StructX";
}

function statusTone(support: MarketRow["support"]): "ok" | "warn" | "bad" | "neutral" {
  if (support === "StructX") return "ok";
  if (support === "DeepBook") return "warn";
  if (support === "Unavailable") return "bad";
  return "neutral";
}

function readCachedMarkets(): CachedMarketsEnvelope | null {
  if (typeof window === "undefined") return null;
  for (const key of MARKETS_CACHE_KEYS) {
    try {
      const raw = window.localStorage.getItem(key);
      if (!raw) continue;
      const parsed = JSON.parse(raw) as CachedMarketsEnvelope;
      if (
        !parsed ||
        typeof parsed !== "object" ||
        typeof parsed.cachedAt !== "number" ||
        !parsed.data
      ) {
        continue;
      }
      return parsed;
    } catch {
      // keep scanning legacy keys
    }
  }
  return null;
}

function writeCachedMarkets(data: MarketsEnvelope): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(
      MARKETS_CACHE_KEYS[0],
      JSON.stringify({
        cachedAt:
          typeof data.cachedAtUnix === "number" ? data.cachedAtUnix * 1000 : Date.now(),
        data,
      } satisfies CachedMarketsEnvelope),
    );
  } catch {
    // ignore cache write failures
  }
}

function sortAssetKeys(a: string, b: string): number {
  const aPriority = ASSET_PRIORITY.indexOf(a);
  const bPriority = ASSET_PRIORITY.indexOf(b);
  if (aPriority !== -1 || bPriority !== -1) {
    if (aPriority === -1) return 1;
    if (bPriority === -1) return -1;
    return aPriority - bPriority;
  }
  return a.localeCompare(b);
}

function formatCacheAge(cachedAt: number | null): string {
  if (!cachedAt || cachedAt <= 0) return "Waiting for first sync";
  const diffMs = Date.now() - cachedAt;
  if (diffMs < 30_000) return "Synced just now";
  const minutes = Math.floor(diffMs / 60_000);
  if (minutes < 60) return `Synced ${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  return `Synced ${hours}h ago`;
}

export default function MarketsPage() {
  const [data, setData] = useState<MarketsEnvelope | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [cachedAt, setCachedAt] = useState<number | null>(null);
  const [assetFilter, setAssetFilter] = useState("ALL");
  const [, setTick] = useState(0);

  const load = useCallback(async (
    signal?: AbortSignal,
    mode: "initial" | "background" | "manual" = "manual",
  ) => {
    setLoading(true);
    if (mode !== "background") {
      setError(null);
    }
    try {
      const r = await fetch(`${API_BASE}/api/markets`, { signal });
      // Read body once as text so we can produce a useful error when the
      // backend returns nothing (most common cause: route not yet wired
      // because the API binary hasn't been rebuilt with the new code).
      const text = await r.text();
      if (!text.trim()) {
        const fallback = !data ? readCachedMarkets() : null;
        if (fallback?.data?.ok) {
          setData(fallback.data);
          setCachedAt(fallback.cachedAt);
        }
        if (r.status === 404) {
          if (mode !== "background") {
            setError(
              "The /api/markets route is not available on the running API server. Rebuild and restart structx-api so it picks up the new route.",
            );
          }
        } else {
          if (mode !== "background") {
            setError(`Backend returned ${r.status} with an empty body.`);
          }
        }
        return;
      }
      let json: MarketsEnvelope;
      try {
        json = JSON.parse(text) as MarketsEnvelope;
      } catch (parseErr) {
        const fallback = !data ? readCachedMarkets() : null;
        if (fallback?.data?.ok) {
          setData(fallback.data);
          setCachedAt(fallback.cachedAt);
        }
        if (mode !== "background") {
          setError(
            `Backend returned non-JSON (${parseErr instanceof Error ? parseErr.message : "parse failed"}). First 200 chars: ${text.slice(0, 200)}`,
          );
        }
        return;
      }
      if (!r.ok || !json.ok) {
        const fallback = !data ? readCachedMarkets() : null;
        if (fallback?.data?.ok) {
          setData(fallback.data);
          setCachedAt(fallback.cachedAt);
        }
        // Surface backend stderr when present so the user sees the actual
        // failure instead of a context-free non-zero status.
        const cliErr = (json.stderr ?? "").trim();
        const headline = json.error ?? `Backend returned ${r.status}`;
        if (mode !== "background") {
          setError(cliErr ? `${headline}: ${cliErr}` : headline);
        }
        return;
      }
      setData(json);
      setCachedAt(
        typeof json.cachedAtUnix === "number" ? json.cachedAtUnix * 1000 : Date.now(),
      );
      writeCachedMarkets(json);
      setError(null);
    } catch (err) {
      if (isAbortError(err)) return;
      const fallback = !data ? readCachedMarkets() : null;
      if (fallback?.data?.ok) {
        setData(fallback.data);
        setCachedAt(fallback.cachedAt);
      }
      if (mode !== "background") {
        setError(err instanceof Error ? err.message : String(err));
      }
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    const c = new AbortController();
    void load(c.signal, "initial");
    return () => c.abort();
  }, [load]);

  useEffect(() => {
    const interval = window.setInterval(() => {
      setTick((value) => value + 1);
    }, COUNTDOWN_TICK_MS);
    return () => window.clearInterval(interval);
  }, []);

  useEffect(() => {
    const interval = window.setInterval(() => {
      const controller = new AbortController();
      void load(controller.signal, "background");
      window.setTimeout(() => controller.abort(), 12_000);
    }, REVALIDATE_MS);
    return () => window.clearInterval(interval);
  }, [load]);

  const rows = (data?.markets ?? []).map(readMarket);
  rows.sort((a, b) => Number(a.expiryMs) - Number(b.expiryMs));

  const assetCounts = rows.reduce<Record<string, number>>((acc, row) => {
    acc[row.underlying] = (acc[row.underlying] ?? 0) + 1;
    return acc;
  }, {});
  const assetCategories = Object.keys(assetCounts).sort(sortAssetKeys);

  useEffect(() => {
    if (assetFilter !== "ALL" && !assetCategories.includes(assetFilter)) {
      setAssetFilter("ALL");
    }
  }, [assetCategories, assetFilter]);

  const filteredRows =
    assetFilter === "ALL"
      ? rows
      : rows.filter((row) => row.underlying === assetFilter);
  const nextTradable = filteredRows[0] ?? rows[0];
  const structxSupportedCount = filteredRows.filter(isStructxSupported).length;
  const deepbookOnlyCount = filteredRows.filter((row) => row.support === "DeepBook").length;

  const assetSections = (assetFilter === "ALL" ? assetCategories : [assetFilter])
    .map((asset) => ({
      asset,
      rows: rows.filter((row) => row.underlying === asset),
    }))
    .filter((section) => section.rows.length > 0)
    .map((section) => {
      const groupedRows = section.rows.reduce<Record<string, MarketRow[]>>((acc, row) => {
        const key = expiryBucket(row.expiryMs);
        acc[key] ??= [];
        acc[key].push(row);
        return acc;
      }, {});
      const groupOrder = ["Sub-hour", "Same day", "This week", "Longer dated", "Expired", "Unknown"];
      return {
        ...section,
        groups: groupOrder
          .filter((key) => groupedRows[key]?.length)
          .map((key) => ({ key, rows: groupedRows[key] })),
      };
    });

  return (
    <main className="landing">
      <LandingHeader />
      <section className="markets-shell">
        <header className="markets-head">
          <div>
            <p className="strategies-eyebrow">
              <span className="strategies-eyebrow-dot" aria-hidden />
              Live markets
            </p>
            <h1>DeepBook Predict markets</h1>
            <p className="markets-sub">
              Browse the active expiry markets by asset. Supported BTC markets
              can be used in StructX, and every market can be opened directly
              in DeepBook.
            </p>
          </div>
          <button
            type="button"
            className={`markets-refresh ${loading ? "is-loading" : ""}`}
            onClick={() => void load(undefined, "manual")}
            disabled={loading}
            aria-label="Refresh markets"
          >
            <svg
              className="markets-refresh-icon"
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
              <path d="M21 12a9 9 0 1 1-3.5-7.1" />
              <path d="M21 4v5h-5" />
            </svg>
            <span>{loading ? "Refreshing" : "Refresh"}</span>
          </button>
        </header>

        {error && <p className="positions-error">{error}</p>}

        {data && (
          <>
            <div className="markets-counts">
              <span>
                <strong>{filteredRows.length}</strong> live{" "}
                {assetFilter === "ALL" ? "Predict" : assetFilter}{" "}
                {filteredRows.length === 1 ? "market" : "markets"}
              </span>
              <span className="dot" aria-hidden />
              <span>
                <strong>{data.assetCount ?? assetCategories.length}</strong> assets
              </span>
              <span className="dot" aria-hidden />
              <span>{formatCacheAge(cachedAt)}</span>
            </div>

            {assetCategories.length === 1 && assetCategories[0] === "BTC" && (
              <p className="markets-note">
                DeepBook currently has active BTC markets in this snapshot.
              </p>
            )}

            <div className="markets-asset-rail" role="tablist" aria-label="Asset categories">
              <button
                type="button"
                className={`markets-asset-pill ${assetFilter === "ALL" ? "active" : ""}`}
                onClick={() => setAssetFilter("ALL")}
                aria-pressed={assetFilter === "ALL"}
              >
                <span>All markets</span>
                <strong>{rows.length}</strong>
              </button>
              {assetCategories.map((asset) => (
                <button
                  key={asset}
                  type="button"
                  className={`markets-asset-pill ${assetFilter === asset ? "active" : ""}`}
                  onClick={() => setAssetFilter(asset)}
                  aria-pressed={assetFilter === asset}
                >
                  <span>{asset}</span>
                  <strong>{assetCounts[asset]}</strong>
                </button>
              ))}
            </div>

            <div className="markets-stat-grid">
              <article className="markets-stat-card">
                <span>Next expiry</span>
                <strong>
                  {nextTradable ? formatCountdown(nextTradable.expiryMs) : "Unavailable"}
                </strong>
                <small>
                  {nextTradable
                    ? formatExpiry(nextTradable.expiryMs)
                    : "No active Predict market right now"}
                </small>
              </article>
              <article className="markets-stat-card">
                <span>Spot reference</span>
                <strong>
                  {nextTradable?.spotRaw
                    ? formatPriceCompact(nextTradable.spotRaw)
                    : "Unavailable"}
                </strong>
                <small>Latest price snapshot from the oracle feed</small>
              </article>
              <article className="markets-stat-card">
                <span>StructX support</span>
                <strong>{structxSupportedCount}</strong>
                <small>
                  {assetFilter === "ALL"
                    ? "Markets StructX can compile today"
                    : `${assetFilter} markets currently openable from StructX`}
                </small>
              </article>
              <article className="markets-stat-card">
                <span>DeepBook direct</span>
                <strong>{deepbookOnlyCount}</strong>
                <small>
                  Markets available directly through DeepBook
                </small>
              </article>
            </div>
          </>
        )}

        {data && filteredRows.length === 0 && !loading && (
          <div className="positions-empty">
            <h3>No live markets in this category</h3>
            <p>
              Switch asset categories or wait for the next DeepBook refresh.
            </p>
          </div>
        )}

        {assetSections.length > 0 && (
          <div className="markets-board">
            {assetSections.map((section) => (
              <section className="markets-asset-section" key={section.asset}>
                <div className="markets-asset-head">
                  <div>
                    <p className="markets-asset-kicker">{section.asset}</p>
                    <h2>
                      {section.rows.length} {section.asset} market
                      {section.rows.length === 1 ? "" : "s"}
                    </h2>
                    <p className="markets-group-note">
                      Active expiry markets keyed off the {section.asset} oracle feed.
                    </p>
                  </div>
                </div>

                {section.groups.map((group) => (
                  <section className="markets-group" key={`${section.asset}-${group.key}`}>
                    <div className="markets-group-head">
                      <div>
                        <p className="markets-group-kicker">{group.key}</p>
                        <h3 className="markets-group-title">
                          {group.rows.length} expiry
                          {group.rows.length === 1 ? "" : "ies"}
                        </h3>
                      </div>
                    </div>

                    <div className="markets-card-grid">
                      {group.rows.map((m) => (
                        <article
                          className="markets-card"
                          key={`${m.oracleId}-${m.expiryMs}`}
                        >
                          <div className="markets-card-top">
                            <div>
                              <p className="markets-card-kicker">
                                {m.underlying} oracle
                              </p>
                              <h4 title={m.oracleId}>{shortAddress(m.oracleId)}</h4>
                            </div>
                            <span className={`market-status market-status-${statusTone(m.support)}`}>
                              {m.support}
                            </span>
                          </div>

                          <p className="markets-card-description">{m.description}</p>

                          <div className="markets-card-metrics">
                            <div>
                              <span>Spot</span>
                              <strong className="mono">
                                {m.spotRaw ? formatPriceCompact(m.spotRaw) : "Unavailable"}
                              </strong>
                            </div>
                            <div>
                              <span>Expires in</span>
                              <strong className="mono">
                                {formatCountdown(m.expiryMs)}
                              </strong>
                            </div>
                            <div>
                              <span>Expiry</span>
                              <strong className="mono">
                                {formatExpiry(m.expiryMs)}
                              </strong>
                            </div>
                            <div>
                              <span>Route</span>
                              <strong>{m.support === "StructX" ? "StructX + DeepBook" : "DeepBook"}</strong>
                            </div>
                          </div>

                          <div className="markets-card-actions">
                            {m.support === "StructX" && (
                              <Link
                                href="/strategies"
                                className="market-cta market-cta-primary"
                              >
                                Build with StructX
                              </Link>
                            )}
                            <a
                              href={m.deepbookUrl}
                              target="_blank"
                              rel="noreferrer noopener"
                              className={`market-cta ${m.support === "StructX" ? "market-cta-secondary" : "market-cta-primary"}`}
                            >
                              Open in DeepBook
                            </a>
                          </div>
                        </article>
                      ))}
                    </div>
                  </section>
                ))}
              </section>
            ))}
          </div>
        )}

        <p className="markets-note">
          Market data loads from the latest saved snapshot while StructX checks
          DeepBook for new assets, upcoming expiries, and expired markets in the
          background.
        </p>
      </section>
      <LandingFooter />
      <LandingStyles />
      <style>{MARKETS_CSS}</style>
    </main>
  );
}

const MARKETS_CSS = `
.markets-shell {
  max-width: 1180px;
  margin: 0 auto;
  padding: 56px 28px 96px;
  display: grid;
  gap: 26px;
}
.markets-head {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 24px;
  flex-wrap: wrap;
}
.markets-refresh {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  height: 36px;
  padding: 0 16px 0 14px;
  border-radius: 999px;
  background: var(--sx-navy);
  color: var(--sx-surface);
  border: 1px solid var(--sx-navy);
  font-size: 13px;
  font-weight: 600;
  letter-spacing: -0.005em;
  cursor: pointer;
  transition: background 0.15s ease, transform 0.06s ease, opacity 0.15s ease;
  box-shadow: 0 1px 0 rgba(16, 40, 74, 0.04);
}
.markets-refresh:hover:not(:disabled) {
  background: #0b1d36;
}
.markets-refresh:active:not(:disabled) {
  transform: translateY(1px);
}
.markets-refresh:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}
.markets-refresh-icon {
  flex: 0 0 auto;
  color: currentColor;
}
.markets-refresh.is-loading .markets-refresh-icon {
  animation: markets-refresh-spin 0.8s linear infinite;
}
@keyframes markets-refresh-spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}

.markets-head h1 {
  margin: 18px 0 10px;
  font-size: clamp(36px, 4.6vw, 52px);
  letter-spacing: -0.03em;
  font-weight: 600;
  color: var(--sx-navy);
}
.markets-sub {
  max-width: 700px;
  color: var(--sx-navy-muted);
  font-size: 15px;
  line-height: 1.65;
  margin: 0;
}
.markets-counts {
  display: inline-flex;
  align-items: center;
  gap: 12px;
  padding: 9px 16px;
  border: 1px solid var(--sx-border);
  border-radius: 999px;
  background: var(--sx-surface);
  color: var(--sx-navy-muted);
  font-family: var(--font-plex-mono), ui-monospace, monospace;
  font-size: 12px;
  font-variant-numeric: tabular-nums;
}
.markets-stat-grid {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 16px;
}
.markets-stat-card {
  display: grid;
  gap: 8px;
  padding: 20px;
  border-radius: 24px;
  border: 1px solid var(--sx-border);
  background:
    linear-gradient(180deg, rgba(255,255,255,0.98), rgba(246,249,252,0.98));
  box-shadow: 0 20px 45px rgba(16, 40, 74, 0.06);
}
.markets-stat-card span {
  color: var(--sx-muted);
  font-size: 12px;
  letter-spacing: 0.06em;
  text-transform: uppercase;
}
.markets-stat-card strong {
  color: var(--sx-navy);
  font-size: clamp(24px, 3vw, 32px);
  font-weight: 600;
  letter-spacing: -0.04em;
  line-height: 1.05;
}
.markets-stat-card small {
  color: var(--sx-navy-muted);
  font-size: 13px;
  line-height: 1.5;
}
.markets-counts strong {
  color: var(--sx-navy);
  font-weight: 600;
  margin-right: 4px;
}
.markets-counts .dot {
  width: 3px;
  height: 3px;
  border-radius: 50%;
  background: var(--sx-border-strong);
}
.markets-asset-rail {
  display: flex;
  gap: 10px;
  overflow-x: auto;
  padding-bottom: 4px;
}
.markets-asset-pill {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  min-height: 38px;
  padding: 0 14px;
  border-radius: 999px;
  border: 1px solid var(--sx-border);
  background: rgba(255,255,255,0.92);
  color: var(--sx-navy-muted);
  cursor: pointer;
  white-space: nowrap;
  transition: border-color 0.15s ease, background 0.15s ease, color 0.15s ease;
}
.markets-asset-pill strong {
  font-family: var(--font-plex-mono), ui-monospace, monospace;
  font-size: 12px;
  color: var(--sx-navy);
}
.markets-asset-pill.active {
  background: var(--sx-navy);
  border-color: var(--sx-navy);
  color: var(--sx-surface);
}
.markets-asset-pill.active strong {
  color: var(--sx-surface);
}
.market-status {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 4px 10px;
  border-radius: 999px;
  font-size: 11.5px;
  font-weight: 500;
  border: 1px solid var(--sx-border);
}
.market-status-ok {
  background: var(--sx-teal-soft);
  color: var(--sx-teal-dark);
  border-color: rgba(33,196,163,.3);
}
.market-status-warn {
  background: #fef3c7;
  color: #b45309;
  border-color: rgba(180,83,9,.25);
}
.market-status-bad {
  background: #fee2e2;
  color: var(--sx-danger);
  border-color: rgba(239,68,68,.3);
}
.market-status-neutral {
  background: var(--sx-surface-soft);
  color: var(--sx-navy-muted);
}
.markets-board {
  display: grid;
  gap: 22px;
}
.markets-asset-section {
  display: grid;
  gap: 18px;
}
.markets-asset-head {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: 18px;
}
.markets-asset-kicker {
  margin: 0 0 6px;
  color: var(--sx-teal-dark);
  font-size: 12px;
  font-weight: 600;
  letter-spacing: 0.12em;
  text-transform: uppercase;
}
.markets-asset-head h2 {
  margin: 0;
  font-size: clamp(30px, 4vw, 40px);
  letter-spacing: -0.04em;
  line-height: 1;
}
.markets-group {
  display: grid;
  gap: 18px;
  padding: 24px;
  border: 1px solid var(--sx-border);
  border-radius: 28px;
  background: linear-gradient(180deg, rgba(255,255,255,0.96), rgba(239,244,248,0.92));
  box-shadow: 0 26px 60px rgba(16, 40, 74, 0.06);
}
.markets-group-head {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: 18px;
  flex-wrap: wrap;
}
.markets-group-kicker {
  margin: 0 0 4px;
  color: var(--sx-teal-dark);
  font-size: 12px;
  font-weight: 600;
  letter-spacing: 0.1em;
  text-transform: uppercase;
}
.markets-group-head h2 {
  margin: 0;
  font-size: clamp(24px, 3vw, 32px);
  letter-spacing: -0.035em;
  line-height: 1.05;
}
.markets-group-title {
  margin: 0;
  font-size: 24px;
  letter-spacing: -0.03em;
  line-height: 1.05;
}
.markets-group-note {
  margin: 0;
  max-width: 360px;
  color: var(--sx-muted);
  font-size: 13px;
  line-height: 1.55;
}
.markets-card-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 16px;
}
.markets-card {
  display: grid;
  gap: 16px;
  padding: 18px;
  border-radius: 22px;
  border: 1px solid rgba(194, 210, 224, 0.9);
  background: rgba(255,255,255,0.92);
}
.markets-card-top {
  display: flex;
  justify-content: space-between;
  gap: 14px;
  align-items: flex-start;
}
.markets-card-kicker {
  margin: 0 0 4px;
  color: var(--sx-muted);
  font-size: 11px;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}
.markets-card-top h4 {
  margin: 0;
  font-family: var(--font-plex-mono), ui-monospace, monospace;
  font-size: 22px;
  letter-spacing: -0.03em;
  font-weight: 500;
}
.markets-card-description {
  margin: 0;
  color: var(--sx-navy-muted);
  font-size: 14px;
  line-height: 1.55;
}
.markets-card-metrics {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 14px 16px;
}
.markets-card-metrics span {
  display: block;
  margin-bottom: 4px;
  color: var(--sx-muted);
  font-size: 11px;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}
.markets-card-metrics strong {
  display: block;
  color: var(--sx-navy);
  font-size: 18px;
  font-weight: 500;
  letter-spacing: -0.03em;
}
.markets-card-actions {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}
.market-cta {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-height: 36px;
  padding: 0 14px;
  border-radius: 999px;
  font-size: 12.5px;
  font-weight: 600;
  white-space: nowrap;
  border: 1px solid var(--sx-border);
  transition: transform 0.06s ease, background 0.15s ease, border-color 0.15s ease;
}
.market-cta:hover {
  transform: translateY(-1px);
}
.market-cta-primary {
  background: var(--sx-navy);
  border-color: var(--sx-navy);
  color: #ffffff;
}
.market-cta-secondary {
  background: rgba(255,255,255,0.96);
  color: var(--sx-navy);
}
.markets-note {
  margin: 0;
  color: var(--sx-muted);
  font-size: 13px;
  line-height: 1.55;
}
@media (max-width: 1080px) {
  .markets-stat-grid,
  .markets-card-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}
@media (max-width: 760px) {
  .markets-shell {
    padding: 34px 18px 72px;
  }
  .markets-head h1 {
    margin-top: 14px;
    font-size: clamp(30px, 10vw, 42px);
  }
  .markets-stat-grid,
  .markets-card-grid,
  .markets-card-metrics {
    grid-template-columns: 1fr;
  }
  .markets-group {
    padding: 18px;
    border-radius: 22px;
  }
  .markets-asset-head {
    align-items: flex-start;
  }
  .markets-group-note {
    max-width: none;
    text-align: left;
  }
  .markets-card-top {
    flex-direction: column;
  }
  .markets-counts {
    display: flex;
    flex-wrap: wrap;
    row-gap: 8px;
  }
}
`;
