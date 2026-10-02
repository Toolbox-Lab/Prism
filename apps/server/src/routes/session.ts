import type { FastifyInstance } from "fastify";
import type { SessionStore, TraceSnapshot } from "../session-store";

// ---------------------------------------------------------------------------
// Request / response schemas
// ---------------------------------------------------------------------------

const createSessionSchema = {
  body: {
    type: "object",
    required: ["txHash", "network", "wsUrl"],
    properties: {
      txHash: { type: "string", minLength: 1 },
      network: { type: "string", minLength: 1 },
      wsUrl: { type: "string", minLength: 1 },
    },
  },
} as const;

const sessionIdParamSchema = {
  params: {
    type: "object",
    required: ["sessionId"],
    properties: {
      sessionId: { type: "string", minLength: 1 },
    },
  },
} as const;

const updateSnapshotSchema = {
  params: {
    type: "object",
    required: ["sessionId"],
    properties: {
      sessionId: { type: "string", minLength: 1 },
    },
  },
  body: {
    type: "object",
    required: ["snapshot"],
    properties: {
      snapshot: {
        type: "object",
        required: ["ledger_sequence", "nodes", "state_diff", "completed"],
        properties: {
          ledger_sequence: { type: "number" },
          nodes: { type: "array" },
          resource_profile: { type: ["object", "null"] },
          state_diff: { type: "array" },
          completed: { type: "boolean" },
          error: { type: "string" },
        },
      },
    },
  },
} as const;

// ---------------------------------------------------------------------------
// Helper – pull the SessionStore off the Fastify instance decorator
// ---------------------------------------------------------------------------
function getStore(app: FastifyInstance): SessionStore {
  // The store is registered as a decorator in index.ts under the key
  // `sessionStore`.  Cast via `any` to keep the route file free of
  // declaration-merging boilerplate.
  return (app as any).sessionStore as SessionStore;
}

// ---------------------------------------------------------------------------
// Helper – extract and validate a Bearer token from the Authorization header
// ---------------------------------------------------------------------------
function extractBearer(authHeader: string | undefined): string | null {
  if (!authHeader) return null;
  const match = authHeader.match(/^Bearer\s+(.+)$/i);
  return match ? match[1] : null;
}

// ---------------------------------------------------------------------------
// Route plugin
// ---------------------------------------------------------------------------

export async function sessionRoutes(app: FastifyInstance) {
  const store = getStore(app);

  // ------------------------------------------------------------------
  // POST /api/session
  // CLI registers a new debug session and receives back a sessionId +
  // secret token that it must include on subsequent authenticated calls.
  // ------------------------------------------------------------------
  app.post<{
    Body: { txHash: string; network: string; wsUrl: string };
  }>("/session", { schema: createSessionSchema }, async (request, reply) => {
    const { txHash, network, wsUrl } = request.body;

    const meta = await store.create({ txHash, network, wsUrl });

    // Return the full meta to the CLI (token is only revealed on creation).
    return reply.status(201).send({
      sessionId: meta.sessionId,
      token: meta.token,
      wsUrl: meta.wsUrl,
      createdAt: meta.createdAt,
    });
  });

  // ------------------------------------------------------------------
  // GET /api/session/:sessionId
  // Frontend polls / bootstraps from this endpoint.  Token is NOT
  // returned here – it is only issued to the CLI at creation time.
  // ------------------------------------------------------------------
  app.get<{
    Params: { sessionId: string };
  }>(
    "/session/:sessionId",
    { schema: sessionIdParamSchema },
    async (request, reply) => {
      const { sessionId } = request.params;

      const meta = await store.get(sessionId);

      if (!meta) {
        return reply.status(404).send({
          status: "not_found",
          message: `Session '${sessionId}' does not exist or has expired.`,
        });
      }

      // Strip the secret token before sending to the frontend.
      const { token: _omit, ...publicMeta } = meta;

      return reply.send({
        status: "active",
        session: publicMeta,
      });
    },
  );

  // ------------------------------------------------------------------
  // PATCH /api/session/:sessionId/snapshot
  // CLI pushes an updated trace snapshot (e.g. after each streamed
  // batch of nodes) so the frontend can bootstrap from a mid-stream
  // state when it joins late or reconnects.
  // Requires: Authorization: Bearer <token>
  // ------------------------------------------------------------------
  app.patch<{
    Params: { sessionId: string };
    Body: { snapshot: TraceSnapshot };
  }>(
    "/session/:sessionId/snapshot",
    { schema: updateSnapshotSchema },
    async (request, reply) => {
      const { sessionId } = request.params;
      const { snapshot } = request.body;

      const token = extractBearer(request.headers.authorization);
      if (!token) {
        return reply
          .status(401)
          .send({ error: "Missing Authorization header" });
      }

      const valid = await store.verifyToken(sessionId, token);
      if (!valid) {
        return reply.status(403).send({ error: "Invalid or expired token" });
      }

      const updated = await store.updateSnapshot(sessionId, snapshot);
      if (!updated) {
        return reply.status(404).send({
          status: "not_found",
          message: `Session '${sessionId}' does not exist or has expired.`,
        });
      }

      return reply.send({ status: "ok" });
    },
  );

  // ------------------------------------------------------------------
  // POST /api/session/:sessionId/heartbeat
  // CLI sends periodic pings to prevent session expiry while a long-
  // running trace is still in progress.
  // Requires: Authorization: Bearer <token>
  // ------------------------------------------------------------------
  app.post<{
    Params: { sessionId: string };
  }>(
    "/session/:sessionId/heartbeat",
    { schema: sessionIdParamSchema },
    async (request, reply) => {
      const { sessionId } = request.params;

      const token = extractBearer(request.headers.authorization);
      if (!token) {
        return reply
          .status(401)
          .send({ error: "Missing Authorization header" });
      }

      const valid = await store.verifyToken(sessionId, token);
      if (!valid) {
        return reply.status(403).send({ error: "Invalid or expired token" });
      }

      const touched = await store.touch(sessionId);
      if (!touched) {
        return reply.status(404).send({
          status: "not_found",
          message: `Session '${sessionId}' does not exist or has expired.`,
        });
      }

      return reply.send({ status: "ok", updatedAt: new Date().toISOString() });
    },
  );

  // ------------------------------------------------------------------
  // DELETE /api/session/:sessionId
  // CLI explicitly closes the session (e.g. user presses Ctrl-C).
  // Requires: Authorization: Bearer <token>
  // ------------------------------------------------------------------
  app.delete<{
    Params: { sessionId: string };
  }>(
    "/session/:sessionId",
    { schema: sessionIdParamSchema },
    async (request, reply) => {
      const { sessionId } = request.params;

      const token = extractBearer(request.headers.authorization);
      if (!token) {
        return reply
          .status(401)
          .send({ error: "Missing Authorization header" });
      }

      const valid = await store.verifyToken(sessionId, token);
      if (!valid) {
        return reply.status(403).send({ error: "Invalid or expired token" });
      }

      const deleted = await store.delete(sessionId);
      if (!deleted) {
        return reply.status(404).send({
          status: "not_found",
          message: `Session '${sessionId}' does not exist or has expired.`,
        });
      }

      return reply.status(204).send();
    },
  );
}
