// Light-weight formatting helpers used across StructX components.

export function shortAddress(address: string | null | undefined): string {
  if (!address) return "—";
  if (address.length <= 14) return address;
  return `${address.slice(0, 6)}…${address.slice(-6)}`;
}

export function formatDate(value: string | null | undefined): string {
  if (!value) return "—";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString();
}

export function copyToClipboard(value: string): Promise<void> {
  if (typeof navigator !== "undefined" && navigator.clipboard) {
    return navigator.clipboard.writeText(value);
  }
  return Promise.resolve();
}

export function bigIntSafe(value: string | undefined | null): bigint | null {
  if (value === undefined || value === null) return null;
  try {
    return BigInt(value);
  } catch {
    return null;
  }
}

export function formatNumberTwoDecimals(value: number): string {
  return new Intl.NumberFormat("en-US", {
    minimumFractionDigits: 0,
    maximumFractionDigits: 2,
  }).format(value);
}

export function formatCompactNumber(
  value: string | number | null | undefined,
): string {
  if (value === null || value === undefined || value === "") return "—";
  const numeric =
    typeof value === "number" ? value : Number.parseFloat(value.toString());
  if (!Number.isFinite(numeric)) return String(value);

  if (Math.abs(numeric) >= 1000) {
    return new Intl.NumberFormat("en-US", {
      notation: "compact",
      maximumFractionDigits: 2,
    }).format(numeric);
  }

  return formatNumberTwoDecimals(numeric);
}

export function formatPriceCompact(value: string | null | undefined): string {
  if (!value) return "—";
  const trimmed = value.trim();
  const numeric = Number.parseFloat(trimmed);
  if (!Number.isFinite(numeric)) return value;

  // DeepBook Predict raw price fields are e9-scaled integers (for example
  // "64530000000000" means 64,530.00). Normalize those before compacting so
  // position/redeem screens don't render fake trillions like "64.53T".
  const looksLikeRawE9 =
    /^-?\d+$/.test(trimmed) && Math.abs(numeric) >= 1_000_000_000;

  return formatCompactNumber(looksLikeRawE9 ? numeric / 1_000_000_000 : numeric);
}

export function formatDusdcDisplayString(
  value: string | null | undefined,
): string {
  if (!value) return "—";
  const numeric = Number.parseFloat(value.replace(/\s*dUSDC$/i, ""));
  if (!Number.isFinite(numeric)) return value;
  return `${formatNumberTwoDecimals(numeric)} dUSDC`;
}

// Format a stdout-derived dUSDC raw string ("1234567") to "1.23 dUSDC".
export function formatDusdcDisplay(raw: string | null | undefined): string {
  if (!raw) return "—";
  const big = bigIntSafe(raw);
  if (big === null) return raw;

  const sign = big < 0n ? "-" : "";
  const abs = big < 0n ? -big : big;
  const cents = (abs + 5_000n) / 10_000n;
  const whole = cents / 100n;
  const frac = (cents % 100n).toString().padStart(2, "0");

  return `${sign}${whole}.${frac} dUSDC`;
}

export function explorerTxUrl(digest: string): string {
  return `https://suiexplorer.com/txblock/${digest}?network=testnet`;
}

export function explorerObjectUrl(objectId: string): string {
  return `https://suiexplorer.com/object/${objectId}?network=testnet`;
}

export const ROLE_LABELS: Record<string, string> = {
  extreme_downside: "Extreme downside",
  moderate_downside: "Moderate downside",
  moderate_upside: "Moderate upside",
  extreme_upside: "Extreme upside",
  severe_downside: "Severe downside hedge",
  mild_downside: "Mild downside hedge",
  large_downside_move: "Large downside move",
  moderate_downside_move: "Moderate downside move",
  moderate_upside_move: "Moderate upside move",
  large_upside_move: "Large upside move",
  upside_breakout_zone: "Upside breakout zone",
  moonshot_tail: "Moonshot tail",
  near_upside_step: "Near upside step",
  upper_upside_step: "Upper upside step",
  upside_continuation_tail: "Upside continuation tail",
  downside_breakdown_zone: "Downside breakdown zone",
  crash_tail: "Crash tail",
  near_downside_step: "Near downside step",
  lower_downside_step: "Lower downside step",
  downside_continuation_tail: "Downside continuation tail",
  lower_outside_wing: "Lower outside wing",
  lower_center_band: "Lower center band",
  upper_center_band: "Upper center band",
  upper_outside_wing: "Upper outside wing",
  near_up_barrier_range: "Near up-barrier range",
  beyond_up_barrier_tail: "Beyond up-barrier tail",
  near_down_barrier_range: "Near down-barrier range",
  beyond_down_barrier_tail: "Beyond down-barrier tail",
};

export const KIND_LABELS: Record<string, string> = {
  DOWN: "Binary down",
  UP: "Binary up",
  RANGE: "Range",
};
