export interface AIExplanation {
  /** One-line plain-English summary of the error. */
  summary: string;

  /** Multi-paragraph detailed explanation. */
  detailed_explanation: string;

  /** Actionable fix suggestions. */
  suggested_fixes: string[];

  /** Tokens consumed by this request (prompt + completion). */
  tokens_used: number;

  /** Whether the result was served from cache. */
  cached: boolean;

  /** Unique identifier for this request (matches dedup fingerprint on cache hit). */
  request_id: string;
}

export interface AIRequestMeta {
  /** SHA-256 fingerprint of the canonical request body. */
  fingerprint: string;

  /** UUID for this specific request attempt. */
  request_id: string;

  /** Unix timestamp (ms) when the request was created. */
  timestamp: number;

  /** Estimated token count for the prompt. */
  tokens_estimated: number;
}

export interface BudgetStatus {
  /** Whether a new AI request is currently allowed. */
  allowed: boolean;

  /** Tokens remaining in the current hourly window. */
  hourly_remaining: number;

  /** Tokens remaining in the current daily window. */
  daily_remaining: number;

  /** Current concurrent in-flight requests. */
  concurrent_active: number;

  /** Maximum allowed concurrent requests. */
  concurrent_limit: number;

  /** When the hourly window resets (ISO 8601). */
  hourly_reset_at: string;

  /** When the daily window resets (ISO 8601). */
  daily_reset_at: string;
}

export type CacheEntryType = "decode" | "profile" | "explain";
