//! `prism serve` — Start a local web server to host the Prism Web UI.

use anyhow::Result;
use clap::Args;
use std::io::prelude::*;
use std::net::{TcpListener, TcpStream};
use std::thread;

/// Arguments for the serve command.
#[derive(Args)]
pub struct ServeArgs {
    /// Port to bind the server to (default: 3000).
    #[arg(long, short = 'p', default_value = "3000")]
    pub port: u16,

    /// Host to bind the server to (default: 127.0.0.1).
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,

    /// Open the default browser automatically after starting.
    #[arg(long)]
    pub open: bool,
}

/// Execute the serve command.
pub async fn run(args: ServeArgs) -> Result<()> {
    let addr = format!("{}:{}", args.host, args.port);

    let listener = TcpListener::bind(&addr)
        .map_err(|e| anyhow::anyhow!("Failed to bind to {}: {}", addr, e))?;

    println!("🌐 Prism dashboard available at: http://{}", addr);
    println!("🔄 Press Ctrl+C to stop the server");

    if args.open {
        open_browser(&format!("http://{}", addr));
    }

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                thread::spawn(move || {
                    if let Err(e) = handle_connection(stream) {
                        eprintln!("Connection error: {e}");
                    }
                });
            }
            Err(e) => eprintln!("Accept error: {e}"),
        }
    }

    Ok(())
}

fn handle_connection(mut stream: TcpStream) -> Result<()> {
    let mut buffer = [0; 1024];
    stream.read(&mut buffer)?;

    let request = String::from_utf8_lossy(&buffer);
    let first_line = request.lines().next().unwrap_or("");

    let (status, content_type, body) = if first_line.starts_with("GET /api/health") {
        ("200 OK", "application/json", get_health_json())
    } else if first_line.starts_with("GET /") {
        ("200 OK", "text/html", get_index_html())
    } else {
        (
            "405 Method Not Allowed",
            "text/plain",
            "Method not allowed".to_string(),
        )
    };

    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\n\r\n{body}",
        body.len()
    );

    stream.write_all(response.as_bytes())?;
    stream.flush()?;
    Ok(())
}

fn get_health_json() -> String {
    r#"{"status":"ok","service":"prism"}"#.to_string()
}

fn get_index_html() -> String {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Prism — Soroban Debugger</title>
    <style>
        body { font-family: system-ui, sans-serif; margin: 0; padding: 2rem; background: #f8fafc; }
        .container { max-width: 800px; margin: 0 auto; }
        .header { text-align: center; margin-bottom: 3rem; }
        .logo { font-size: 2.5rem; font-weight: bold; color: #1e40af; margin-bottom: 0.5rem; }
        .subtitle { color: #64748b; font-size: 1.1rem; }
        .card { background: white; border-radius: 8px; padding: 2rem; box-shadow: 0 1px 3px rgba(0,0,0,0.1); margin-bottom: 1.5rem; }
        .card h2 { margin-top: 0; color: #1e293b; }
        .form-group { margin-bottom: 1rem; }
        label { display: block; margin-bottom: 0.4rem; font-weight: 500; color: #374151; }
        input, select { width: 100%; padding: 0.5rem 0.75rem; border: 1px solid #d1d5db; border-radius: 6px; font-size: 1rem; box-sizing: border-box; }
        button { background: #1e40af; color: white; padding: 0.6rem 1.5rem; border: none; border-radius: 6px; cursor: pointer; font-size: 1rem; }
        button:hover { background: #1d4ed8; }
        .status { padding: 0.75rem 1rem; border-radius: 6px; margin-top: 1rem; display: none; }
        .status.success { background: #dcfce7; color: #166534; }
        .status.error { background: #fef2f2; color: #991b1b; }
        .commands { list-style: none; padding: 0; margin: 0; }
        .commands li { padding: 0.5rem 0; border-bottom: 1px solid #f1f5f9; color: #475569; }
        .commands li:last-child { border-bottom: none; }
        .commands code { background: #f1f5f9; padding: 0.1rem 0.4rem; border-radius: 4px; font-size: 0.9rem; }
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <div class="logo">🔆 Prism</div>
            <div class="subtitle">Soroban Transaction Debugger</div>
        </div>

        <div class="card">
            <h2>Analyze a Transaction</h2>
            <div class="form-group">
                <label for="tx-hash">Transaction Hash</label>
                <input type="text" id="tx-hash" placeholder="64-character hex hash...">
            </div>
            <div class="form-group">
                <label for="network">Network</label>
                <select id="network">
                    <option value="testnet">Testnet</option>
                    <option value="mainnet">Mainnet</option>
                    <option value="futurenet">Futurenet</option>
                </select>
            </div>
            <button onclick="analyzeTransaction()">Analyze</button>
            <div id="status" class="status"></div>
        </div>

        <div class="card">
            <h2>CLI Commands</h2>
            <ul class="commands">
                <li><code>prism decode &lt;hash&gt;</code> — Translate errors into plain English</li>
                <li><code>prism inspect &lt;hash&gt;</code> — Full transaction context and metadata</li>
                <li><code>prism trace &lt;hash&gt;</code> — Step-by-step execution replay</li>
                <li><code>prism profile &lt;hash&gt;</code> — Resource consumption analysis</li>
                <li><code>prism diff &lt;hash&gt;</code> — State changes before/after</li>
                <li><code>prism whatif &lt;hash&gt; --modify patch.json</code> — Re-simulate with changes</li>
            </ul>
        </div>
    </div>

    <script>
        function analyzeTransaction() {
            var txHash = document.getElementById("tx-hash").value;
            var network = document.getElementById("network").value;
            var status = document.getElementById("status");

            if (!txHash || txHash.length !== 64) {
                status.textContent = "Please enter a valid 64-character transaction hash.";
                status.className = "status error";
                status.style.display = "block";
                return;
            }

            status.textContent = "Run: prism decode " + txHash + " --network " + network;
            status.className = "status success";
            status.style.display = "block";
        }
    </script>
</body>
</html>"#.to_string()
}

fn open_browser(url: &str) {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }

    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", url])
            .spawn();
    }

    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
}
