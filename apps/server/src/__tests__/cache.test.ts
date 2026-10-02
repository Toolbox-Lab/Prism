import { describe, it, expect, vi, beforeEach } from "vitest";

// ---------------------------------------------------------------------------
// Mock Redis
// ---------------------------------------------------------------------------
const store = new Map<string, { value: string; expiry: number }>();

vi.mock("../redis", () => ({
  cacheSet: vi.fn(async (key: string, value: string, ttl?: number) => {
    store.set(key, {
      value,
      expiry: Date.now() + (ttl ?? 3600) * 1000,
    });
    return true;
  }),
  cacheGet: vi.fn(async (key: string) => {
    const entry = store.get(key);
    if (!entry) return null;
    if (Date.now() > entry.expiry) {
      store.delete(key);
      return null;
    }
    return entry.value;
  }),
  cacheDel: vi.fn(async (...keys: string[]) => {
    let count = 0;
    for (const k of keys) {
      if (store.delete(k)) count++;
    }
    return count;
  }),
  scanKeys: vi.fn(async (pattern: string) => {
    const regex = new RegExp(
      "^" + pattern.replace(/\*/g, ".*").replace(/\?/g, ".") + "$",
    );
    return Array.from(store.keys()).filter((k) => regex.test(k));
  }),
}));

// Mock config
vi.mock("../config", () => ({
  config: {
    cache: {
      decodeTtl: 86400,
      profileTtl: 3600,
    },
  },
}));

// Mock prompt version hash to a stable value
vi.mock("../ai/prompts", () => ({
  PROMPT_VERSION_HASH: "test12345678",
}));

const {
  setCache,
  getCache,
  invalidateByTxHash,
  invalidateEntry,
  clearByType,
  clearAll,
} = await import("../cache");

describe("Cache — basic operations", () => {
  beforeEach(() => {
    store.clear();
  });

  it("setCache + getCache round-trip", async () => {
    await setCache("explain", "fp-1", '{"summary":"hello"}');
    const result = await getCache("explain", "fp-1");
    expect(result).toBe('{"summary":"hello"}');
  });

  it("getCache returns null on miss", async () => {
    const result = await getCache("explain", "nonexistent");
    expect(result).toBeNull();
  });

  it("different types do not collide", async () => {
    await setCache("explain", "fp-1", '"explain-data"');
    await setCache("profile", "fp-1", '"profile-data"');

    expect(await getCache("explain", "fp-1")).toBe('"explain-data"');
    expect(await getCache("profile", "fp-1")).toBe('"profile-data"');
  });
});

describe("Cache — invalidation", () => {
  beforeEach(() => {
    store.clear();
  });

  it("invalidateEntry removes a specific entry across prompt versions", async () => {
    // Simulate entries from different prompt versions
    store.set("grat:cache:explain:v1hash:fp-a", {
      value: "old",
      expiry: Date.now() + 100_000,
    });
    store.set("grat:cache:explain:v2hash:fp-a", {
      value: "new",
      expiry: Date.now() + 100_000,
    });

    const removed = await invalidateEntry("explain", "fp-a");
    expect(removed).toBe(2);
    expect(store.size).toBe(0);
  });

  it("invalidateByTxHash removes entries containing the tx hash", async () => {
    const txHash = "abc123def";
    store.set(`grat:cache:explain:test12345678:${txHash}`, {
      value: "data",
      expiry: Date.now() + 100_000,
    });
    store.set(`grat:cache:profile:test12345678:other-fp`, {
      value: "keep",
      expiry: Date.now() + 100_000,
    });

    const removed = await invalidateByTxHash(txHash);
    expect(removed).toBe(1);
    expect(store.size).toBe(1);
  });

  it("clearByType removes all entries of a given type", async () => {
    store.set("grat:cache:explain:h:fp-1", {
      value: "a",
      expiry: Date.now() + 100_000,
    });
    store.set("grat:cache:explain:h:fp-2", {
      value: "b",
      expiry: Date.now() + 100_000,
    });
    store.set("grat:cache:profile:h:fp-3", {
      value: "c",
      expiry: Date.now() + 100_000,
    });

    const removed = await clearByType("explain");
    expect(removed).toBe(2);
    expect(store.size).toBe(1);
  });

  it("clearAll removes everything", async () => {
    store.set("grat:cache:explain:h:fp-1", {
      value: "a",
      expiry: Date.now() + 100_000,
    });
    store.set("grat:cache:profile:h:fp-2", {
      value: "b",
      expiry: Date.now() + 100_000,
    });

    const removed = await clearAll();
    expect(removed).toBe(2);
    expect(store.size).toBe(0);
  });
});

describe("Cache — version-based invalidation", () => {
  beforeEach(() => {
    store.clear();
  });

  it("cache miss on prompt version mismatch (different version tag in key)", async () => {
    // Write with current mock prompt version "test12345678"
    await setCache("explain", "fp-version-test", '"current"');

    // The key includes "test12345678", so getCache with the same mock will hit.
    expect(await getCache("explain", "fp-version-test")).toBe('"current"');

    // If the prompt version changed, the key would differ and this would be a miss.
    // We verify by directly checking the key contains the version hash.
    const keys = Array.from(store.keys());
    expect(keys[0]).toContain("test12345678");
  });
});
