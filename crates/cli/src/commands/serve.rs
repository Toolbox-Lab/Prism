use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use clap::Args;
use futures_util::{SinkExt, StreamExt as _};
use grat_core::types::config::NetworkConfig;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

#[derive(Args)]
pub struct ServeArgs {
    #[arg(long, short, default_value = "8080")]
    pub port: u16,

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
        trace_id: String,
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
    let network = Arc::new(network.clone());
    let addr: SocketAddr = format!("{}:{}", args.host, args.port).parse()?;

    let api_router = Router::new()
        .route("/trace/:tx_hash", get(get_trace_api))
        .with_state(Arc::clone(&network));

    let static_dir = get_static_assets_path();
    let static_service = if static_dir.exists() {
        tracing::info!("Serving web app from {}", static_dir.display());
        ServeDir::new(static_dir)
    } else {
        tracing::warn!(
            "Web app assets not found at {}. Serving placeholder.",
            static_dir.display()
        );
        ServeDir::new(".")
    };

    let app = Router::new()
        .route("/", get(index_handler))
        .nest("/api", api_router)
        .route("/ws", get(ws_handler))
        .fallback_service(static_service)
        .layer(TraceLayer::new_for_http())
        .with_state(Arc::clone(&network));

    println!("🚀 Grat instrumentation server starting...");
    println!("   URL: http://{addr}");
    println!("   WebSocket: ws://{addr}/ws");
    println!("   API Bridge: http://{addr}/api/trace/<tx_hash>");
    println!("   Press Ctrl+C to stop");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn index_handler() -> Html<&'static str> {
    Html(
        r#"
        <!DOCTYPE html>
        <html>
        <head>
            <title>Grat | instrumentation</title>
            <style>
                body { background: #0f172a; color: #f8fafc; font-family: system-ui; display: flex; align-items: center; justify-content: center; height: 100vh; margin: 0; }
                .card { background: #1e293b; padding: 2rem; border-radius: 1rem; border: 1px solid #334155; text-align: center; }
                h1 { color: #38bdf8; margin: 0 0 1rem; }
            </style>
        </head>
        <body>
            <div class="card">
                <h1>Grat Instrumentation</h1>
                <p>The web dashboard is being served. Connect your front-end to <code>/ws</code> or use the <code>/api</code> endpoints.</p>
            </div>
        </body>
        </html>
    "#,
    )
}

async fn get_trace_api(
    Path(tx_hash): Path<String>,
    State(network): State<Arc<NetworkConfig>>,
) -> impl IntoResponse {
    match grat_core::replay::replay_transaction(&tx_hash, &network).await {
        Ok(trace) => axum::Json(trace).into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Trace failed: {e}"),
        )
            .into_response(),
    }
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(network): State<Arc<NetworkConfig>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_connection(socket, network))
}

async fn handle_ws_connection(socket: WebSocket, network: Arc<NetworkConfig>) {
    let (mut sender, mut receiver) = socket.split();

    while let Some(Ok(msg)) = receiver.next().await {
        if let Message::Text(text) = msg {
            if let Ok(request) = serde_json::from_str::<TraceRequest>(&text) {
                let (tx, mut rx) = tokio::sync::broadcast::channel::<TraceStreamMessage>(100);
                let tx_hash = request.tx_hash.clone();
                let resume_from = request.resume_from;
                let network = Arc::clone(&network);

                tokio::spawn(async move {
                    let _ = stream_trace_replay(&tx_hash, &network, tx, resume_from).await;
                });

                while let Ok(update) = rx.recv().await {
                    let json = serde_json::to_string(&update).unwrap();
                    if sender.send(Message::Text(json)).await.is_err() {
                        break;
                    }
                }
            }
        }
    }
}

fn get_static_assets_path() -> std::path::PathBuf {
    let dirs = directories::ProjectDirs::from("com", "toolbox-lab", "grat").unwrap();
    dirs.data_dir().join("web")
}

#[derive(Debug, serde::Deserialize)]
struct TraceRequest {
    tx_hash: String,
    /// Synchronization cursor: the index of the last trace node the client has
    /// already received. When present, the server fast-forwards the replay and
    /// only streams nodes that occurred after this index (used for resuming a
    /// trace across a dropped/reconnected WebSocket instead of replaying from 0).
    #[serde(default)]
    resume_from: Option<usize>,
}

async fn stream_trace_replay(
    tx_hash: &str,
    network: &NetworkConfig,
    sender: tokio::sync::broadcast::Sender<TraceStreamMessage>,
    resume_from: Option<usize>,
) -> anyhow::Result<()> {
    use std::time::Instant;

    let start = Instant::now();

    let _ = sender.send(TraceStreamMessage::TraceStarted {
        tx_hash: tx_hash.to_string(),
        ledger_sequence: 0,
    });

    let ledger_state = match grat_core::replay::state::reconstruct_state(tx_hash, network).await {
        Ok(state) => state,
        Err(e) => {
            let _ = sender.send(TraceStreamMessage::TraceError {
                error: format!("Failed to reconstruct state: {e}"),
            });
            return Err(e.into());
        }
    };

    let _ = sender.send(TraceStreamMessage::TraceStarted {
        tx_hash: tx_hash.to_string(),
        ledger_sequence: ledger_state.ledger_sequence,
    });

    let trace_id = grat_core::types::trace::TraceId::new();
    let result =
        match grat_core::replay::sandbox::execute_with_tracing(&ledger_state, tx_hash, trace_id).await {
            Ok(r) => r,
            Err(e) => {
                let _ = sender.send(TraceStreamMessage::TraceError {
                    error: format!("Sandbox execution failed: {e}"),
                });
                return Err(e.into());
            }
        };

    let mut node_count = 0;
    for (idx, event) in result.events.iter().enumerate() {
        // Fast-forward: when the client resumes after a disconnect it tells us the
        // last node index it already received. Skip re-broadcasting those nodes so
        // the resumed stream contains only unseen trace data (no duplication, no
        // wasted bandwidth). `node_count` still advances so totals stay accurate.
        if let Some(cursor) = resume_from {
            if idx <= cursor {
                node_count += 1;
                continue;
            }
        }

        let node_json = serde_json::to_value(event)?;

        let _ = sender.send(TraceStreamMessage::TraceNode {
            node: node_json,
            path: vec![idx],
        });

        node_count += 1;

        if idx % 10 == 0 {
            let _ = sender.send(TraceStreamMessage::ResourceUpdate {
                trace_id: result.trace_id.0.clone(),
                cpu_used: result.total_cpu,
                memory_used: result.total_memory,
                cpu_limit: 100_000_000,
                memory_limit: 40 * 1024 * 1024,
            });
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
    }

    let state_diff = grat_core::replay::differ::compute_diff(&ledger_state, &result)?;
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
