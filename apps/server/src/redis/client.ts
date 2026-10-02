import Redis from "ioredis";
import { config } from "../config";

let instance: Redis | null = null;

/**
 * Returns a singleton Redis client.
 * Connection is lazy — created on first call.
 */
export function getRedis(): Redis {
  if (!instance) {
    instance = new Redis(config.redisUrl, {
      maxRetriesPerRequest: 3,
      lazyConnect: false,
    });
  }
  return instance;
}

/**
 * Gracefully close the Redis connection (for tests / shutdown).
 */
export async function closeRedis(): Promise<void> {
  if (instance) {
    await instance.quit();
    instance = null;
  }
}

// ---------------------------------------------------------------------------
// Typed helper functions
// ---------------------------------------------------------------------------

/**
 * SET with optional TTL (seconds). Returns true if the key was set.
 */
export async function cacheSet(
  key: string,
  value: string,
  ttlSeconds?: number,
): Promise<boolean> {
  const redis = getRedis();
  if (ttlSeconds && ttlSeconds > 0) {
    const result = await redis.set(key, value, "EX", ttlSeconds);
    return result === "OK";
  }
  const result = await redis.set(key, value);
  return result === "OK";
}

/**
 * GET — returns null on miss.
 */
export async function cacheGet(key: string): Promise<string | null> {
  return getRedis().get(key);
}

/**
 * DEL — returns number of keys removed.
 */
export async function cacheDel(...keys: string[]): Promise<number> {
  if (keys.length === 0) return 0;
  return getRedis().del(...keys);
}

/**
 * SET NX PX — atomic "set if not exists" with millisecond expiry.
 * Returns true if the lock was acquired (key didn't exist).
 */
export async function acquireLock(
  key: string,
  value: string,
  ttlMs: number,
): Promise<boolean> {
  const result = await getRedis().set(key, value, "PX", ttlMs, "NX");
  return result === "OK";
}

/**
 * INCRBY and return new value.
 */
export async function incrBy(key: string, amount: number): Promise<number> {
  return getRedis().incrby(key, amount);
}

/**
 * DECRBY and return new value.
 */
export async function decrBy(key: string, amount: number): Promise<number> {
  return getRedis().decrby(key, amount);
}

/**
 * GET as number (returns 0 on miss).
 */
export async function getNumber(key: string): Promise<number> {
  const val = await getRedis().get(key);
  return val ? Number(val) : 0;
}

/**
 * Set expiry on existing key (seconds).
 */
export async function setExpiry(
  key: string,
  ttlSeconds: number,
): Promise<void> {
  await getRedis().expire(key, ttlSeconds);
}

/**
 * Find all keys matching a glob pattern. Use sparingly.
 */
export async function scanKeys(pattern: string): Promise<string[]> {
  const redis = getRedis();
  const keys: string[] = [];
  let cursor = "0";

  do {
    const [nextCursor, batch] = await redis.scan(
      cursor,
      "MATCH",
      pattern,
      "COUNT",
      200,
    );
    cursor = nextCursor;
    keys.push(...batch);
  } while (cursor !== "0");

  return keys;
}
