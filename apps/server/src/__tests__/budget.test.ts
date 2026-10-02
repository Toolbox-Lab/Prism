import { describe, it, expect, vi, beforeEach } from "vitest";

// ---------------------------------------------------------------------------
// Mock Redis
// ---------------------------------------------------------------------------
const counters = new Map<string, number>();
const expiries = new Map<string, number>();

vi.mock("../redis", () => ({
  incrBy: vi.fn(async (key: string, amount: number) => {
    const current = counters.get(key) ?? 0;
    const next = current + amount;
    counters.set(key, next);
    return next;
  }),
  decrBy: vi.fn(async (key: string, amount: number) => {
    const current = counters.get(key) ?? 0;
    const next = current - amount;
    counters.set(key, next);
    return next;
  }),
  getNumber: vi.fn(async (key: string) => {
    return counters.get(key) ?? 0;
  }),
  setExpiry: vi.fn(async (key: string, ttl: number) => {
    expiries.set(key, ttl);
  }),
}));

// Mock config with low limits for easier testing
vi.mock("../config", () => ({
  config: {
    budget: {
      hourlyTokenLimit: 1000,
      dailyTokenLimit: 5000,
      maxConcurrentRequests: 2,
    },
  },
}));

const {
  assertBudgetAvailable,
  getBudgetStatus,
  recordUsage,
  acquireConcurrency,
  releaseConcurrency,
} = await import("../budget");

const { BudgetExceededError, ConcurrencyLimitError } =
  await import("../budget/errors");

describe("Budget — status checks", () => {
  beforeEach(() => {
    counters.clear();
    expiries.clear();
  });

  it("returns allowed=true when under all limits", async () => {
    const status = await getBudgetStatus();
    expect(status.allowed).toBe(true);
    expect(status.hourly_remaining).toBe(1000);
    expect(status.daily_remaining).toBe(5000);
    expect(status.concurrent_active).toBe(0);
  });

  it("returns allowed=false when hourly limit is exceeded", async () => {
    counters.set("grat:budget:hourly", 1001);
    const status = await getBudgetStatus();
    expect(status.allowed).toBe(false);
    expect(status.hourly_remaining).toBe(0);
  });

  it("returns allowed=false when daily limit is exceeded", async () => {
    counters.set("grat:budget:daily", 5001);
    const status = await getBudgetStatus();
    expect(status.allowed).toBe(false);
    expect(status.daily_remaining).toBe(0);
  });

  it("returns allowed=false when concurrency limit is met", async () => {
    counters.set("grat:budget:concurrent", 2);
    const status = await getBudgetStatus();
    expect(status.allowed).toBe(false);
  });
});

describe("Budget — assertBudgetAvailable", () => {
  beforeEach(() => {
    counters.clear();
    expiries.clear();
  });

  it("does not throw when under limits", async () => {
    await expect(assertBudgetAvailable()).resolves.toBeUndefined();
  });

  it("throws BudgetExceededError when hourly limit is exceeded", async () => {
    counters.set("grat:budget:hourly", 1500);
    await expect(assertBudgetAvailable()).rejects.toThrow(BudgetExceededError);
  });

  it("throws BudgetExceededError when daily limit is exceeded", async () => {
    counters.set("grat:budget:daily", 6000);
    await expect(assertBudgetAvailable()).rejects.toThrow(BudgetExceededError);
  });

  it("throws ConcurrencyLimitError when at max concurrency", async () => {
    counters.set("grat:budget:concurrent", 2);
    await expect(assertBudgetAvailable()).rejects.toThrow(
      ConcurrencyLimitError,
    );
  });
});

describe("Budget — recording usage", () => {
  beforeEach(() => {
    counters.clear();
    expiries.clear();
  });

  it("increments hourly and daily counters", async () => {
    await recordUsage(150);
    expect(counters.get("grat:budget:hourly")).toBe(150);
    expect(counters.get("grat:budget:daily")).toBe(150);
  });

  it("accumulates across multiple calls", async () => {
    await recordUsage(100);
    await recordUsage(200);
    expect(counters.get("grat:budget:hourly")).toBe(300);
    expect(counters.get("grat:budget:daily")).toBe(300);
  });

  it("sets expiry on first write", async () => {
    await recordUsage(50);
    expect(expiries.has("grat:budget:hourly")).toBe(true);
    expect(expiries.has("grat:budget:daily")).toBe(true);
  });
});

describe("Budget — concurrency semaphore", () => {
  beforeEach(() => {
    counters.clear();
  });

  it("acquireConcurrency increments the counter", async () => {
    const val = await acquireConcurrency();
    expect(val).toBe(1);
  });

  it("releaseConcurrency decrements the counter", async () => {
    counters.set("grat:budget:concurrent", 3);
    const val = await releaseConcurrency();
    expect(val).toBe(2);
  });

  it("releaseConcurrency floors at 0", async () => {
    counters.set("grat:budget:concurrent", 0);
    const val = await releaseConcurrency();
    expect(val).toBe(0);
  });
});
