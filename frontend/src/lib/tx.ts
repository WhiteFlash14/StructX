// Transaction builder used by the frontend to construct the Sui PTB
// from the deterministic build response returned by the backend.

import { Transaction } from "@mysten/sui/transactions";

import type { BuildOpenStrategyResponse } from "@/types/structx";

export const PREDICT_PACKAGE_ID =
  "0xf5ea2b3749c65d6e56507cc35388719aadb28f9cab873696a2f8687f5c785138";
export const PREDICT_MANAGER_TYPE = `${PREDICT_PACKAGE_ID}::predict_manager::PredictManager`;
export const DUSDC_COIN_TYPE =
  "0xe95040085976bfd54a1a07225cd46c8a2b4e8e2b6732f140a0fc49850ba73e1a::dusdc::DUSDC";

function requireString(value: string | undefined, message: string): string {
  if (!value || value === "0") {
    throw new Error(message);
  }
  return value;
}

export function addSlippageBps(raw: bigint, slippageBps: number): bigint {
  if (!Number.isInteger(slippageBps) || slippageBps < 0 || slippageBps > 10_000) {
    throw new Error("Slippage must be a whole number between 0 and 10,000 bps.");
  }
  return (raw * BigInt(10_000 + slippageBps) + 9_999n) / 10_000n;
}

/**
 * The mint entrypoints do not accept a max-cost argument. Keeping only this
 * much dUSDC in the manager gives the combined transaction enough room for
 * the selected slippage while avoiding an exact-price deposit that can fail
 * as soon as the order book moves.
 */
export function requiredManagerReserveRaw(
  payload: BuildOpenStrategyResponse,
): bigint {
  if (payload.summary.legs.length === 0) {
    return BigInt(payload.summary.premiumRequiredRaw);
  }

  return payload.summary.legs.reduce((total, leg) => {
    const raw = leg.maxCostRaw ?? leg.premiumRaw;
    return total + BigInt(raw);
  }, 0n);
}

export function buildOpenStrategyTransaction(
  payload: BuildOpenStrategyResponse,
): Transaction {
  const tx = new Transaction();
  tx.setSender(payload.owner);

  for (const leg of payload.summary.legs) {
    const quantityRaw = requireString(leg.quantityRaw, "missing quantityRaw");

    if (leg.kind === "DOWN") {
      const strikeRaw = requireString(leg.strikeRaw, "missing DOWN strikeRaw");
      const key = tx.moveCall({
        target: `${payload.predictPackageId}::market_key::down`,
        arguments: [
          tx.pure.address(payload.oracleId),
          tx.pure.u64(payload.expiryMs),
          tx.pure.u64(strikeRaw),
        ],
      });
      tx.moveCall({
        target: `${payload.predictPackageId}::predict::mint`,
        typeArguments: [payload.dusdcCoinType],
        arguments: [
          tx.object(payload.predictObjectId),
          tx.object(payload.managerId),
          tx.object(payload.oracleId),
          key,
          tx.pure.u64(quantityRaw),
          tx.object(payload.clockObjectId),
        ],
      });
    } else if (leg.kind === "UP") {
      const strikeRaw = requireString(leg.strikeRaw, "missing UP strikeRaw");
      const key = tx.moveCall({
        target: `${payload.predictPackageId}::market_key::up`,
        arguments: [
          tx.pure.address(payload.oracleId),
          tx.pure.u64(payload.expiryMs),
          tx.pure.u64(strikeRaw),
        ],
      });
      tx.moveCall({
        target: `${payload.predictPackageId}::predict::mint`,
        typeArguments: [payload.dusdcCoinType],
        arguments: [
          tx.object(payload.predictObjectId),
          tx.object(payload.managerId),
          tx.object(payload.oracleId),
          key,
          tx.pure.u64(quantityRaw),
          tx.object(payload.clockObjectId),
        ],
      });
    } else if (leg.kind === "RANGE") {
      const lowerRaw = requireString(leg.lowerRaw, "missing RANGE lowerRaw");
      const upperRaw = requireString(leg.upperRaw, "missing RANGE upperRaw");
      const key = tx.moveCall({
        target: `${payload.predictPackageId}::range_key::new`,
        arguments: [
          tx.pure.address(payload.oracleId),
          tx.pure.u64(payload.expiryMs),
          tx.pure.u64(lowerRaw),
          tx.pure.u64(upperRaw),
        ],
      });
      tx.moveCall({
        target: `${payload.predictPackageId}::predict::mint_range`,
        typeArguments: [payload.dusdcCoinType],
        arguments: [
          tx.object(payload.predictObjectId),
          tx.object(payload.managerId),
          tx.object(payload.oracleId),
          key,
          tx.pure.u64(quantityRaw),
          tx.object(payload.clockObjectId),
        ],
      });
    }
  }

  tx.setGasBudget(500_000_000);
  return tx;
}

export function buildCreateManagerTransaction(owner: string): Transaction {
  const tx = new Transaction();
  tx.setSender(owner);
  tx.moveCall({
    target: `${PREDICT_PACKAGE_ID}::predict::create_manager`,
    arguments: [],
  });
  tx.setGasBudget(200_000_000);
  return tx;
}

// IMPORTANT: per the on-chain `predict::redeem` source, the payout is deposited
// back **into the PredictManager**, not the wallet. Surfacing this in the UI
// is the caller's responsibility. To move dUSDC out to the wallet the user
// must perform a separate manager withdraw (not yet implemented).
export type RedeemPositionArgs = {
  owner: string;
  managerId: string;
  oracleId: string;
  expiryMs: string;
  predictPackageId?: string;
  predictObjectId: string;
  clockObjectId: string;
  dusdcCoinType?: string;
  quantityRaw: string;
} & (
  | { kind: "DOWN" | "UP"; strikeRaw: string }
  | { kind: "RANGE"; lowerRaw: string; upperRaw: string }
);

const DEFAULT_PREDICT_OBJECT_ID =
  "0xc8736204d12f0a7277c86388a68bf8a194b0a14c5538ad13f22cbd8e2a38028a";
const DEFAULT_CLOCK_OBJECT_ID = "0x6";

/**
 * Build a PTB that redeems exactly one position. Used for both devInspect
 * preview and the wallet-signed close. Same shape as mint, with `redeem` /
 * `redeem_range` instead of `mint` / `mint_range`.
 */
export function buildRedeemPositionTransaction(args: RedeemPositionArgs): Transaction {
  const pkg = args.predictPackageId ?? PREDICT_PACKAGE_ID;
  const coinType = args.dusdcCoinType ?? DUSDC_COIN_TYPE;
  const predictObj = args.predictObjectId ?? DEFAULT_PREDICT_OBJECT_ID;
  const clockObj = args.clockObjectId ?? DEFAULT_CLOCK_OBJECT_ID;
  const quantity = requireString(args.quantityRaw, "missing quantityRaw");

  const tx = new Transaction();
  tx.setSender(args.owner);

  if (args.kind === "RANGE") {
    const lowerRaw = requireString(args.lowerRaw, "missing lowerRaw");
    const upperRaw = requireString(args.upperRaw, "missing upperRaw");
    const key = tx.moveCall({
      target: `${pkg}::range_key::new`,
      arguments: [
        tx.pure.address(args.oracleId),
        tx.pure.u64(args.expiryMs),
        tx.pure.u64(lowerRaw),
        tx.pure.u64(upperRaw),
      ],
    });
    tx.moveCall({
      target: `${pkg}::predict::redeem_range`,
      typeArguments: [coinType],
      arguments: [
        tx.object(predictObj),
        tx.object(args.managerId),
        tx.object(args.oracleId),
        key,
        tx.pure.u64(quantity),
        tx.object(clockObj),
      ],
    });
  } else {
    const strikeRaw = requireString(args.strikeRaw, "missing strikeRaw");
    const key = tx.moveCall({
      target: `${pkg}::market_key::${args.kind === "UP" ? "up" : "down"}`,
      arguments: [
        tx.pure.address(args.oracleId),
        tx.pure.u64(args.expiryMs),
        tx.pure.u64(strikeRaw),
      ],
    });
    tx.moveCall({
      target: `${pkg}::predict::redeem`,
      typeArguments: [coinType],
      arguments: [
        tx.object(predictObj),
        tx.object(args.managerId),
        tx.object(args.oracleId),
        key,
        tx.pure.u64(quantity),
        tx.object(clockObj),
      ],
    });
  }

  tx.setGasBudget(500_000_000);
  return tx;
}

/**
 * Pull `payout` out of the parsed PositionRedeemed / RangeRedeemed event in
 * a devInspect or executed result. Returns 0n if the event isn't present.
 */
export function readRedeemPayoutFromEvents(
  events: ReadonlyArray<{ type?: string; parsedJson?: unknown }>,
): { payoutRaw: bigint; isSettled: boolean } {
  for (const ev of events) {
    if (!ev.type) continue;
    if (
      ev.type.endsWith("::predict::PositionRedeemed") ||
      ev.type.endsWith("::predict::RangeRedeemed")
    ) {
      const parsed = ev.parsedJson as
        | { payout?: string | number; is_settled?: boolean }
        | undefined;
      const payout = parsed?.payout;
      try {
        return {
          payoutRaw:
            payout === undefined ? 0n : BigInt(String(payout)),
          isSettled: Boolean(parsed?.is_settled),
        };
      } catch {
        return { payoutRaw: 0n, isSettled: false };
      }
    }
  }
  return { payoutRaw: 0n, isSettled: false };
}

// Minimal shape we need from the Sui RPC client. We use a structural type
// instead of importing `SuiClient` because the import path / class name has
// moved across dApp Kit versions; this keeps the helper version-stable.
export type CoinSource = {
  getCoins(args: {
    owner: string;
    coinType: string;
    cursor?: string | null;
    limit?: number;
  }): Promise<{
    data: ReadonlyArray<{ coinObjectId: string; balance: string }>;
    hasNextPage: boolean;
    nextCursor?: string | null;
  }>;
};

/**
 * Sum (and return objectId list of) the user's dUSDC coin balance, for the
 * deposit pre-flight check. Returns raw (6-dec) balance.
 */
export async function fetchWalletDusdcBalance(
  client: CoinSource,
  owner: string,
  minimumRaw?: bigint,
): Promise<{ totalRaw: bigint; coinObjectIds: string[] }> {
  const out: string[] = [];
  let cursor: string | null | undefined = null;
  let total = 0n;
  const seenCursors = new Set<string>();

  while (true) {
    const page = await client.getCoins({
      owner,
      coinType: DUSDC_COIN_TYPE,
      cursor,
      limit: 50,
    });
    for (const coin of page.data) {
      out.push(coin.coinObjectId);
      try {
        total += BigInt(coin.balance);
      } catch {
        // ignore non-numeric balances
      }
      if (minimumRaw !== undefined && total >= minimumRaw) {
        return { totalRaw: total, coinObjectIds: out };
      }
    }
    if (!page.hasNextPage || !page.nextCursor) break;
    if (seenCursors.has(page.nextCursor)) {
      throw new Error("The Sui RPC repeated a dUSDC pagination cursor.");
    }
    seenCursors.add(page.nextCursor);
    cursor = page.nextCursor;
  }
  return { totalRaw: total, coinObjectIds: out };
}

/**
 * Build a single PTB that:
 *   1. If `depositRaw > 0`: merges the user's dUSDC coins, splits off exactly
 *      `depositRaw`, and calls `predict_manager::deposit<dUSDC>(manager, coin)`.
 *   2. Issues every mint / mint_range call from the backend's build response.
 *
 * This is the safe pattern: deposit and mint atomically in the same wallet
 * signature. If anything in the mint phase reverts, the deposit reverts too,
 * so the user never ends up with money parked in a manager but no positions.
 */
export function buildDepositAndOpenStrategyTransaction(args: {
  payload: BuildOpenStrategyResponse;
  depositRaw: bigint;
  walletDusdcCoinIds: string[];
}): Transaction {
  const { payload, depositRaw, walletDusdcCoinIds } = args;
  const tx = new Transaction();
  tx.setSender(payload.owner);

  // ----- Deposit phase -----
  if (depositRaw > 0n) {
    if (walletDusdcCoinIds.length === 0) {
      throw new Error(
        "No dUSDC coins in wallet. Get test dUSDC from the Sui Testnet faucet first.",
      );
    }
    const primary = tx.object(walletDusdcCoinIds[0]);
    if (walletDusdcCoinIds.length > 1) {
      tx.mergeCoins(
        primary,
        walletDusdcCoinIds.slice(1).map((id) => tx.object(id)),
      );
    }
    const [depositCoin] = tx.splitCoins(primary, [tx.pure.u64(depositRaw)]);
    tx.moveCall({
      target: `${payload.predictPackageId}::predict_manager::deposit`,
      typeArguments: [payload.dusdcCoinType],
      arguments: [tx.object(payload.managerId), depositCoin],
    });
  }

  // ----- Mint phase -----
  for (const leg of payload.summary.legs) {
    const quantityRaw = requireString(leg.quantityRaw, "missing quantityRaw");

    if (leg.kind === "DOWN") {
      const strikeRaw = requireString(leg.strikeRaw, "missing DOWN strikeRaw");
      const key = tx.moveCall({
        target: `${payload.predictPackageId}::market_key::down`,
        arguments: [
          tx.pure.address(payload.oracleId),
          tx.pure.u64(payload.expiryMs),
          tx.pure.u64(strikeRaw),
        ],
      });
      tx.moveCall({
        target: `${payload.predictPackageId}::predict::mint`,
        typeArguments: [payload.dusdcCoinType],
        arguments: [
          tx.object(payload.predictObjectId),
          tx.object(payload.managerId),
          tx.object(payload.oracleId),
          key,
          tx.pure.u64(quantityRaw),
          tx.object(payload.clockObjectId),
        ],
      });
    } else if (leg.kind === "UP") {
      const strikeRaw = requireString(leg.strikeRaw, "missing UP strikeRaw");
      const key = tx.moveCall({
        target: `${payload.predictPackageId}::market_key::up`,
        arguments: [
          tx.pure.address(payload.oracleId),
          tx.pure.u64(payload.expiryMs),
          tx.pure.u64(strikeRaw),
        ],
      });
      tx.moveCall({
        target: `${payload.predictPackageId}::predict::mint`,
        typeArguments: [payload.dusdcCoinType],
        arguments: [
          tx.object(payload.predictObjectId),
          tx.object(payload.managerId),
          tx.object(payload.oracleId),
          key,
          tx.pure.u64(quantityRaw),
          tx.object(payload.clockObjectId),
        ],
      });
    } else if (leg.kind === "RANGE") {
      const lowerRaw = requireString(leg.lowerRaw, "missing RANGE lowerRaw");
      const upperRaw = requireString(leg.upperRaw, "missing RANGE upperRaw");
      const key = tx.moveCall({
        target: `${payload.predictPackageId}::range_key::new`,
        arguments: [
          tx.pure.address(payload.oracleId),
          tx.pure.u64(payload.expiryMs),
          tx.pure.u64(lowerRaw),
          tx.pure.u64(upperRaw),
        ],
      });
      tx.moveCall({
        target: `${payload.predictPackageId}::predict::mint_range`,
        typeArguments: [payload.dusdcCoinType],
        arguments: [
          tx.object(payload.predictObjectId),
          tx.object(payload.managerId),
          tx.object(payload.oracleId),
          key,
          tx.pure.u64(quantityRaw),
          tx.object(payload.clockObjectId),
        ],
      });
    }
  }

  tx.setGasBudget(500_000_000);
  return tx;
}
