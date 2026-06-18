// Typed API client for the StructX backend.
//
// Design notes
// ------------
// * Every endpoint accepts an optional AbortSignal so React components can
//   cancel in-flight calls on unmount / wallet switch (no leaks, no stale
//   state writes).
// * Reads that frequently repeat (manager lookup, manager balance) flow
//   through an in-memory cache with TTL + in-flight dedup, so component
//   re-renders / route changes within seconds are free.
// * Best-effort writes (save manager) intentionally swallow errors and
//   degrade gracefully — they're persistence hints, not user-facing actions.

import { cachedFetch, invalidate, seed } from "@/lib/cache";
import type {
  ApiErrorBody,
  AuditInput,
  AuditResponse,
  BuildOpenInput,
  BuildOpenStrategyResponse,
  CompileFromIntentInput,
  CompileInput,
  CompileResponse,
  GuidedCompileResponse,
  ManagerBalanceResponse,
  ParseIntentInput,
  ParsedIntentResponse,
} from "@/types/structx";

export const API_BASE =
  process.env.NEXT_PUBLIC_STRUCTX_API_BASE ?? "http://127.0.0.1:8787";

// Cache namespaces — kept as constants so callers can invalidate them from
// outside without re-typing string literals.
const NS_MANAGER = "managers";
const NS_BALANCE = "balance";

// TTLs: tuned for the actual data-change cadence.
//   Manager mapping: only changes when the user creates a new manager. 60s
//   is a safe ceiling — after a create we explicitly seed/invalidate.
//   Balance: changes after every mint/redeem. 5s is short enough that the
//   user perceives it as live and long enough to dedup re-renders.
const MANAGER_TTL_MS = 60_000;
const BALANCE_TTL_MS = 5_000;

export class ApiError extends Error {
  body: ApiErrorBody;
  endpoint: string;
  status: number;

  constructor(message: string, body: ApiErrorBody, endpoint: string, status: number) {
    super(message);
    this.body = body;
    this.endpoint = endpoint;
    this.status = status;
  }
}

// Lightweight predicate so callers can branch on cancel without unwrapping
// to dom-specific error shapes.
export function isAbortError(err: unknown): boolean {
  return err instanceof DOMException && err.name === "AbortError";
}

type FetchOpts = { signal?: AbortSignal };

export type CatalogMarketStatus =
  | "inactive"
  | "active"
  | "pending_settlement"
  | "settled"
  | "expired_unknown"
  | "unknown";

export type CatalogMarketKind =
  | "scalar_price"
  | "scalar_event"
  | "binary_event"
  | "categorical_event"
  | "unknown";

export type CatalogMarketCategory =
  | "crypto"
  | "finance"
  | "sports"
  | "politics"
  | "macro"
  | "weather"
  | "other"
  | "unknown";

export type CatalogMarketSnapshot = {
  market_id: string;
  oracle_id: string;
  underlying: string;
  display_name: string;
  category: CatalogMarketCategory;
  market_kind: CatalogMarketKind;
  expiry_ms: number;
  status: CatalogMarketStatus;
  spot?: number | null;
  settlement_price?: number | null;
  valid_strikes: number[];
  min_strike?: number | null;
  max_strike?: number | null;
  quote_assets: string[];
  preferred_quote_asset: string;
  latest_price_updated_at_ms?: number | null;
  svi_updated_at_ms?: number | null;
  fetched_at_ms: number;
  tags: string[];
  metadata: unknown;
};

export type MarketCatalogStatusResponse = {
  exists: boolean;
  schema_version?: number | null;
  market_count: number;
  active_market_count: number;
  last_refreshed_at_ms?: number | null;
  age_ms?: number | null;
  source?: string | null;
  warnings: string[];
};

export type MarketCatalogRefreshResponse = {
  ok: boolean;
  market_count: number;
  active_market_count: number;
  report: {
    total_input_items: number;
    accepted_markets: number;
    rejected_items: number;
    warnings: string[];
  };
};

export type StrategyTemplateId =
  | "directional_above"
  | "directional_below"
  | "range_inside"
  | "breakout_outside"
  | "one_sided_tail"
  | "upside_rocket"
  | "custom_piecewise"
  | "smart_budget";

export type RiskStyle =
  | "conservative"
  | "balanced"
  | "aggressive"
  | "tail_heavy"
  | "higher_hit_rate";

export type IntentConfidence = "high" | "medium" | "low" | "none";

export type IntentPlan = {
  raw_prompt: string;
  market_query: string;
  category_hint?: CatalogMarketCategory | null;
  market_kind_hint?: CatalogMarketKind | null;
  strategy_template: StrategyTemplateId;
  direction?: "up" | "down" | "either_side" | "inside_range" | "unknown" | null;
  range?: {
    lower?: number | null;
    upper?: number | null;
  } | null;
  budget?: number | null;
  quote_asset: string;
  risk_style: RiskStyle;
  expiry_preference: string;
  confidence: IntentConfidence;
  needs_clarification: boolean;
  clarification_question?: string | null;
  assumptions: string[];
  warnings: string[];
};

export type IntentPlanningResponse = {
  intent_plan: IntentPlan;
  candidate_markets: CatalogMarketSnapshot[];
  selected_market?: CatalogMarketSnapshot | null;
  needs_clarification: boolean;
  clarification_question?: string | null;
};

export type ProposalQuoteMetadata = {
  quote_batch_id: string;
  quoted_at_ms: number;
  max_quote_age_ms: number;
  source: string;
  oracle_id: string;
  market_fetched_at_ms: number;
};

export type CompiledProposalLeg = {
  kind: string;
  oracle_id: string;
  expiry_ms: number;
  strike?: number | null;
  lower?: number | null;
  upper?: number | null;
  quantity: number;
  ask_price?: number | null;
  premium?: number | null;
  role?: string | null;
  label?: string | null;
};

export type ProposalPayoffRow = {
  label: string;
  settlement_lower?: number | null;
  settlement_upper?: number | null;
  gross_payout: number;
  net_pnl: number;
};

export type ExecutionProposal = {
  proposal_id: string;
  user_address?: string | null;
  raw_prompt: string;
  selected_market: CatalogMarketSnapshot;
  reason_for_selection: string;
  strategy_template: StrategyTemplateId;
  backend_strategy_id: string;
  legs: CompiledProposalLeg[];
  total_premium: number;
  max_loss: number;
  max_payout: number;
  payoff_table: ProposalPayoffRow[];
  net_pnl_table: ProposalPayoffRow[];
  quote_metadata: ProposalQuoteMetadata;
  assumptions: string[];
  warnings: string[];
  requires_user_signature: boolean;
  raw_compiled_strategy: unknown;
};

export type IntentExecutionAudit = {
  schema_version: number;
  audit_id: string;
  proposal_id: string;
  user_address?: string | null;
  manager_id?: string | null;
  tx_digest: string;
  status: "submitted" | "confirmed" | "failed" | "unknown";
  market_id: string;
  oracle_id: string;
  underlying: string;
  strategy_template: string;
  backend_strategy_id: string;
  total_premium: number;
  max_loss: number;
  max_payout: number;
  created_at_ms: number;
  updated_at_ms: number;
  warnings: string[];
  raw_execution_result: unknown;
  proposal: ExecutionProposal;
};

export type AuditIntentExecutionResponse = {
  ok: boolean;
  audit: IntentExecutionAudit;
  position_sync_status: string;
  position_ids: string[];
  warnings: string[];
};

export type IntentExecutePlanResponse = {
  proposal_id: string;
  user_address?: string | null;
  compiled_strategy_id?: string | null;
  raw_compiled_strategy: unknown;
  proposal: ExecutionProposal;
  warnings: string[];
};

export type IntentPositionStatus =
  | "pending_confirmation"
  | "open_pending_ledger_sync"
  | "failed"
  | "unknown";

export type IntentPositionSummary = {
  source: "intent_audit_overlay" | string;
  proposal_id: string;
  audit_id: string;
  tx_digest: string;
  user_address?: string | null;
  manager_id?: string | null;
  market_id: string;
  oracle_id: string;
  underlying: string;
  raw_prompt: string;
  strategy_template: string;
  backend_strategy_id: string;
  total_premium: number;
  max_loss: number;
  max_payout: number;
  status: IntentPositionStatus;
  created_at_ms: number;
  updated_at_ms: number;
  warnings: string[];
};

async function readJsonResponse<T>(
  endpoint: string,
  response: Response,
): Promise<T> {
  const text = await response.text();
  if (!text.trim()) {
    const missingRouteMessage =
      response.status === 404
        ? `${endpoint} is not available on the running API server. Restart structx-api so it picks up the latest routes.`
        : `${endpoint} returned an empty response. Restart the API and retry.`;
    throw new ApiError(
      missingRouteMessage,
      {
        ok: false,
        code: response.status === 404 ? "API_ROUTE_MISSING" : "API_UNAVAILABLE",
        message:
          response.status === 404 ? "Route not found on running API" : "Empty response",
      },
      endpoint,
      response.status,
    );
  }

  let json: unknown;
  try {
    json = JSON.parse(text);
  } catch (err) {
    throw new ApiError(
      `${endpoint} returned non-JSON: ${(err as Error).message}`,
      { ok: false, code: "API_UNAVAILABLE", message: "Non-JSON response" },
      endpoint,
      response.status,
    );
  }

  if (!response.ok) {
    const body = json as ApiErrorBody;
    const message =
      body?.message ?? body?.error ?? body?.stderr ?? `${endpoint} failed`;
    throw new ApiError(message, body, endpoint, response.status);
  }

  if (
    typeof json === "object" &&
    json !== null &&
    "ok" in json &&
    (json as { ok: unknown }).ok === false
  ) {
    const body = json as ApiErrorBody;
    const message =
      body?.message ?? body?.error ?? body?.stderr ?? `${endpoint} returned ok=false`;
    throw new ApiError(message, body, endpoint, response.status);
  }

  return json as T;
}

async function postJson<T>(
  endpoint: string,
  payload: unknown,
  opts: FetchOpts = {},
): Promise<T> {
  const response = await fetch(`${API_BASE}${endpoint}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(payload),
    signal: opts.signal,
  });
  return readJsonResponse<T>(endpoint, response);
}

async function getJson<T>(
  endpoint: string,
  opts: FetchOpts = {},
): Promise<T> {
  const response = await fetch(`${API_BASE}${endpoint}`, {
    method: "GET",
    signal: opts.signal,
  });
  return readJsonResponse<T>(endpoint, response);
}

export async function compileStrategy(
  input: CompileInput,
  opts: FetchOpts = {},
): Promise<CompileResponse> {
  return postJson<CompileResponse>("/api/strategies/compile", input, opts);
}

export async function parseIntent(
  input: ParseIntentInput,
  opts: FetchOpts = {},
): Promise<ParsedIntentResponse> {
  return postJson<ParsedIntentResponse>("/api/intent/parse", input, opts);
}

export async function planFromIntent(
  input: {
    userAddress?: string;
    prompt: string;
    budget?: number;
    quoteAsset?: string;
    riskStyle?: RiskStyle;
  },
  opts: FetchOpts = {},
): Promise<IntentPlanningResponse> {
  return postJson<IntentPlanningResponse>(
    "/api/intent/plan",
    {
      user_address: input.userAddress,
      prompt: input.prompt,
      budget: input.budget,
      quote_asset: input.quoteAsset,
      risk_style: input.riskStyle,
    },
    opts,
  );
}

export async function quoteIntentPlan(
  input: {
    userAddress?: string;
    intentPlan: IntentPlan;
    selectedMarketId?: string;
    budget?: number;
    maxQuoteAgeMs?: number;
  },
  opts: FetchOpts = {},
): Promise<ExecutionProposal> {
  return postJson<ExecutionProposal>(
    "/api/intent/quote",
    {
      user_address: input.userAddress,
      intent_plan: input.intentPlan,
      selected_market_id: input.selectedMarketId,
      budget: input.budget,
      max_quote_age_ms: input.maxQuoteAgeMs,
    },
    opts,
  );
}

export async function auditIntentExecution(
  input: {
    proposalId: string;
    txDigest: string;
    userAddress?: string;
    managerId?: string;
    executionResult?: unknown;
  },
  opts: FetchOpts = {},
): Promise<AuditIntentExecutionResponse> {
  return postJson<AuditIntentExecutionResponse>(
    "/api/intent/audit-execution",
    {
      proposal_id: input.proposalId,
      tx_digest: input.txDigest,
      user_address: input.userAddress,
      manager_id: input.managerId,
      execution_result: input.executionResult,
    },
    opts,
  );
}

export async function buildIntentExecutePlan(
  input: {
    proposalId: string;
    userAddress?: string;
  },
  opts: FetchOpts = {},
): Promise<IntentExecutePlanResponse> {
  return postJson<IntentExecutePlanResponse>(
    "/api/intent/execute-plan",
    {
      proposal_id: input.proposalId,
      user_address: input.userAddress,
    },
    opts,
  );
}

export async function getIntentAuditByProposal(
  proposalId: string,
  opts: FetchOpts = {},
): Promise<IntentExecutionAudit> {
  return getJson<IntentExecutionAudit>(
    `/api/intent/audits/proposal/${encodeURIComponent(proposalId)}`,
    opts,
  );
}

export async function listRecentIntentAudits(
  max = 25,
  opts: FetchOpts = {},
): Promise<IntentExecutionAudit[]> {
  return getJson<IntentExecutionAudit[]>(
    `/api/intent/audits/recent?max=${encodeURIComponent(String(max))}`,
    opts,
  );
}

export async function listIntentPositions(
  input: {
    userAddress?: string;
    max?: number;
  } = {},
  opts: FetchOpts = {},
): Promise<IntentPositionSummary[]> {
  const params = new URLSearchParams();
  if (input.userAddress) params.set("user_address", input.userAddress);
  if (input.max) params.set("max", String(input.max));
  const qs = params.toString();
  return getJson<IntentPositionSummary[]>(
    `/api/intent/positions${qs ? `?${qs}` : ""}`,
    opts,
  );
}

export async function compileFromIntent(
  input: CompileFromIntentInput,
  opts: FetchOpts = {},
): Promise<GuidedCompileResponse> {
  return postJson<GuidedCompileResponse>(
    "/api/strategies/compile-from-intent",
    input,
    opts,
  );
}

export async function getMarketCatalogStatus(
  opts: FetchOpts = {},
): Promise<MarketCatalogStatusResponse> {
  return getJson<MarketCatalogStatusResponse>("/api/markets/catalog/status", opts);
}

export async function refreshMarketCatalog(
  opts: FetchOpts = {},
): Promise<MarketCatalogRefreshResponse> {
  const response = await fetch(`${API_BASE}/api/markets/catalog/refresh`, {
    method: "POST",
    signal: opts.signal,
  });
  return readJsonResponse<MarketCatalogRefreshResponse>(
    "/api/markets/catalog/refresh",
    response,
  );
}

export async function searchMarketCatalog(
  params: {
    q?: string;
    quoteAsset?: string;
    requireActive?: boolean;
    category?: CatalogMarketCategory;
    kind?: CatalogMarketKind;
    expiry?: "nearest_active" | "soonest" | "latest" | "any";
  },
  opts: FetchOpts = {},
): Promise<CatalogMarketSnapshot[]> {
  const search = new URLSearchParams();

  if (params.q) search.set("q", params.q);
  if (params.quoteAsset) search.set("quote_asset", params.quoteAsset);
  if (params.requireActive !== undefined) {
    search.set("require_active", String(params.requireActive));
  }
  if (params.category) search.set("category", params.category);
  if (params.kind) search.set("kind", params.kind);
  if (params.expiry) search.set("expiry", params.expiry);

  const qs = search.toString();
  return getJson<CatalogMarketSnapshot[]>(
    `/api/markets/search${qs ? `?${qs}` : ""}`,
    opts,
  );
}

export async function getCatalogMarket(
  marketId: string,
  opts: FetchOpts = {},
): Promise<CatalogMarketSnapshot> {
  return getJson<CatalogMarketSnapshot>(
    `/api/markets/catalog/${encodeURIComponent(marketId)}`,
    opts,
  );
}

/**
 * Read manager balance. Cached for {@link BALANCE_TTL_MS} per manager id; if
 * multiple components on the same page ask, only one round-trip happens.
 * Call {@link invalidateManagerBalance} after a mint/redeem to force fresh.
 */
export async function getManagerBalance(
  managerId: string,
  opts: FetchOpts = {},
): Promise<ManagerBalanceResponse> {
  return cachedFetch(NS_BALANCE, managerId, BALANCE_TTL_MS, () =>
    postJson<ManagerBalanceResponse>(
      "/api/manager-balance-json",
      { manager_id: managerId },
      opts,
    ),
  );
}

export function invalidateManagerBalance(managerId?: string): void {
  invalidate(NS_BALANCE, managerId);
}

export async function buildOpenStrategy(
  input: BuildOpenInput,
  opts: FetchOpts = {},
): Promise<BuildOpenStrategyResponse> {
  return postJson<BuildOpenStrategyResponse>(
    "/api/tx/build-open-strategy",
    input,
    opts,
  );
}

export async function auditOpenStrategy(
  input: AuditInput,
  opts: FetchOpts = {},
): Promise<AuditResponse> {
  return postJson<AuditResponse>("/api/tx/audit-open-strategy", input, opts);
}

// ============================================================================
// Positions — disk-backed ledger of every position the user has opened via
// StructX. Read endpoint is fast (in-process), sync endpoint re-walks the
// persisted audit records.
// ============================================================================

export type PositionStatus = "open" | "closed";
export type LegKind = "DOWN" | "UP" | "RANGE";

export type PositionRecord = {
  positionId: string;
  status: PositionStatus;
  strategy?: string | null;
  sourceDigest: string;
  openedAtUnix: number;
  oracleId: string;
  expiryMs: string;
  kind: LegKind;
  direction?: string | null;
  strikeRaw?: string | null;
  lowerRaw?: string | null;
  upperRaw?: string | null;
  originalQuantityRaw: string;
  remainingQuantityRaw: string;
  premiumPaidRaw: string;
  realizedPayoutRaw: string;
  realizedPnlRaw: string;
  lastPreviewPayoutRaw: string;
  lastPreviewPnlRaw: string;
  lastPreviewAtUnix: number;
  metadata?: Record<string, unknown> | null;
};

export type PositionsSummary = {
  openCount: number;
  closedCount: number;
  totalPremiumPaidRaw: string;
  totalEstimatedRedeemRaw: string;
  totalUnrealizedPnlRaw: string;
  totalRealizedPnlRaw: string;
  earliestExpiryMs?: string;
};

export type PositionsResponse = {
  ok: boolean;
  owner: string;
  managerId: string;
  positions: PositionRecord[];
  summary: PositionsSummary;
  auditDigests?: string[];
  redeemDigests?: string[];
  updatedAtUnix?: number;
  warnings?: string[];
};

export async function listPositions(
  args: { owner: string; managerId: string },
  opts: FetchOpts = {},
): Promise<PositionsResponse> {
  const qs = new URLSearchParams({
    owner: args.owner,
    managerId: args.managerId,
  }).toString();
  return getJson<PositionsResponse>(`/api/positions?${qs}`, opts);
}

export async function syncPositionsFromAudits(
  args: { owner: string; managerId: string },
  opts: FetchOpts = {},
): Promise<PositionsResponse & { appliedLegs?: number }> {
  return postJson<PositionsResponse & { appliedLegs?: number }>(
    "/api/positions/sync-from-audits",
    args,
    opts,
  );
}

export type SyncFromChainInput = {
  owner: string;
  managerId: string;
  mintedLegs: Array<{
    kind: "DOWN" | "UP" | "RANGE";
    direction?: string;
    oracleId: string;
    expiryMs: string;
    strikeRaw?: string;
    lowerRaw?: string;
    upperRaw?: string;
    quantityRaw: string;
    costRaw: string;
    sourceDigest: string;
    openedAtUnix?: number;
    strategy?: string;
  }>;
  redeemedLegs: Array<{
    kind: "DOWN" | "UP" | "RANGE";
    oracleId: string;
    expiryMs: string;
    strikeRaw?: string;
    lowerRaw?: string;
    upperRaw?: string;
    quantityRaw: string;
    payoutRaw: string;
    sourceDigest: string;
  }>;
};

export async function syncPositionsFromChain(
  input: SyncFromChainInput,
  opts: FetchOpts = {},
): Promise<
  PositionsResponse & { appliedMints?: number; appliedRedeems?: number }
> {
  return postJson<
    PositionsResponse & { appliedMints?: number; appliedRedeems?: number }
  >("/api/positions/sync-from-chain", input, opts);
}

export type AuditRedeemInput = {
  owner: string;
  managerId: string;
  positionId: string;
  digest: string;
  effects: unknown;
  events: unknown[];
  objectChanges: unknown[];
};

export type AuditRedeemResponse = {
  ok: boolean;
  digest: string;
  explorerUrl?: string;
  executionStatus?: string;
  managerId: string;
  positionId: string;
  updatedPosition?: PositionRecord | null;
  summary?: PositionsSummary;
  redeemedLegs?: Array<{
    kind: LegKind;
    oracleId: string;
    expiryMs: string;
    quantityRaw: string;
    payoutRaw: string;
  }>;
  warnings?: string[];
  error?: string;
};

export async function auditRedeemPosition(
  input: AuditRedeemInput,
  opts: FetchOpts = {},
): Promise<AuditRedeemResponse> {
  return postJson<AuditRedeemResponse>(
    "/api/tx/audit-redeem-position",
    input,
    opts,
  );
}

export type StoredManagerResponse = {
  ok: boolean;
  address: string;
  managerId?: string;
};

function normalizeAddress(address: string): string {
  const trimmed = address.trim().toLowerCase();
  return trimmed.startsWith("0x") ? trimmed : `0x${trimmed}`;
}

/**
 * Look up the PredictManager id previously persisted for `address` in the
 * backend JSON store. Returns `null` if nothing has ever been saved for this
 * wallet, or if the backend can't be reached (we never want a flaky backend
 * to block the auto-create path — the caller falls through to on-chain
 * discovery + create).
 *
 * Cached for {@link MANAGER_TTL_MS} per address + in-flight deduped, so the
 * common "user navigates between strategy pages" path is zero network.
 */
export async function getStoredManager(
  address: string,
  opts: FetchOpts = {},
): Promise<string | null> {
  const key = normalizeAddress(address);
  return cachedFetch(NS_MANAGER, key, MANAGER_TTL_MS, async () => {
    try {
      const body = await getJson<StoredManagerResponse>(
        `/api/managers/${encodeURIComponent(key)}`,
        opts,
      );
      return body.managerId ?? null;
    } catch (err) {
      // Don't cache failures: re-throwing would let cachedFetch evict, so the
      // next call retries. But we never want this to surface as a UI error —
      // a missing/flaky backend should fall through to on-chain discovery.
      if (isAbortError(err)) throw err;
      return null;
    }
  });
}

/**
 * Persist `managerId` for `address` in the backend JSON store. Best-effort: if
 * persistence fails, callers still hold the manager id in memory + localStorage
 * for the current session, so the user never gets stuck.
 *
 * On success we seed the read cache so the next {@link getStoredManager} for
 * this address skips the network entirely.
 */
export async function saveStoredManager(
  address: string,
  managerId: string,
  opts: FetchOpts = {},
): Promise<boolean> {
  const key = normalizeAddress(address);
  try {
    const r = await fetch(
      `${API_BASE}/api/managers/${encodeURIComponent(key)}`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ managerId }),
        signal: opts.signal,
      },
    );
    if (r.ok) {
      seed<string | null>(NS_MANAGER, key, managerId, MANAGER_TTL_MS);
      return true;
    }
    return false;
  } catch (err) {
    if (isAbortError(err)) throw err;
    return false;
  }
}

export function invalidateStoredManager(address?: string): void {
  invalidate(NS_MANAGER, address ? normalizeAddress(address) : undefined);
}
