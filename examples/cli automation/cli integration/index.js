#!/usr/bin/env node

/**
 * Prism CLI Integration Helper
 *
 * Wraps the Prism Rust binary with a transparent error layer so that Rust
 * panics, XDR failures, and other fatal errors are surfaced directly to the
 * developer instead of being swallowed by Node.js's generic ChildProcessError.
 *
 * Usage:
 *   node "examples/cli automation/cli integration/index.js" <command> [args...]
 *
 * Example:
 *   node "examples/cli automation/cli integration/index.js" decode <tx-hash>
 */

"use strict";

const { execFile } = require("child_process");
const { promisify } = require("util");
const path = require("path");

const execFileAsync = promisify(execFile);

// ─── ANSI colour helpers ────────────────────────────────────────────────────
const RESET = "\x1b[0m";
const YELLOW = "\x1b[33m";
const RED = "\x1b[31m";
const BOLD = "\x1b[1m";
const DIM = "\x1b[2m";

/**
 * GratCliError
 *
 * A custom Error subclass that captures the full context of a Rust binary
 * failure: the human-readable stderr message, the OS exit code, and the
 * wall-clock execution duration. It deliberately omits the Node.js stack
 * trace so that the Rust panic message is the only thing the developer sees.
 */
class GratCliError extends Error {
  /**
   * @param {object} options
   * @param {string}  options.rustMessage  - Raw text written to stderr by Rust
   * @param {number}  options.exitCode     - Non-zero OS exit code returned by the binary
   * @param {number}  options.durationMs   - Wall-clock time in milliseconds
   * @param {string}  options.command      - The CLI command that was invoked
   */
  constructor({ rustMessage, exitCode, durationMs, command }) {
    super(rustMessage);

    this.name = "GratCliError";
    this.rustMessage = rustMessage;
    this.exitCode = exitCode;
    this.durationMs = durationMs;
    this.command = command;

    // Remove the Node.js-generated stack so it does not pollute the output.
    // Developers should look at rustMessage, not the JS call-site.
    this.stack = undefined;
  }

  /**
   * Format the error for console output using warning colours.
   * @returns {string}
   */
  format() {
    const ruler = `${YELLOW}${"─".repeat(60)}${RESET}`;
    return [
      "",
      ruler,
      `${BOLD}${YELLOW}⚠  Prism CLI failed (exit code ${this.exitCode})${RESET}`,
      ruler,
      "",
      `${BOLD}Command :${RESET}  ${this.command}`,
      `${BOLD}Duration:${RESET}  ${this.durationMs} ms`,
      "",
      `${BOLD}${RED}Rust error output:${RESET}`,
      this.rustMessage
        .split("\n")
        .map((line) => `  ${DIM}│${RESET} ${line}`)
        .join("\n"),
      "",
      ruler,
      "",
    ].join("\n");
  }
}

// ─── Binary resolution ───────────────────────────────────────────────────────

/**
 * Resolve the path to the Prism binary.
 * Prefers the release build; falls back to the debug build for development.
 */
function resolveBinaryPath() {
  const repoRoot = path.resolve(__dirname, "..", "..", "..");

  // Allow an environment-variable override for CI or custom install paths.
  if (process.env.PRISM_BIN) {
    return process.env.PRISM_BIN;
  }

  return path.join(repoRoot, "target", "release", "prism");
}

// ─── Core execution wrapper ──────────────────────────────────────────────────

/**
 * Run the Prism Rust binary with the provided arguments.
 *
 * On success the function resolves with the trimmed stdout string.
 * On failure it throws a `GratCliError` whose `.rustMessage` contains the
 * verbatim text that Rust wrote to stderr – making cross-language debugging
 * fully transparent.
 *
 * @param {string[]} args  - Arguments forwarded to the Prism binary
 * @returns {Promise<string>}
 * @throws {GratCliError}
 */
async function runPrism(args = []) {
  const binary = resolveBinaryPath();
  const command = `${binary} ${args.join(" ")}`;
  const start = Date.now();

  try {
    const { stdout, stderr } = await execFileAsync(binary, args, {
      // Capture both streams as UTF-8 strings so no output is lost.
      encoding: "utf8",
      // Give long-running operations up to 5 minutes before we time out.
      timeout: 5 * 60 * 1000,
      // Soroban envelope decodes can emit multi-megabyte JSON payloads, while
      // Node.js caps a child's stdout at only 200 KB by default. Overflowing
      // that cap kills the process with ERR_CHILD_PROCESS_STDIO_MAXBUFFER and
      // discards the data. Raise the ceiling to 10 MB so large decodes stream
      // through intact.
      maxBuffer: 10 * 1024 * 1024,
    });

    // Rust can write diagnostic or warning logs to stderr even on a
    // successful (exit code 0) run. Surface them instead of dropping them so
    // no crucial output is lost during the execution lifecycle.
    const diagnostics = (stderr || "").trim();
    if (diagnostics) {
      console.warn(diagnostics);
    }

    return stdout.trim();
  } catch (err) {
    const durationMs = Date.now() - start;

    // `execFile` rejects with an error that exposes `.code` (exit code) and
    // `.stderr` (the raw text Rust wrote to stderr). Extract both and wrap
    // them in a GratCliError so the actual Rust panic message is surfaced.
    const exitCode = typeof err.code === "number" ? err.code : 1;
    const stderrText = (err.stderr || "").trim();
    const rustMessage =
      stderrText || err.message || "Unknown error from Rust binary";

    throw new GratCliError({
      rustMessage,
      exitCode,
      durationMs,
      command,
    });
  }
}

// ─── Entry point (CLI passthrough) ──────────────────────────────────────────

async function main() {
  // Forward every argument after the script name directly to the Rust binary.
  const args = process.argv.slice(2);

  if (args.length === 0) {
    console.error(
      `${YELLOW}Usage: node "examples/cli automation/cli integration/index.js" <command> [args...]${RESET}\n` +
        `${DIM}Example: node ... decode <tx-hash>${RESET}`,
    );
    process.exit(1);
  }

  try {
    const output = await runPrism(args);
    if (output) {
      console.log(output);
    }
  } catch (err) {
    if (err instanceof GratCliError) {
      // Print the formatted Rust message in warning colours; skip the JS stack.
      console.warn(err.format());
      process.exit(err.exitCode);
    } else {
      // Unexpected Node.js-level error – rethrow so it surfaces normally.
      throw err;
    }
  }
}

main();

// ─── Exports (for programmatic use) ─────────────────────────────────────────
module.exports = { runPrism, GratCliError };
