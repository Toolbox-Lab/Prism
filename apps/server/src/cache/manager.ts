import type { CacheEntryType } from "../ai/types";
import { cacheSet, cacheGet, cacheDel, scanKeys } from "../redis";
import { PROMPT_VERSION_HASH } from "../ai/prompts";
import { config } from "../config";

// ---------------------------------------------------------------------------
// Key schema: grat:cache:{type}:{promptVersion}:{fingerprint}
// ---------------------------------------------------------------------------

function cacheKey(type: CacheEntryType, fingerprint: string): string {
  return `grat:cache:${type}:${PROMPT_VERSION_HASH}:${fingerprint}`;
}

/**
 * Derive a partial key pattern that matches all prompt-versions for a given
 * type + fingerprint.  Used for version-independent invalidation.
 */
function allVersionsPattern(type: CacheEntryType, fingerprint: string): string {
  return `grat:cache:${type}:*:${fingerprint}`;
}

// ---------------------------------------------------------------------------
// TTL resolution
// ---------------------------------------------------------------------------
function ttlFor(type: CacheEntryType): number {
  switch (type) {
    case "decode":
    case "explain":
      return config.cache.decodeTtl;
    case "profile":
      return config.cache.profileTtl;
    default:
      return config.cache.decodeTtl;
  }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Store a value in the cache with the appropriate TTL and prompt-version tag.
 */
export async function setCache(
  type: CacheEntryType,
  fingerprint: string,
  value: string,
): Promise<void> {
  const key = cacheKey(type, fingerprint);
  await cacheSet(key, value, ttlFor(type));
}

/**
 * Retrieve a cached value.  Returns `null` on miss (including version mismatch).
 */
export async function getCache(
  type: CacheEntryType,
  fingerprint: string,
): Promise<string | null> {
  const key = cacheKey(type, fingerprint);
  return cacheGet(key);
}

/**
 * Invalidate all cached entries for a specific transaction hash.
 *
 * Because we fingerprint on `txHash + network + requestType`, we need to scan
 * for keys matching the txHash across all types.  This is intentionally broad.
 */
export async function invalidateByTxHash(txHash: string): Promise<number> {
  const pattern = `grat:cache:*:*:*${txHash.toLowerCase()}*`;
  const keys = await scanKeys(pattern);
  if (keys.length === 0) return 0;
  return cacheDel(...keys);
}

/**
 * Invalidate a specific cache entry across all prompt versions.
 */
export async function invalidateEntry(
  type: CacheEntryType,
  fingerprint: string,
): Promise<number> {
  const pattern = allVersionsPattern(type, fingerprint);
  const keys = await scanKeys(pattern);
  if (keys.length === 0) return 0;
  return cacheDel(...keys);
}

/**
 * Clear all cached entries of a given type.
 */
export async function clearByType(type: CacheEntryType): Promise<number> {
  const pattern = `grat:cache:${type}:*`;
  const keys = await scanKeys(pattern);
  if (keys.length === 0) return 0;
  return cacheDel(...keys);
}

/**
 * Clear the entire AI cache.
 */
export async function clearAll(): Promise<number> {
  const pattern = "grat:cache:*";
  const keys = await scanKeys(pattern);
  if (keys.length === 0) return 0;
  return cacheDel(...keys);
}
