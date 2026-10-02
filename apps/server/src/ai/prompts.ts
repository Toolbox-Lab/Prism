import { createHash } from "node:crypto";

/**
 * Prompt version — bump this when the prompt content changes materially.
 * Cache keys include this version so stale entries auto-miss after updates.
 */
export const PROMPT_VERSION = "v1";

/**
 * Hash of the current prompt version, used as part of cache keys.
 */
export const PROMPT_VERSION_HASH: string = createHash("sha256")
  .update(PROMPT_VERSION)
  .digest("hex")
  .slice(0, 12);

// ---------------------------------------------------------------------------
// System prompt
// ---------------------------------------------------------------------------
export const SYSTEM_PROMPT = `You are Grat, an expert Soroban smart-contract diagnostics assistant.

Your job is to take a raw DiagnosticReport (JSON) from a failed Stellar Soroban transaction and produce a clear, actionable explanation that a developer can immediately act on.

Rules:
1. Write in plain English — avoid jargon unless the developer needs the exact term.
2. Start with a one-sentence summary of what went wrong.
3. Provide a detailed explanation covering root cause, contributing factors, and on-chain context.
4. List concrete, actionable fixes ordered by likelihood of resolving the issue.
5. When referencing error codes, always include the human-readable name (e.g., "Error #8 (InvalidAction)").
6. If the report includes contract-specific errors resolved from WASM metadata, highlight those prominently.
7. Never fabricate contract addresses, ledger sequences, or function names — only reference data present in the report.
8. Keep the response under 800 tokens.`;

// ---------------------------------------------------------------------------
// User prompt template
// ---------------------------------------------------------------------------

/**
 * Build the user prompt from a diagnostic report JSON string.
 */
export function buildUserPrompt(reportJson: string): string {
  return `Analyze this failed Soroban transaction diagnostic report and explain what went wrong, why, and how to fix it.

\`\`\`json
${reportJson}
\`\`\`

Respond with JSON matching this exact schema:
{
  "summary": "<one-sentence summary>",
  "detailed_explanation": "<multi-paragraph explanation>",
  "suggested_fixes": ["<fix 1>", "<fix 2>", ...]
}`;
}

/**
 * Rough token count estimate (4 chars ≈ 1 token).
 * Good enough for budget pre-flight; actual usage comes from the API response.
 */
export function estimateTokens(text: string): number {
  return Math.ceil(text.length / 4);
}
