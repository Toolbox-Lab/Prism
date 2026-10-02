import { config } from "../config";
import { incrBy, decrBy, getNumber, setExpiry } from "../redis";
import { BudgetExceededError, ConcurrencyLimitError } from "./errors";
import type { BudgetStatus } from "../ai/types";

// ---------------------------------------------------------------------------
// Redis key constants
// ---------------------------------------------------------------------------
const HOURLY_KEY = "grat:budget:hourly";
const DAILY_KEY = "grat:budget:daily";
const CONCURRENT_KEY = "grat:budget:concurrent";

// ---------------------------------------------------------------------------
// Time helpers
// ---------------------------------------------------------------------------

/** Seconds remaining until the top of the next hour. */
function secondsUntilNextHour(): number {
  const now = new Date();
  const next = new Date(now);
  next.setMinutes(0, 0, 0);
  next.setHours(next.getHours() + 1);
  return Math.ceil((next.getTime() - now.getTime()) / 1000);
}

/** Seconds remaining until midnight UTC. */
function secondsUntilMidnightUTC(): number {
  const now = new Date();
  const tomorrow = new Date(now);
  tomorrow.setUTCDate(tomorrow.getUTCDate() + 1);
  tomorrow.setUTCHours(0, 0, 0, 0);
  return Math.ceil((tomorrow.getTime() - now.getTime()) / 1000);
}

function hourlyResetDate(): Date {
  const d = new Date();
  d.setMinutes(0, 0, 0);
  d.setHours(d.getHours() + 1);
  return d;
}

function dailyResetDate(): Date {
  const d = new Date();
  d.setUTCDate(d.getUTCDate() + 1);
  d.setUTCHours(0, 0, 0, 0);
  return d;
}

// ---------------------------------------------------------------------------
// Budget enforcement
// ---------------------------------------------------------------------------

/**
 * Pre-flight check: is a new AI request allowed given current usage?
 * Throws `BudgetExceededError` or `ConcurrencyLimitError` when limits are hit.
 */
export async function assertBudgetAvailable(): Promise<void> {
  const status = await getBudgetStatus();

  if (!status.allowed) {
    if (status.concurrent_active >= status.concurrent_limit) {
      throw new ConcurrencyLimitError(
        status.concurrent_active,
        status.concurrent_limit,
      );
    }
    throw new BudgetExceededError({
      hourlyRemaining: status.hourly_remaining,
      dailyRemaining: status.daily_remaining,
      resetAt:
        status.hourly_remaining <= 0
          ? new Date(status.hourly_reset_at)
          : new Date(status.daily_reset_at),
    });
  }
}

/**
 * Return current budget status without throwing.
 */
export async function getBudgetStatus(): Promise<BudgetStatus> {
  const [hourlyUsed, dailyUsed, concurrent] = await Promise.all([
    getNumber(HOURLY_KEY),
    getNumber(DAILY_KEY),
    getNumber(CONCURRENT_KEY),
  ]);

  const { hourlyTokenLimit, dailyTokenLimit, maxConcurrentRequests } =
    config.budget;

  const hourlyRemaining = Math.max(0, hourlyTokenLimit - hourlyUsed);
  const dailyRemaining = Math.max(0, dailyTokenLimit - dailyUsed);

  const allowed =
    hourlyRemaining > 0 &&
    dailyRemaining > 0 &&
    concurrent < maxConcurrentRequests;

  return {
    allowed,
    hourly_remaining: hourlyRemaining,
    daily_remaining: dailyRemaining,
    concurrent_active: concurrent,
    concurrent_limit: maxConcurrentRequests,
    hourly_reset_at: hourlyResetDate().toISOString(),
    daily_reset_at: dailyResetDate().toISOString(),
  };
}

// ---------------------------------------------------------------------------
// Recording usage
// ---------------------------------------------------------------------------

/**
 * Record `tokensUsed` against the hourly and daily counters.
 * Automatically sets expiry on first write so counters reset naturally.
 */
export async function recordUsage(tokensUsed: number): Promise<void> {
  const [hourlyNew, dailyNew] = await Promise.all([
    incrBy(HOURLY_KEY, tokensUsed),
    incrBy(DAILY_KEY, tokensUsed),
  ]);

  // Set expiry on first increment (counter was 0 before this write).
  if (hourlyNew === tokensUsed) {
    await setExpiry(HOURLY_KEY, secondsUntilNextHour());
  }
  if (dailyNew === tokensUsed) {
    await setExpiry(DAILY_KEY, secondsUntilMidnightUTC());
  }
}

// ---------------------------------------------------------------------------
// Concurrency semaphore
// ---------------------------------------------------------------------------

/**
 * Increment the concurrent-request counter. Call `releaseConcurrency()`
 * when the request finishes.
 */
export async function acquireConcurrency(): Promise<number> {
  return incrBy(CONCURRENT_KEY, 1);
}

/**
 * Decrement the concurrent-request counter.
 * Floors at 0 to avoid drift from crash scenarios.
 */
export async function releaseConcurrency(): Promise<number> {
  const val = await decrBy(CONCURRENT_KEY, 1);
  return Math.max(0, val);
}
