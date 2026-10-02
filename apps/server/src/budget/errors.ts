/**
 * Thrown when the hourly or daily token budget has been exhausted.
 */
export class BudgetExceededError extends Error {
  public readonly hourlyRemaining: number;
  public readonly dailyRemaining: number;
  public readonly resetAt: Date;

  constructor(opts: {
    hourlyRemaining: number;
    dailyRemaining: number;
    resetAt: Date;
  }) {
    const which = opts.hourlyRemaining <= 0 ? "hourly" : "daily";
    super(
      `AI token budget exceeded: ${which} limit reached. ` +
        `Resets at ${opts.resetAt.toISOString()}.`,
    );
    this.name = "BudgetExceededError";
    this.hourlyRemaining = opts.hourlyRemaining;
    this.dailyRemaining = opts.dailyRemaining;
    this.resetAt = opts.resetAt;
  }
}

/**
 * Thrown when the maximum number of concurrent AI requests is already in-flight.
 */
export class ConcurrencyLimitError extends Error {
  public readonly currentActive: number;
  public readonly limit: number;

  constructor(currentActive: number, limit: number) {
    super(
      `AI concurrency limit reached: ${currentActive}/${limit} requests in-flight. ` +
        `Retry after an in-flight request completes.`,
    );
    this.name = "ConcurrencyLimitError";
    this.currentActive = currentActive;
    this.limit = limit;
  }
}
