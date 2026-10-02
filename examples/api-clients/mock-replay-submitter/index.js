#!/usr/bin/env node

const API_URL = process.env.API_URL || "http://localhost:3001";
const REPLAY_ENDPOINT = `${API_URL}/api/replay`;

/**
 * Submits a transaction hash to the grat-server replay API and returns the
 * job token used to track simulation progress.
 */
async function submitReplayJob(txHash) {
  let response;
  try {
    response = await fetch(REPLAY_ENDPOINT, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Accept: "application/json",
      },
      body: JSON.stringify({ tx_hash: txHash }),
    });
  } catch (err) {
    throw new Error(
      `Could not reach grat-server at ${REPLAY_ENDPOINT}: ${err.message}. ` +
        "Is the server running? (pnpm --filter grat-server dev)",
    );
  }

  let payload;
  try {
    payload = await response.json();
  } catch (err) {
    throw new Error(
      `Server returned a non-JSON response (HTTP ${response.status} ${response.statusText}): ${err.message}`,
    );
  }

  if (!response.ok) {
    const detail = payload && (payload.error || payload.message);
    throw new Error(
      `Replay submission failed with HTTP ${response.status} ${response.statusText}` +
        (detail ? `: ${detail}` : ""),
    );
  }

  if (!payload || !payload.jobId) {
    throw new Error(
      `Replay submission succeeded (HTTP ${response.status}) but the response did not include a "jobId": ${JSON.stringify(
        payload,
      )}`,
    );
  }

  return payload.jobId;
}

/**
 * Polls the server using exponential backoff to check the status of a replay job.
        396-polling-state-machine
 * Implements a strict state machine with circuit breaker to prevent infinite loops.
 *
 * States:
 * - queued / pending / waiting → backoff → next poll
 * - running / active → backoff → next poll
 * - completed → extract result, resolve
 * - failed / error → extract error_reason, reject
 *
 * Circuit breaker: max 60 seconds OR max 50 iterations

 *
 * @param {string} jobId The ID of the job to poll.
 * @param {number} currentDelay The delay before the next poll request in milliseconds.
 * @returns {Promise<object>} The final job status payload.
        main
 */
function pollJobStatus(jobId, currentDelay = 500, iteration = 0, startTime = Date.now()) {
  const MAX_ITERATIONS = 50;
  const MAX_TIMEOUT_MS = 60000;

  return new Promise((resolve, reject) => {
    const executePoll = async () => {
      const elapsed = Date.now() - startTime;

      if (iteration >= MAX_ITERATIONS) {
        reject(new Error(`Polling exceeded maximum iterations (${MAX_ITERATIONS}). Job may be stuck.`));
        return;
      }
      if (elapsed >= MAX_TIMEOUT_MS) {
        reject(new Error(`Polling exceeded maximum timeout (${MAX_TIMEOUT_MS / 1000}s). Job may be stuck.`));
        return;
      }

      try {
        const response = await fetch(`${REPLAY_ENDPOINT}/${jobId}`, {
          headers: { Accept: "application/json" },
        });

        if (!response.ok) {
          console.warn(
            `[Poll Warning] Replay status fetch failed with HTTP ${response.status}`,
          );
          scheduleNext();
          return;
        }

        let payload;
        try {
          payload = await response.json();
        } catch (err) {
          console.warn(`[Poll Warning] Non-JSON response: ${err.message}`);
          scheduleNext();
          return;
        }

        const status = payload.status;
        396-polling-state-machine

        const pendingStatuses = [
          "queued",
          "pending",
          "running",
          "waiting",
          "active",
        ];
        main

        // ─── State Machine ────────────────────────────────────────────
        switch (status) {
          case 'queued':
          case 'pending':
          case 'waiting':
          case 'running':
          case 'active':
            console.log(`[Poll ${iteration + 1}] Job is ${status}... retrying in ${currentDelay}ms`);
            scheduleNext();
            break;

          case 'completed':
          case 'done':
          case 'success':
            console.log(`[Poll ${iteration + 1}] Job completed successfully ✓`);
            const result = payload.result || payload.data || payload;
            resolve(result);
            break;

          case 'failed':
          case 'error':
            const errorReason = payload.error_reason || payload.error || payload.message || 'Unknown error';
            console.error(`[Poll ${iteration + 1}] Job failed: ${errorReason}`);
            reject(new Error(`Job failed: ${errorReason}`));
            break;

          default:
            console.warn(`[Poll ${iteration + 1}] Unknown status "${status}", treating as pending...`);
            scheduleNext();
            break;
        }
      } catch (err) {
        396-polling-state-machine
        console.warn(`[Poll Warning] Network/request error during poll: ${err.message}`);

        // Handle network/request errors gracefully without crashing
        console.warn(
          `[Poll Warning] Network/request error during poll: ${err.message}`,
        );
        main
        scheduleNext();
      }
    };

    const scheduleNext = () => {
      const nextDelay = Math.min(currentDelay * 2, 5000);
      pollJobStatus(jobId, nextDelay, iteration + 1, startTime)
        .then(resolve)
        .catch(reject);
    };

    setTimeout(executePoll, currentDelay);
  });
}

async function main() {
  const txHash = process.argv[2];

  if (!txHash) {
    console.error("Usage: node index.js <tx-hash>");
    process.exitCode = 1;
    return;
  }

  console.log(`Submitting replay job for ${txHash} to ${REPLAY_ENDPOINT}...`);

  try {
    const jobId = await submitReplayJob(txHash);
    console.log(`✓ Replay job accepted. jobId: ${jobId}`);

    console.log('Polling for job status... (max 60s / 50 iterations)');
    const finalResult = await pollJobStatus(jobId);

    console.log('\n=== Simulation Results ===');
    console.log(JSON.stringify(finalResult, null, 2));
    console.log('===========================\n');

    console.log('✓ Job completed successfully. Exiting.');
    process.exit(0);
  } catch (err) {
    console.error(`✗ ${err.message}`);
    process.exitCode = 1;
  }
}

if (require.main === module) {
  main();
}

module.exports = { submitReplayJob, pollJobStatus };
