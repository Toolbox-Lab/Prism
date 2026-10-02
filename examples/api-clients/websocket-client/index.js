#!/usr/bin/env node

const WebSocket = require("ws");

/**
 * TraceStreamClient encapsulates WebSocket connection lifecycle and message handling.
 * Provides a clean interface for establishing, managing, and gracefully closing connections
 * while isolating connection state to prevent global scope pollution.
 */
class TraceStreamClient {
  constructor(wsUrl, txHash) {
    this.wsUrl = wsUrl;
    this.txHash = txHash;
    this.ws = null;
    this.nodeCount = 0;
    this.startTime = null;
    this.isConnecting = false;
    this.isClosed = false;
  }

  /**
   * Connects to the WebSocket server and sets up all event listeners.
   * Returns a promise that resolves when the connection is established.
   */
  connect() {
    return new Promise((resolve, reject) => {
      if (this.isConnecting || this.ws) {
        reject(new Error("Already connecting or connected"));
        return;
      }

      this.isConnecting = true;
      this.startTime = Date.now();

      try {
        this.ws = new WebSocket(this.wsUrl);
        this.attachEventListeners(resolve, reject);
      } catch (err) {
        this.isConnecting = false;
        reject(err);
      }
    });
  }

  /**
   * Attaches all event listeners to the WebSocket instance.
   * This method isolates event binding logic for cleaner connection management.
   */
  attachEventListeners(resolve, reject) {
    const handleOpen = () => {
      this.isConnecting = false;
      console.log("✓ Connected to Grat WebSocket server");
      console.log(`Requesting trace for: ${this.txHash}\n`);

      this.ws.send(JSON.stringify({ tx_hash: this.txHash }));
      resolve();
    };

    const handleMessage = (data) => {
      try {
        const message = JSON.parse(data.toString());
        this.handleMessageType(message);
      } catch (err) {
        console.error("Failed to parse message:", err);
      }
    };

    const handleError = (err) => {
      this.isConnecting = false;
      console.error("WebSocket error:", err.message);
      reject(err);
    };

    const handleClose = () => {
      if (!this.isClosed) {
        console.log("\nConnection closed");
      }
      this.cleanup();
    };

    this.ws.on("open", handleOpen);
    this.ws.on("message", handleMessage);
    this.ws.on("error", handleError);
    this.ws.on("close", handleClose);
  }

  /**
   * Routes incoming messages to appropriate handlers based on message type.
   */
  handleMessageType(message) {
    switch (message.type) {
      case "trace_started":
        this.handleTraceStarted(message);
        break;

      case "trace_node":
        this.handleTraceNode();
        break;

      case "resource_update":
        this.handleResourceUpdate(message);
        break;

      case "state_diff_entry":
        this.handleStateDiffEntry(message);
        break;

      case "trace_completed":
        this.handleTraceCompleted(message);
        break;

      case "trace_error":
        this.handleTraceError(message);
        break;

      default:
        console.log("\n⚠ Unknown message type:", message.type);
    }
  }

  handleTraceStarted(message) {
    console.log("🚀 Trace started");
    console.log(`   Transaction: ${message.tx_hash}`);
    console.log(`   Ledger: ${message.ledger_sequence}\n`);
  }

  handleTraceNode() {
    this.nodeCount++;
    process.stdout.write(`\r📦 Received ${this.nodeCount} trace nodes...`);
  }

  handleResourceUpdate(message) {
    const cpuPercent = ((message.cpu_used / message.cpu_limit) * 100).toFixed(
      1,
    );
    const memPercent = (
      (message.memory_used / message.memory_limit) *
      100
    ).toFixed(1);
    console.log(`\n📊 Resources: CPU ${cpuPercent}%, Memory ${memPercent}%`);
  }

  handleStateDiffEntry(message) {
    console.log(`\n📝 State change: ${message.key} (${message.change_type})`);
  }

  handleTraceCompleted(message) {
    const duration = Date.now() - this.startTime;
    console.log("\n\n✅ Trace completed!");
    console.log(`   Total nodes: ${message.total_nodes}`);
    console.log(`   Server duration: ${message.duration_ms}ms`);
    console.log(`   Client duration: ${duration}ms`);
    this.close();
  }

  handleTraceError(message) {
    console.error("\n\n❌ Trace error:", message.error);
    this.close();
  }

  /**
   * Gracefully closes the WebSocket connection and cleans up resources.
   */
  close() {
    this.isClosed = true;
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.close();
    }
  }

  /**
   * Internal cleanup method called when connection is fully closed.
   * Removes all event listeners and releases socket reference.
   */
  cleanup() {
    if (this.ws) {
      this.ws.removeAllListeners();
      this.ws = null;
    }
  }
}

/**
 * Factory function for creating and connecting a TraceStreamClient instance.
 * Handles initialization, connection, and error scenarios.
 */
async function connectToTraceStream(wsUrl, txHash) {
  const client = new TraceStreamClient(wsUrl, txHash);
  await client.connect();
  return client;
}

// Main execution
async function main() {
  const TX_HASH = process.argv[2];
  const WS_URL = process.env.WS_URL || "ws://localhost:8080";

  if (!TX_HASH) {
    console.error("Usage: node websocket-client.js <tx-hash>");
    process.exit(1);
  }

  console.log(`Connecting to ${WS_URL}...`);

  try {
    const client = await connectToTraceStream(WS_URL, TX_HASH);

    process.on("SIGINT", () => {
      console.log("\n\nClosing connection...");
      client.close();
      process.exit(0);
    });
  } catch (err) {
    console.error("Failed to connect:", err.message);
    process.exit(1);
  }
}

main().catch((err) => {
  console.error("Unexpected error:", err);
  process.exit(1);
});

connectToTraceStream();
