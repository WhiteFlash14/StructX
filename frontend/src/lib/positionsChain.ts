// Walk a wallet's Sui tx history for DeepBook Predict mint/redeem events
// that belong to a specific PredictManager. Returns the legs in a shape the
// backend sync-from-chain endpoint understands.
//
// Why client-side rather than backend: the backend can't reach the user's
// wallet RPC without bringing in a Sui client + caching layer. The frontend
// already has a typed SuiClient from dApp Kit, paginates fast, and returns
// only the events that match the user — far less data to ship over the wire.
//
// Scope: we cap at MAX_PAGES * PAGE_LIMIT transactions to avoid unbounded
// scans on prolific wallets. For typical StructX usage (a few mints/closes)
// this covers months of history. The dedup happens server-side via
// ledger.audit_digests / redeem_digests, so re-runs are safe.

import { PREDICT_PACKAGE_ID } from "@/lib/tx";

const PAGE_LIMIT = 50;
const MAX_PAGES = 6;

export type ChainMintedLeg = {
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
  openedAtUnix: number;
};

export type ChainRedeemedLeg = {
  kind: "DOWN" | "UP" | "RANGE";
  oracleId: string;
  expiryMs: string;
  strikeRaw?: string;
  lowerRaw?: string;
  upperRaw?: string;
  quantityRaw: string;
  payoutRaw: string;
  sourceDigest: string;
};

// Minimal client surface — same structural-typing trick as fetchWalletDusdcBalance.
// `FromAddress` is required because dApp Kit's SuiJsonRpcClient type also
// requires it.
type TxBlocksClient = {
  queryTransactionBlocks(args: {
    filter: { FromAddress: string };
    options?: { showEvents?: boolean; showEffects?: boolean };
    cursor?: string | null;
    limit?: number;
    order?: "ascending" | "descending";
  }): Promise<{
    data: Array<{
      digest: string;
      timestampMs?: string | null;
      effects?: {
        status?: { status?: string };
      } | null;
      events?: Array<{
        type?: string;
        parsedJson?: unknown;
      }> | null;
    }>;
    hasNextPage: boolean;
    nextCursor?: string | null;
  }>;
};

// Type-narrowing helper for `parsedJson` reads. Sui returns these as `unknown`
// because Move events vary across packages.
function pickString(obj: unknown, field: string): string | undefined {
  if (!obj || typeof obj !== "object") return undefined;
  const v = (obj as Record<string, unknown>)[field];
  if (typeof v === "string") return v;
  if (typeof v === "number") return String(v);
  if (typeof v === "bigint") return v.toString();
  return undefined;
}

function pickBool(obj: unknown, field: string): boolean {
  if (!obj || typeof obj !== "object") return false;
  return Boolean((obj as Record<string, unknown>)[field]);
}

/**
 * Walk this wallet's recent transactions and return every PositionMinted /
 * RangeMinted / PositionRedeemed / RangeRedeemed event whose `manager_id`
 * matches the supplied PredictManager id.
 */
export async function fetchOnChainPositionEvents(
  client: TxBlocksClient,
  owner: string,
  managerId: string,
  options: { signal?: AbortSignal } = {},
): Promise<{
  mintedLegs: ChainMintedLeg[];
  redeemedLegs: ChainRedeemedLeg[];
}> {
  const mintedLegs: ChainMintedLeg[] = [];
  const redeemedLegs: ChainRedeemedLeg[] = [];
  const targetManager = managerId.toLowerCase();
  let cursor: string | null | undefined = null;

  for (let page = 0; page < MAX_PAGES; page += 1) {
    if (options.signal?.aborted) break;
    const result = await client.queryTransactionBlocks({
      filter: { FromAddress: owner },
      options: { showEvents: true, showEffects: true },
      cursor,
      limit: PAGE_LIMIT,
      order: "descending",
    });

    for (const tx of result.data) {
      if (tx.effects?.status?.status !== "success") continue;
      const digest = tx.digest;
      const timestampMs = tx.timestampMs ? Number(tx.timestampMs) : 0;
      const openedAtUnix = timestampMs > 0 ? Math.floor(timestampMs / 1000) : 0;

      for (const ev of tx.events ?? []) {
        const type = ev.type ?? "";
        // Only care about predict package events.
        if (!type.includes(`${PREDICT_PACKAGE_ID}::predict::`)) continue;
        const parsed = ev.parsedJson;
        const eventManager = pickString(parsed, "manager_id");
        if (!eventManager || eventManager.toLowerCase() !== targetManager) {
          continue;
        }

        const oracleId = pickString(parsed, "oracle_id");
        const expiryMs = pickString(parsed, "expiry");
        const quantityRaw = pickString(parsed, "quantity");
        if (!oracleId || !expiryMs || !quantityRaw) continue;

        if (type.endsWith("::predict::PositionMinted")) {
          const isUp = pickBool(parsed, "is_up");
          mintedLegs.push({
            kind: isUp ? "UP" : "DOWN",
            direction: isUp ? "up" : "down",
            oracleId,
            expiryMs,
            strikeRaw: pickString(parsed, "strike"),
            quantityRaw,
            costRaw: pickString(parsed, "cost") ?? "0",
            sourceDigest: digest,
            openedAtUnix,
          });
        } else if (type.endsWith("::predict::RangeMinted")) {
          mintedLegs.push({
            kind: "RANGE",
            oracleId,
            expiryMs,
            lowerRaw: pickString(parsed, "lower_strike"),
            upperRaw: pickString(parsed, "higher_strike"),
            quantityRaw,
            costRaw: pickString(parsed, "cost") ?? "0",
            sourceDigest: digest,
            openedAtUnix,
          });
        } else if (type.endsWith("::predict::PositionRedeemed")) {
          const isUp = pickBool(parsed, "is_up");
          redeemedLegs.push({
            kind: isUp ? "UP" : "DOWN",
            oracleId,
            expiryMs,
            strikeRaw: pickString(parsed, "strike"),
            quantityRaw,
            payoutRaw: pickString(parsed, "payout") ?? "0",
            sourceDigest: digest,
          });
        } else if (type.endsWith("::predict::RangeRedeemed")) {
          redeemedLegs.push({
            kind: "RANGE",
            oracleId,
            expiryMs,
            lowerRaw: pickString(parsed, "lower_strike"),
            upperRaw: pickString(parsed, "higher_strike"),
            quantityRaw,
            payoutRaw: pickString(parsed, "payout") ?? "0",
            sourceDigest: digest,
          });
        }
      }
    }

    if (!result.hasNextPage || !result.nextCursor) break;
    cursor = result.nextCursor;
  }

  return { mintedLegs, redeemedLegs };
}
