import type { FastifyInstance } from "fastify";
import { explainError } from "../ai";
import { getBudgetStatus } from "../budget";
import { BudgetExceededError, ConcurrencyLimitError } from "../budget";
import { invalidateByTxHash, clearAll } from "../cache";

// ---------------------------------------------------------------------------
// Request / response schemas (inline for now — could be moved to a schema file)
// ---------------------------------------------------------------------------

interface ExplainBody {
  report: Record<string, unknown>;
  tx_hash: string;
  network: string;
}

interface CacheDeleteParams {
  txHash: string;
}

// ---------------------------------------------------------------------------
// Route plugin
// ---------------------------------------------------------------------------

export async function aiRoutes(app: FastifyInstance) {
  /**
   * POST /api/ai/explain
   *
   * Accepts a DiagnosticReport and returns an AI-generated explanation.
   * Integrates request deduplication, token-budget enforcement, and caching.
   */
  app.post<{ Body: ExplainBody }>("/ai/explain", async (request, reply) => {
    const { report, tx_hash, network } = request.body;

    if (!report || !tx_hash || !network) {
      return reply.status(400).send({
        error: "Missing required fields: report, tx_hash, network",
      });
    }

    try {
      const reportJson = JSON.stringify(report);
      const explanation = await explainError(reportJson, tx_hash, network);
      return explanation;
    } catch (err) {
      if (err instanceof BudgetExceededError) {
        return reply.status(429).send({
          error: err.message,
          hourly_remaining: err.hourlyRemaining,
          daily_remaining: err.dailyRemaining,
          reset_at: err.resetAt.toISOString(),
        });
      }
      if (err instanceof ConcurrencyLimitError) {
        return reply.status(429).send({
          error: err.message,
          concurrent_active: err.currentActive,
          concurrent_limit: err.limit,
        });
      }
      request.log.error(err, "AI explain request failed");
      return reply.status(500).send({
        error: err instanceof Error ? err.message : "Internal server error",
      });
    }
  });

  /**
   * GET /api/ai/budget
   *
   * Returns current token usage and remaining budget.
   */
  app.get("/ai/budget", async () => {
    return getBudgetStatus();
  });

  /**
   * DELETE /api/ai/cache/:txHash
   *
   * Invalidates all cached AI responses for a specific transaction hash.
   */
  app.delete<{ Params: CacheDeleteParams }>(
    "/ai/cache/:txHash",
    async (request) => {
      const { txHash } = request.params;
      const removed = await invalidateByTxHash(txHash);
      return { invalidated: removed, tx_hash: txHash };
    },
  );

  /**
   * DELETE /api/ai/cache
   *
   * Clears the entire AI response cache.
   */
  app.delete("/ai/cache", async () => {
    const removed = await clearAll();
    return { invalidated: removed };
  });
}
