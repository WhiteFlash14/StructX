// Tiny in-memory cache for API client calls. Two jobs:
//   1. Time-to-live (TTL) — hold a fresh value for `ttlMs` so the same request
//      from a different component (or after a quick navigation) is a no-op.
//   2. In-flight dedup — if two callers fire the same key at the same time,
//      they share a single underlying fetch instead of both round-tripping.
//
// Deliberately tiny: no LRU, no persistence, no SWR. The point is to make
// fast paths actually fast. We don't try to compete with a real query lib.
//
// All entries are scoped by `namespace` so unrelated callers can't collide on
// the same key — `managers:0x..` won't be served from `balance:0x..` etc.

type Entry<T> = {
  value: T;
  expiresAt: number;
  promise?: Promise<T>;
};

const stores = new Map<string, Map<string, Entry<unknown>>>();

function storeFor<T>(namespace: string): Map<string, Entry<T>> {
  let s = stores.get(namespace);
  if (!s) {
    s = new Map();
    stores.set(namespace, s);
  }
  return s as Map<string, Entry<T>>;
}

/**
 * Returns a cached value if fresh, else invokes `fetcher` once and caches the
 * result. Concurrent callers with the same `key` share the same in-flight
 * promise (dedup) — exactly one network round-trip happens.
 *
 * On fetch error, the cache entry is removed so the next call retries.
 */
export async function cachedFetch<T>(
  namespace: string,
  key: string,
  ttlMs: number,
  fetcher: () => Promise<T>,
): Promise<T> {
  const s = storeFor<T>(namespace);
  const now = Date.now();
  const hit = s.get(key);
  if (hit) {
    if (hit.promise) return hit.promise; // dedup in-flight
    if (hit.expiresAt > now) return hit.value;
  }

  const promise = fetcher().then(
    (value) => {
      s.set(key, { value, expiresAt: Date.now() + ttlMs });
      return value;
    },
    (err) => {
      s.delete(key);
      throw err;
    },
  );

  // Stash the promise so concurrent callers dedup before it resolves. We
  // intentionally keep `value: undefined` here — readers will hit `promise`
  // first and never see this placeholder.
  s.set(key, { value: undefined as unknown as T, expiresAt: 0, promise });
  return promise;
}

/** Drop a cached entry (e.g. after a mutation). Without `key`, clears the
 * whole namespace. */
export function invalidate(namespace: string, key?: string): void {
  const s = stores.get(namespace);
  if (!s) return;
  if (key) s.delete(key);
  else s.clear();
}

/** Write a known-good value into the cache without a fetch (e.g. after we
 * just received it via another path). */
export function seed<T>(namespace: string, key: string, value: T, ttlMs: number): void {
  const s = storeFor<T>(namespace);
  s.set(key, { value, expiresAt: Date.now() + ttlMs });
}
