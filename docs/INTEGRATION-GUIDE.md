# Integration Guide: WebSocket Streaming

This guide explains how to integrate the WebSocket streaming feature with the existing Prism codebase.

## Overview

The WebSocket streaming implementation is designed to work alongside the existing batch trace functionality. It adds a new `prism serve` command that streams trace updates in real-time.

## Architecture Integration

### Current Architecture
```
CLI Command → Replay Engine → Complete Trace → Output
```

### New Architecture (Streaming)
```
CLI Command → WebSocket Server → Replay Engine → Stream Events → Client
                                       ↓
                                  Broadcast Channel
```

## Integration Steps

### 1. Resolve Dependency Conflicts

The workspace currently has a dependency conflict with `soroban-spec`. This needs to be resolved first:

```toml
# In Cargo.toml, update to compatible versions:
[workspace.dependencies]
soroban-spec = "21.7.7"  # Use latest stable
soroban-spec-tools = "21.7.7"
```

### 2. Enhance Replay Engine for Streaming

The current `execute_with_tracing` function returns all events at once. For true streaming, modify it to accept a callback:

```rust
// Current signature (in crates/core/src/replay/sandbox.rs)
pub async fn execute_with_tracing(
    state: &LedgerState,
    tx_hash: &str,
) -> PrismResult<SandboxResult>

// Proposed streaming signature
pub async fn execute_with_tracing_stream<F>(
    state: &LedgerState,
    tx_hash: &str,
    event_callback: F,
) -> PrismResult<SandboxResult>
where
    F: Fn(TraceEvent) + Send + Sync,
```

Example implementation:

```rust
pub async fn execute_with_tracing_stream<F>(
    state: &LedgerState,
    tx_hash: &str,
    event_callback: F,
) -> PrismResult<SandboxResult>
where
    F: Fn(TraceEvent) + Send + Sync,
{
    // Initialize sandbox
    let mut events = Vec::new();
    
    // During execution, emit events immediately
    for event in sandbox.execute() {
        event_callback(event.clone());  // Stream to callback
        events.push(event);              // Also collect for final result
        
        // Small delay to avoid overwhelming callback
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    
    // Return complete result
    Ok(SandboxResult {
        success: true,
        events,
        // ... other fields
    })
}
```

### 3. Update Serve Command

Modify `crates/cli/src/commands/serve.rs` to use the streaming API:

```rust
async fn stream_trace_replay(
    tx_hash: &str,
    network: &NetworkConfig,
    sender: broadcast::Sender<TraceStreamMessage>,
) -> anyhow::Result<()> {
    // ... existing code ...
    
    // Use streaming API instead of batch
    let result = prism_core::replay::sandbox::execute_with_tracing_stream(
        &ledger_state,
        tx_hash,
        |event| {
            // Stream each event immediately
            let node_json = serde_json::to_value(&event).unwrap();
            let _ = sender.send(TraceStreamMessage::TraceNode {
                node: node_json,
                path: vec![event.index],
            });
        }
    ).await?;
    
    // ... rest of code ...
}
```

### 4. Backward Compatibility

Keep the existing batch API for CLI commands:

```rust
// Batch API (existing)
pub async fn execute_with_tracing(
    state: &LedgerState,
    tx_hash: &str,
) -> PrismResult<SandboxResult> {
    // Call streaming API with no-op callback
    execute_with_tracing_stream(state, tx_hash, |_| {}).await
}
```

This ensures existing commands (`prism trace`, `prism profile`, etc.) continue to work.

### 5. Add Feature Flag (Optional)

For gradual rollout, add a feature flag:

```toml
# In crates/cli/Cargo.toml
[features]
default = ["websocket"]
websocket = ["tokio-tungstenite", "futures-util"]
```

```rust
// In crates/cli/src/commands/mod.rs
#[cfg(feature = "websocket")]
pub mod serve;
```

## Testing Integration

### Unit Tests

Add tests for the streaming functionality:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_streaming_trace() {
        let (tx, mut rx) = broadcast::channel(100);
        
        // Spawn streaming task
        tokio::spawn(async move {
            stream_trace_replay("test_hash", &NetworkConfig::testnet(), tx)
                .await
                .unwrap();
        });
        
        // Verify messages received
        let mut count = 0;
        while let Ok(msg) = rx.recv().await {
            count += 1;
            match msg {
                TraceStreamMessage::TraceStarted { .. } => {},
                TraceStreamMessage::TraceNode { .. } => {},
                TraceStreamMessage::TraceCompleted { .. } => break,
                _ => {}
            }
        }
        
        assert!(count > 0);
    }
}
```

### Integration Tests

Create integration tests that verify end-to-end flow:

```rust
#[tokio::test]
async fn test_websocket_server() {
    // Start server
    let server = tokio::spawn(async {
        serve::run(
            serve::ServeArgs {
                port: 8081,
                host: "127.0.0.1".to_string(),
            },
            &NetworkConfig::testnet(),
        ).await
    });
    
    // Give server time to start
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Connect client
    let (ws_stream, _) = tokio_tungstenite::connect_async("ws://127.0.0.1:8081")
        .await
        .unwrap();
    
    // Send trace request
    // ... test logic ...
    
    server.abort();
}
```

## Deployment Integration

### Docker

Add WebSocket support to Docker configuration:

```dockerfile
# Dockerfile
FROM rust:1.77 as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/prism /usr/local/bin/
EXPOSE 8080
CMD ["prism", "serve", "--host", "0.0.0.0", "--port", "8080"]
```

### Systemd Service

Create a systemd service for the WebSocket server:

```ini
# /etc/systemd/system/prism-serve.service
[Unit]
Description=Prism WebSocket Server
After=network.target

[Service]
Type=simple
User=prism
ExecStart=/usr/local/bin/prism serve --port 8080 --network mainnet
Restart=on-failure
RestartSec=5s

[Install]
WantedBy=multi-user.target
```

### Nginx Reverse Proxy

Configure Nginx to proxy WebSocket connections:

```nginx
server {
    listen 80;
    server_name prism.example.com;
    
    location /ws {
        proxy_pass http://localhost:8080;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_read_timeout 86400;
    }
}
```

## Configuration Integration

### Environment Variables

Support configuration via environment variables:

```rust
// In crates/cli/src/commands/serve.rs
pub struct ServeArgs {
    #[arg(long, short, default_value = "8080", env = "PRISM_WS_PORT")]
    pub port: u16,
    
    #[arg(long, default_value = "127.0.0.1", env = "PRISM_WS_HOST")]
    pub host: String,
}
```

### Configuration File

Support configuration via file:

```toml
# ~/.prism/config.toml
[websocket]
port = 8080
host = "127.0.0.1"
max_connections = 100
message_buffer_size = 100
throttle_delay_ms = 5
```

## Monitoring Integration

### Metrics

Add metrics for monitoring:

```rust
use std::sync::atomic::{AtomicUsize, Ordering};

static ACTIVE_CONNECTIONS: AtomicUsize = AtomicUsize::new(0);
static TOTAL_TRACES: AtomicUsize = AtomicUsize::new(0);
static TOTAL_MESSAGES: AtomicUsize = AtomicUsize::new(0);

// In connection handler
ACTIVE_CONNECTIONS.fetch_add(1, Ordering::Relaxed);
// ... on disconnect
ACTIVE_CONNECTIONS.fetch_sub(1, Ordering::Relaxed);

// Expose metrics endpoint
async fn metrics_handler() -> String {
    format!(
        "active_connections {}\ntotal_traces {}\ntotal_messages {}",
        ACTIVE_CONNECTIONS.load(Ordering::Relaxed),
        TOTAL_TRACES.load(Ordering::Relaxed),
        TOTAL_MESSAGES.load(Ordering::Relaxed),
    )
}
```

### Logging

Add structured logging:

```rust
use tracing::{info, warn, error, debug};

// In serve command
info!(
    port = args.port,
    host = %args.host,
    "Starting WebSocket server"
);

// In connection handler
info!(
    peer = %peer_addr,
    "New connection"
);

// In trace handler
debug!(
    tx_hash = %tx_hash,
    nodes = node_count,
    "Streaming trace"
);
```

## Security Integration

### Authentication

Add JWT authentication:

```rust
use jsonwebtoken::{decode, DecodingKey, Validation};

async fn authenticate_connection(token: &str) -> Result<Claims, Error> {
    let key = DecodingKey::from_secret(SECRET.as_ref());
    let validation = Validation::default();
    let token_data = decode::<Claims>(token, &key, &validation)?;
    Ok(token_data.claims)
}

// In connection handler
let auth_header = /* extract from WebSocket headers */;
let claims = authenticate_connection(auth_header).await?;
```

### Rate Limiting

Add rate limiting per client:

```rust
use std::collections::HashMap;
use std::time::{Duration, Instant};

struct RateLimiter {
    requests: HashMap<String, Vec<Instant>>,
    max_requests: usize,
    window: Duration,
}

impl RateLimiter {
    fn check(&mut self, client_id: &str) -> bool {
        let now = Instant::now();
        let requests = self.requests.entry(client_id.to_string())
            .or_insert_with(Vec::new);
        
        // Remove old requests
        requests.retain(|&t| now.duration_since(t) < self.window);
        
        // Check limit
        if requests.len() >= self.max_requests {
            return false;
        }
        
        requests.push(now);
        true
    }
}
```

## Web UI Integration

### Environment Configuration

Configure WebSocket URL based on environment:

```typescript
// apps/web/src/config.ts
export const config = {
  wsUrl: process.env.NEXT_PUBLIC_WS_URL || 
         (typeof window !== 'undefined' 
           ? `ws://${window.location.hostname}:8080`
           : 'ws://localhost:8080'),
};

// In TracePage
import { config } from '@/config';
const { trace, loading, requestTrace } = useTrace(config.wsUrl);
```

### Build Configuration

Add environment variables to build:

```bash
# .env.local
NEXT_PUBLIC_WS_URL=ws://localhost:8080

# .env.production
NEXT_PUBLIC_WS_URL=wss://prism.example.com/ws
```

## Troubleshooting Integration

### Common Issues

1. **Port conflicts**: Check if port 8080 is already in use
   ```bash
   lsof -i :8080
   ```

2. **Firewall blocking**: Ensure firewall allows WebSocket connections
   ```bash
   sudo ufw allow 8080/tcp
   ```

3. **CORS issues**: Configure CORS headers if needed
   ```rust
   // Add CORS headers to WebSocket response
   ```

4. **Connection drops**: Implement ping/pong keepalive
   ```rust
   // Send periodic ping messages
   tokio::spawn(async move {
       loop {
           tokio::time::sleep(Duration::from_secs(30)).await;
           ws.send(Message::Ping(vec![])).await.ok();
       }
   });
   ```

## Migration Path

### Phase 1: Development (Current)
- WebSocket server runs alongside existing CLI
- Both batch and streaming APIs available
- Testing with example client

### Phase 2: Beta Testing
- Deploy to staging environment
- Invite users to test streaming feature
- Gather feedback and iterate

### Phase 3: Production
- Deploy to production
- Monitor performance and errors
- Gradually increase traffic

### Phase 4: Optimization
- Add caching for frequently traced transactions
- Implement connection pooling
- Add load balancing if needed

## Rollback Plan

If issues arise, the feature can be disabled without affecting existing functionality:

1. Stop the `prism serve` process
2. Users fall back to batch `prism trace` command
3. Web UI falls back to REST API (if implemented)
4. No data loss or corruption

## Success Metrics

Track these metrics to measure integration success:

- Server uptime and stability
- Average message latency
- Number of concurrent connections
- Error rate
- User satisfaction
- Performance vs batch loading

## Next Steps

1. Resolve dependency conflicts
2. Test with real transaction data
3. Deploy to staging environment
4. Gather user feedback
5. Iterate and improve
6. Deploy to production

## Support

For integration help:
- Discord: emry_ss
- Documentation: `docs/websocket-streaming.md`
- Examples: `examples/websocket-client.js`
