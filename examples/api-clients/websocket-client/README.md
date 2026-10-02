# WebSocket Trace Client

This example demonstrates how to connect to the Grat `grat-server` WebSocket API to stream real-time tracing data, CPU/Memory resource updates, and state differences for a specific Soroban transaction.

## Installation

```bash
pnpm install
# or npm install
```

## Running the Example

Make sure the `grat-server` is actively running in another terminal (`pnpm --filter grat-server dev`).

Run the script by providing a mock or real transaction hash:

```bash
pnpm start <transaction_hash>
```
