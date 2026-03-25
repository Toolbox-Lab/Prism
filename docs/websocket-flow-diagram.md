# WebSocket Trace Streaming Flow Diagram

## Connection and Request Flow

```
┌─────────────┐                                    ┌──────────────┐
│  Web Client │                                    │ prism serve  │
│   (Browser) │                                    │  (Rust CLI)  │
└──────┬──────┘                                    └──────┬───────┘
       │                                                  │
       │  1. WebSocket Connection Request                │
       │─────────────────────────────────────────────────>│
       │                                                  │
       │  2. Connection Accepted                         │
       │<─────────────────────────────────────────────────│
       │                                                  │
       │  3. Send Trace Request                          │
       │     {"tx_hash": "abc123..."}                    │
       │─────────────────────────────────────────────────>│
       │                                                  │
       │                                                  │  4. Spawn Trace Task
       │                                                  │─────────────┐
       │                                                  │             │
       │                                                  │<────────────┘
       │                                                  │
       │  5. trace_started                               │
       │     {"type": "trace_started", ...}              │
       │<─────────────────────────────────────────────────│
       │                                                  │
       │  6. trace_node (×N)                             │
       │     {"type": "trace_node", ...}                 │
       │<─────────────────────────────────────────────────│
       │<─────────────────────────────────────────────────│
       │<─────────────────────────────────────────────────│
       │                                                  │
       │  7. resource_update (periodic)                  │
       │     {"type": "resource_update", ...}            │
       │<─────────────────────────────────────────────────│
       │                                                  │
       │  8. state_diff_entry (×M)                       │
       │     {"type": "state_diff_entry", ...}           │
       │<─────────────────────────────────────────────────│
       │<─────────────────────────────────────────────────│
       │                                                  │
       │  9. trace_completed                             │
       │     {"type": "trace_completed", ...}            │
       │<─────────────────────────────────────────────────│
       │                                                  │
```

## Internal Server Flow

```
┌──────────────────────────────────────────────────────────────┐
│                      prism serve                              │
│                                                               │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  Main Server Loop                                    │    │
│  │                                                       │    │
│  │  1. Accept WebSocket connection                      │    │
│  │  2. Spawn connection handler task                    │    │
│  │  3. Wait for next connection                         │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                               │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  Connection Handler Task                             │    │
│  │                                                       │    │
│  │  1. Receive trace request                            │    │
│  │  2. Create broadcast channel                         │    │
│  │  3. Spawn trace replay task                          │    │
│  │  4. Forward messages from channel to WebSocket       │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                               │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  Trace Replay Task                                   │    │
│  │                                                       │    │
│  │  1. Send trace_started                               │    │
│  │  2. Reconstruct ledger state                         │    │
│  │  3. Execute with tracing                             │    │
│  │  4. For each trace event:                            │    │
│  │     - Send trace_node                                │    │
│  │     - Throttle (5ms delay)                           │    │
│  │     - Every 10 nodes: send resource_update           │    │
│  │  5. Compute state diff                               │    │
│  │  6. For each diff entry:                             │    │
│  │     - Send state_diff_entry                          │    │
│  │  7. Send trace_completed                             │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                               │
└──────────────────────────────────────────────────────────────┘
```

## React Component Flow

```
┌──────────────────────────────────────────────────────────────┐
│                      Web UI (React)                           │
│                                                               │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  TracePage Component                                 │    │
│  │                                                       │    │
│  │  1. User enters tx_hash                              │    │
│  │  2. Calls requestTrace(tx_hash)                      │    │
│  │  3. Renders loading state                            │    │
│  │  4. Receives updates via callbacks                   │    │
│  │  5. Updates UI incrementally                         │    │
│  └─────────────────────────────────────────────────────┘    │
│                          │                                    │
│                          ▼                                    │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  useTrace Hook                                       │    │
│  │                                                       │    │
│  │  State:                                              │    │
│  │  - trace: TraceData                                  │    │
│  │  - loading: boolean                                  │    │
│  │                                                       │    │
│  │  Callbacks:                                          │    │
│  │  - onTraceStarted → Initialize trace                │    │
│  │  - onTraceNode → Append to nodes[]                  │    │
│  │  - onResourceUpdate → Update profile                │    │
│  │  - onStateDiffEntry → Append to diff[]              │    │
│  │  - onTraceCompleted → Set completed flag            │    │
│  │  - onTraceError → Set error state                   │    │
│  └─────────────────────────────────────────────────────┘    │
│                          │                                    │
│                          ▼                                    │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  useWebSocket Hook                                   │    │
│  │                                                       │    │
│  │  1. Connect to WebSocket server                      │    │
│  │  2. Handle connection lifecycle                      │    │
│  │  3. Parse incoming messages                          │    │
│  │  4. Route to appropriate callback                    │    │
│  │  5. Provide sendMessage() function                   │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                               │
└──────────────────────────────────────────────────────────────┘
```

## Data Flow Timeline

```
Time →

T0:  User clicks "Trace Transaction"
     │
     ├─> WebSocket connection established
     │
T1:  Client sends {"tx_hash": "..."}
     │
     ├─> Server receives request
     │
T2:  Server sends trace_started
     │
     ├─> UI shows "Reconstructing state..."
     │
T3:  Server reconstructs ledger state
     │
T4:  Server starts replay
     │
     ├─> Server sends trace_node #1
     │   └─> UI displays node #1
     │
T5:  Server sends trace_node #2
     │   └─> UI displays node #2
     │
T6:  Server sends trace_node #3
     │   └─> UI displays node #3
     │
     ... (nodes continue streaming)
     │
T7:  Server sends resource_update
     │   └─> UI updates progress bars
     │
     ... (more nodes)
     │
T8:  Server computes state diff
     │
     ├─> Server sends state_diff_entry #1
     │   └─> UI displays diff #1
     │
     ├─> Server sends state_diff_entry #2
     │   └─> UI displays diff #2
     │
T9:  Server sends trace_completed
     │
     ├─> UI shows "Trace complete!"
     │
T10: Connection remains open for next request
```

## Error Handling Flow

```
┌─────────────┐                                    ┌──────────────┐
│  Web Client │                                    │ prism serve  │
└──────┬──────┘                                    └──────┬───────┘
       │                                                  │
       │  Normal flow...                                 │
       │                                                  │
       │                                                  │  Error occurs
       │                                                  │  (e.g., state
       │                                                  │   reconstruction
       │                                                  │   fails)
       │                                                  │
       │  trace_error                                    │
       │     {"type": "trace_error",                     │
       │      "error": "Failed to reconstruct..."}       │
       │<─────────────────────────────────────────────────│
       │                                                  │
       │  Display error to user                          │
       │  Stop loading spinner                           │
       │  Show retry option                              │
       │                                                  │
```

## Concurrent Sessions

```
┌──────────────┐
│ prism serve  │
└──────┬───────┘
       │
       ├─────────────────────────────────────────┐
       │                                         │
       ▼                                         ▼
┌──────────────┐                         ┌──────────────┐
│  Client A    │                         │  Client B    │
│  (tx_hash_1) │                         │  (tx_hash_2) │
└──────┬───────┘                         └──────┬───────┘
       │                                         │
       │  Independent trace task                 │  Independent trace task
       │  with own broadcast channel             │  with own broadcast channel
       │                                         │
       ▼                                         ▼
   Trace 1 events                           Trace 2 events
   streamed to A                            streamed to B
```

## Component Hierarchy

```
TracePage
├── Form (tx_hash input, network select)
├── Connection Status (streaming indicator)
├── Loading Spinner (with progress)
└── Trace Results
    ├── Transaction Info Card
    │   ├── tx_hash
    │   ├── ledger_sequence
    │   ├── node count
    │   └── error (if any)
    │
    ├── ResourceProfile
    │   ├── CPU progress bar
    │   ├── Memory progress bar
    │   └── Warnings
    │
    ├── ExecutionTimeline
    │   └── TraceNode[] (incrementally added)
    │       ├── node index
    │       ├── event type
    │       ├── path
    │       └── data
    │
    └── StateDiffViewer
        └── DiffEntry[] (incrementally added)
            ├── key
            ├── change type badge
            ├── before value
            └── after value
```

## Message Size Considerations

```
Message Type          Typical Size    Frequency       Total Impact
─────────────────────────────────────────────────────────────────
trace_started         ~200 bytes      1 per trace     Negligible
trace_node            ~500 bytes      N nodes         N × 500 bytes
resource_update       ~150 bytes      Every 10 nodes  (N/10) × 150 bytes
state_diff_entry      ~300 bytes      M changes       M × 300 bytes
trace_completed       ~100 bytes      1 per trace     Negligible

Example for 1000-node trace with 50 state changes:
  Total: ~500KB over ~5 seconds = ~100KB/s
```

## Performance Optimization Points

1. **Throttling**: 5ms delay between nodes prevents client overwhelm
2. **Batching**: Resource updates every 10 nodes reduces message count
3. **Bounded Channels**: 100-message capacity prevents memory issues
4. **Async Tasks**: Each trace runs independently without blocking
5. **Incremental Rendering**: React updates UI efficiently with new data
