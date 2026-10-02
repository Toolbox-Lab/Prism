import { randomUUID } from "node:crypto";
import { config } from "../config";
import {
  fingerprint,
  tryAcquire,
  publishResult,
  waitForResult,
} from "../dedup";
import {
  assertBudgetAvailable,
  recordUsage,
  acquireConcurrency,
  releaseConcurrency,
} from "../budget";
import { getCache, setCache } from "../cache";
import { SYSTEM_PROMPT, buildUserPrompt, estimateTokens } from "./prompts";
import type { AIExplanation } from "./types";

// ---------------------------------------------------------------------------
// Provider-agnostic AI API call
// ---------------------------------------------------------------------------

interface ChatCompletion {
  summary: string;
  detailed_explanation: string;
  suggested_fixes: string[];
  tokens_used: number;
}

/**
 * Calls an OpenAI-compatible chat-completions endpoint.
 * Abstracted so the provider can be swapped by changing `config.ai.apiBaseUrl`.
 */
async function callLLM(reportJson: string): Promise<ChatCompletion> {
  const {
    apiKey,
    apiBaseUrl,
    model,
    maxTokensPerRequest,
    temperature,
    timeoutMs,
  } = config.ai;

  if (!apiKey) {
    throw new Error(
      "AI_API_KEY is not configured. Set the AI_API_KEY environment variable.",
    );
  }

  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);

  try {
    const response = await fetch(`${apiBaseUrl}/chat/completions`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${apiKey}`,
      },
      body: JSON.stringify({
        model,
        max_tokens: maxTokensPerRequest,
        temperature,
        response_format: { type: "json_object" },
        messages: [
          { role: "system", content: SYSTEM_PROMPT },
          { role: "user", content: buildUserPrompt(reportJson) },
        ],
      }),
      signal: controller.signal,
    });

    if (!response.ok) {
      const text = await response.text();
      throw new Error(`AI provider returned HTTP ${response.status}: ${text}`);
    }

    const data = (await response.json()) as {
      choices: { message: { content: string } }[];
      usage?: { total_tokens: number };
    };

    const raw = data.choices?.[0]?.message?.content;
    if (!raw) {
      throw new Error("AI provider returned an empty response.");
    }

    const parsed = JSON.parse(raw) as {
      summary?: string;
      detailed_explanation?: string;
      suggested_fixes?: string[];
    };

    return {
      summary: parsed.summary ?? "Unable to summarize.",
      detailed_explanation:
        parsed.detailed_explanation ?? "No detailed explanation provided.",
      suggested_fixes: parsed.suggested_fixes ?? [],
      tokens_used: data.usage?.total_tokens ?? estimateTokens(raw),
    };
  } finally {
    clearTimeout(timer);
  }
}

// ---------------------------------------------------------------------------
// Orchestrated pipeline: dedup → budget → cache → LLM → cache → publish
// ---------------------------------------------------------------------------

/**
 * High-level entry point. Given a raw diagnostic report (as JSON string),
 * return a plain-English AI explanation.
 *
 * The function coordinates deduplication, budget enforcement, cache lookup,
 * the LLM call, cache write, and usage recording.
 */
export async function explainError(
  reportJson: string,
  txHash: string,
  network: string,
): Promise<AIExplanation> {
  const fp = fingerprint(txHash, network, "explain");

  // 1. Cache hit — return immediately
  const cached = await getCache("explain", fp);
  if (cached) {
    const parsed = JSON.parse(cached) as AIExplanation;
    return { ...parsed, cached: true };
  }

  // 2. Dedup — is there already an in-flight request for this fingerprint?
  const dedup = await tryAcquire(fp);

  if (!dedup.acquired) {
    // Another request is already processing — wait for its result
    const coalesced = await waitForResult(fp);
    if (coalesced) {
      const parsed = JSON.parse(coalesced) as AIExplanation;
      return { ...parsed, cached: true, request_id: dedup.requestId };
    }
    // Fallback: the original timed out — treat this as a fresh request
  }

  // 3. Budget pre-flight
  await assertBudgetAvailable();

  // 4. Concurrency gate
  await acquireConcurrency();

  try {
    // 5. Call the LLM
    const result = await callLLM(reportJson);

    const explanation: AIExplanation = {
      summary: result.summary,
      detailed_explanation: result.detailed_explanation,
      suggested_fixes: result.suggested_fixes,
      tokens_used: result.tokens_used,
      cached: false,
      request_id: dedup.requestId,
    };

    // 6. Record token usage
    await recordUsage(result.tokens_used);

    // 7. Cache the result
    const serialized = JSON.stringify(explanation);
    await setCache("explain", fp, serialized);

    // 8. Publish result for any coalesced waiters
    await publishResult(fp, serialized);

    return explanation;
  } finally {
    await releaseConcurrency();
  }
}
