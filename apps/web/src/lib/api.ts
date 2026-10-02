const API_BASE = process.env.NEXT_PUBLIC_API_URL || "http://localhost:3001";

export async function requestReplay(txHash: string, network: string) {
  const res = await fetch(`${API_BASE}/api/replay`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ txHash, network }),
  });
  return res.json();
}

export async function getHealth() {
  const res = await fetch(`${API_BASE}/health`);
  return res.json();
}

/**
 * Fetch live session metadata by ID.
 *
 * Returns `null` when the session does not exist or has expired (HTTP 404).
 * Throws for any other non-OK status.
 */
export async function getSession(
  sessionId: string,
): Promise<SessionResponse | null> {
  const res = await fetch(
    `${API_BASE}/api/session/${encodeURIComponent(sessionId)}`,
  );

  if (res.status === 404) return null;

  if (!res.ok) {
    const body = await res.text();
    throw new Error(
      `Failed to fetch session ${sessionId}: ${res.status} ${body}`,
    );
  }

  return res.json() as Promise<SessionResponse>;
}

// ---------------------------------------------------------------------------
// Types mirroring the server's public session payload (token is never sent).
// ---------------------------------------------------------------------------

export interface TraceSnapshotResponse {
  ledger_sequence: number;
  nodes: unknown[];
  resource_profile: ResourceProfileResponse | null;
  state_diff: StateDiffEntryResponse[];
  completed: boolean;
  error?: string;
}

export interface ResourceProfileResponse {
  cpu_used: number;
  memory_used: number;
  cpu_limit: number;
  memory_limit: number;
  read_bytes?: number;
  read_limit?: number;
  write_bytes?: number;
  write_limit?: number;
}

export interface StateDiffEntryResponse {
  key: string;
  before?: string;
  after?: string;
  change_type: string;
}

export interface SessionMetaPublic {
  sessionId: string;
  txHash: string;
  network: string;
  /** WebSocket URL the frontend should open for the live trace stream. */
  wsUrl: string;
  createdAt: string;
  updatedAt: string;
  traceSnapshot: TraceSnapshotResponse | null;
}

export interface SessionResponse {
  status: "active";
  session: SessionMetaPublic;
}
