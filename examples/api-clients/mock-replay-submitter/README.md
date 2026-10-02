# Mock Replay Submitter

This example demonstrates how an external script or CI/CD pipeline can interact with the `grat-server` REST API to submit a transaction hash for simulation/replay, and dynamically poll the API for the job's completion status.

## Installation

No external dependencies required (uses Node's native `fetch` API).
Requires Node >= 18.

## Running the Example

Make sure the `grat-server` is actively running in another terminal (`pnpm --filter grat-server dev`).

Run the script by providing a transaction hash:

```bash
npm start <transaction_hash>
```
