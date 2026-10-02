import Redis from "ioredis";
import { randomBytes } from "crypto";

/** How long (seconds) a session stays alive in Redis without a heartbeat. */
const SESSION_TTL_SECONDS = 3600; // 1 hour

export interface SessionMeta {
  /** Opaque unique identifier for this debug session. */
  sessionId: string;
  /** Short-lived bearer token the CLI uses to prove ownership. */
  token: string;
  /** Transaction hash being debugged. */
  txHash: string;
  /** Stellar network the session targets. */
  network: string;
  /** Full WebSocket URL the frontend should connect to for the live trace stream. */
  wsUrl: string;
  /** ISO timestamp of when the session was created. */
  createdAt: string;
  /** ISO timestamp of the most recent heartbeat / state update. */
  updatedAt: string;
  /** Snapshot of the latest trace state pushed by the CLI (partial – streamed incrementally over WS). */
  traceSnapshot: TraceSnapshot | null;
}

export interface TraceSnapshot {
  ledger_sequence: number;
  nodes: unknown[];
  resource_profile: ResourceProfile | null;
  state_diff: StateDiffEntry[];
  completed: boolean;
  error?: string;
}

interface ResourceProfile {
  cpu_used: number;
  memory_used: number;
  cpu_limit: number;
  memory_limit: number;
  read_bytes?: number;
  read_limit?: number;
  write_bytes?: number;
  write_limit?: number;
}

interface StateDiffEntry {
  key: string;
  before?: string;
  after?: string;
  change_type: string;
}

function redisKey(sessionId: string): string {
  return `grat:session:${sessionId}`;
}

export class SessionStore {
  private redis: Redis;

  constructor(redisUrl: string) {
    this.redis = new Redis(redisUrl, {
      // Fail fast on startup rather than silently queuing commands forever.
      enableOfflineQueue: false,
      lazyConnect: true,
    });
  }

  async connect(): Promise<void> {
    await this.redis.connect();
  }

  async disconnect(): Promise<void> {
    await this.redis.quit();
  }

  /**
   * Create a brand-new session and persist it in Redis.
   * Returns the full SessionMeta including the generated token.
   */
  async create(opts: {
    txHash: string;
    network: string;
    wsUrl: string;
  }): Promise<SessionMeta> {
    const sessionId = randomBytes(16).toString("hex");
    const token = randomBytes(32).toString("hex");
    const now = new Date().toISOString();

    const meta: SessionMeta = {
      sessionId,
      token,
      txHash: opts.txHash,
      network: opts.network,
      wsUrl: opts.wsUrl,
      createdAt: now,
      updatedAt: now,
      traceSnapshot: null,
    };

    await this.redis.set(
      redisKey(sessionId),
      JSON.stringify(meta),
      "EX",
      SESSION_TTL_SECONDS,
    );

    return meta;
  }

  /**
   * Retrieve a session by ID.  Returns null when the session does not exist or
   * has expired.
   */
  async get(sessionId: string): Promise<SessionMeta | null> {
    const raw = await this.redis.get(redisKey(sessionId));
    if (!raw) return null;
    return JSON.parse(raw) as SessionMeta;
  }

  /**
   * Persist an updated trace snapshot and refresh the TTL so active sessions
   * do not expire while the CLI is still streaming.
   */
  async updateSnapshot(
    sessionId: string,
    snapshot: TraceSnapshot,
  ): Promise<boolean> {
    const meta = await this.get(sessionId);
    if (!meta) return false;

    meta.traceSnapshot = snapshot;
    meta.updatedAt = new Date().toISOString();

    await this.redis.set(
      redisKey(sessionId),
      JSON.stringify(meta),
      "EX",
      SESSION_TTL_SECONDS,
    );
    return true;
  }

  /**
   * Extend the TTL without changing any data (heartbeat from the CLI).
   */
  async touch(sessionId: string): Promise<boolean> {
    const result = await this.redis.expire(
      redisKey(sessionId),
      SESSION_TTL_SECONDS,
    );
    return result === 1;
  }

  /**
   * Remove a session immediately (e.g. CLI closes the debug session).
   */
  async delete(sessionId: string): Promise<boolean> {
    const result = await this.redis.del(redisKey(sessionId));
    return result === 1;
  }

  /**
   * Verify that a bearer token matches the stored token for the given session.
   * Constant-time comparison prevents timing attacks.
   */
  async verifyToken(sessionId: string, token: string): Promise<boolean> {
    const meta = await this.get(sessionId);
    if (!meta) return false;

    // timingSafeEqual requires same-length buffers.
    const { timingSafeEqual } = await import("crypto");
    const a = Buffer.from(meta.token);
    const b = Buffer.from(token);
    if (a.length !== b.length) return false;
    return timingSafeEqual(a, b);
  }
}
