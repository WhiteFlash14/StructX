import type { PortfolioTradeRecord } from "@/types/structx";

const MAX_RECORDS = 36;

function storageKey(address: string): string {
  return `structx.portfolio.${address.toLowerCase()}`;
}

function safeSort(records: PortfolioTradeRecord[]): PortfolioTradeRecord[] {
  return [...records].sort((a, b) => {
    const left = Date.parse(a.openedAt);
    const right = Date.parse(b.openedAt);
    return (Number.isFinite(right) ? right : 0) - (Number.isFinite(left) ? left : 0);
  });
}

export function readPortfolioHistory(address: string | null | undefined): PortfolioTradeRecord[] {
  if (!address || typeof window === "undefined") return [];

  try {
    const raw = window.localStorage.getItem(storageKey(address));
    if (!raw) return [];
    const parsed = JSON.parse(raw) as PortfolioTradeRecord[];
    if (!Array.isArray(parsed)) return [];
    return safeSort(parsed);
  } catch {
    return [];
  }
}

export function writePortfolioHistory(
  address: string | null | undefined,
  records: PortfolioTradeRecord[],
): PortfolioTradeRecord[] {
  if (!address || typeof window === "undefined") return records;

  const next = safeSort(records).slice(0, MAX_RECORDS);

  try {
    window.localStorage.setItem(storageKey(address), JSON.stringify(next));
  } catch {
    // localStorage may be unavailable; keep the in-memory value anyway.
  }

  return next;
}

export function appendPortfolioHistory(
  address: string | null | undefined,
  record: PortfolioTradeRecord,
): PortfolioTradeRecord[] {
  const existing = readPortfolioHistory(address);
  const deduped = existing.filter((item) => item.digest !== record.digest);
  return writePortfolioHistory(address, [record, ...deduped]);
}
