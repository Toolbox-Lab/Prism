//! `prism serve` — Start WebSocket server for streaming trace updates.

use clap::Args;
use futures_util::{SinkExt, StreamExt};
use prism_core::types::config::NetworkConfig;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio_tungstenite::{accept_async, tungstenite::Message};

#[derive(Args)]
pub struct ServeArgs {
    /// Port to listen on for WebSocket connections.
    #[arg(long, short, default_value = "8080")]
    pub port: u16,

    /// Host to bind to.
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TraceStreamMessage {
    TraceStarted {
        tx_hash: String,
        ledger_sequence: u32,
    },
    TraceNode {
        node: serde_json::Value,
        path: Vec<usize>,
    },
    ResourceUpdate {
        cpu_used: u64,
        memory_used: u64,
        cpu_limit: u64,
        memory_limit: u64,
    },
    StateDiffEntry {
        key: String,
        before: Option<String>,
        after: Option<String>,
        change_type: String,
    },
    TraceCompleted {
        total_nodes: usize,
        duration_ms: u64,
    },
    TraceError {
        error: String,
    },
}

pub async fn run(args: ServeArgs, network: &NetworkConfig) -> anyhow::Result<()> {
    let addr: SocketAddr = format!("{}:{}", args.host, args.port).parse()?;
    let listener = TcpListener::bind(&addr).await?;

    println!("🚀 Prism WebSocket server listening on ws://{}", addr);
    println!("   Ready to stream trace updates to connected clients");
    println!("   Press Ctrl+C to stop");

    let network = Arc::new(network.clone());

    loop {
        match listener.accept().await {
            Ok((stream, peer_addr)) => {
                let network = Arc::clone(&network);
                tokio::spawn(handle_connection(stream, peer_addr, network));
            }
            Err(e) => {
                tracing::error!("Failed to accept connection: {}", e);
            }
        }
    }
}

async fn handle_connection(stream: TcpStream, peer_addr: SocketAddr, network: Arc<NetworkConfig>) {
    tracing::info!("New WebSocket connection from {}", peer_addr);

    let ws_stream = match accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            tracing::error!("WebSocket handshake failed: {}", e);
            return;
        }
    };

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    while let Some(msg) = ws_receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                if let Ok(request) = serde_json::from_str::<TraceRequest>(&text) {
                    tracing::info!("Received trace request for tx: {}", request.tx_hash);

                    let (tx, mut rx) = broadcast::channel::<TraceStreamMessage>(100);

                    let tx_hash = request.tx_hash.clone();
                    let network = Arc::clone(&network);
                    tokio::spawn(async move {
                        if let Err(e) = stream_trace_replay(&tx_hash, &network, tx).await {
                            tracing::error!("Trace replay failed: {}", e);
                        }
                    });

                    while let Ok(update) = rx.recv().await {
                        let json = match serde_json::to_string(&update) {
                            Ok(j) => j,
                            Err(e) => {
                                tracing::error!("Failed to serialize trace update: {}", e);
                                continue;
                            }
                        };

                        if let Err(e) = ws_sender.send(Message::Text(json)).await {
                            tracing::error!("Failed to send WebSocket message: {}", e);
                            break;
                        }
                    }
                } else {
                    tracing::warn!("Invalid trace request: {}", text);
                }
            }
            Ok(Message::Close(_)) => {
                tracing::info!("Client {} closed connection", peer_addr);
                break;
            }
            Ok(Message::Ping(data)) => {
                if let Err(e) = ws_sender.send(Message::Pong(data)).await {
                    tracing::error!("Failed to send pong: {}", e);
                    break;
                }
            }
            Err(e) => {
                tracing::error!("WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }

    tracing::info!("Connection closed: {}", peer_addr);
}

#[derive(Debug, serde::Deserialize)]
struct TraceRequest {
    tx_hash: String,
}

async fn stream_trace_replay(
    tx_hash: &str,
    network: &NetworkConfig,
    sender: broadcast::Sender<TraceStreamMessage>,
) -> anyhow::Result<()> {
    use std::time::Instant;

    let start = Instant::now();

    let _ = sender.send(TraceStreamMessage::TraceStarted {
        tx_hash: tx_hash.to_string(),
        ledger_sequence: 0,
    });

    let ledger_state = match prism_core::replay::state::reconstruct_state(tx_hash, network).await {
        Ok(state) => state,
        Err(e) => {
            let _ = sender.send(TraceStreamMessage::TraceError {
                error: format!("Failed to reconstruct state: {}", e),
            });
            return Err(e.into());
        }
    };

    let _ = sender.send(TraceStreamMessage::TraceStarted {
        tx_hash: tx_hash.to_string(),
        ledger_sequence: ledger_state.ledger_sequence,
    });

    let result =
        match prism_core::replay::sandbox::execute_with_tracing(&ledger_state, tx_hash).await {
            Ok(r) => r,
            Err(e) => {
                let _ = sender.send(TraceStreamMessage::TraceError {
                    error: format!("Sandbox execution failed: {}", e),
                });
                return Err(e.into());
            }
        };

    let mut node_count = 0;
    for (idx, event) in result.events.iter().enumerate() {
        let node_json = serde_json::to_value(event)?;

        let _ = sender.send(TraceStreamMessage::TraceNode {
            node: node_json,
            path: vec![idx],
        });

        node_count += 1;

        if idx % 10 == 0 {
            let _ = sender.send(TraceStreamMessage::ResourceUpdate {
                cpu_used: result.total_cpu,
                memory_used: result.total_memory,
                cpu_limit: 100_000_000,
                memory_limit: 40 * 1024 * 1024,
            });
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
    }

    let state_diff = prism_core::replay::differ::compute_diff(&ledger_state, &result)?;
    for entry in &state_diff.entries {
        let _ = sender.send(TraceStreamMessage::StateDiffEntry {
            key: entry.key.clone(),
            before: entry.before.clone(),
            after: entry.after.clone(),
            change_type: format!("{:?}", entry.change_type),
        });
    }

    let duration_ms = start.elapsed().as_millis() as u64;
    let _ = sender.send(TraceStreamMessage::TraceCompleted {
        total_nodes: node_count,
        duration_ms,
    });

    Ok(())
}
