#!/usr/bin/env node

/**
 * Example WebSocket client for testing Prism trace streaming
 * 
 * Usage:
 *   node examples/websocket-client.js <tx-hash>
 * 
 * Prerequisites:
 *   1. Start the Prism WebSocket server: prism serve --port 8080
 *   2. Run this script with a valid transaction hash
 */

const WebSocket = require('ws');

const TX_HASH = process.argv[2];
const WS_URL = process.env.WS_URL || 'ws://localhost:8080';

if (!TX_HASH) {
  console.error('Usage: node websocket-client.js <tx-hash>');
  process.exit(1);
}

console.log(`Connecting to ${WS_URL}...`);

const ws = new WebSocket(WS_URL);

let nodeCount = 0;
let startTime = Date.now();

ws.on('open', () => {
  console.log('✓ Connected to Prism WebSocket server');
  console.log(`Requesting trace for: ${TX_HASH}\n`);
  
  // Send trace request
  ws.send(JSON.stringify({ tx_hash: TX_HASH }));
});

ws.on('message', (data) => {
  try {
    const message = JSON.parse(data.toString());
    
    switch (message.type) {
      case 'trace_started':
        console.log('🚀 Trace started');
        console.log(`   Transaction: ${message.tx_hash}`);
        console.log(`   Ledger: ${message.ledger_sequence}\n`);
        break;
        
      case 'trace_node':
        nodeCount++;
        process.stdout.write(`\r📦 Received ${nodeCount} trace nodes...`);
        break;
        
      case 'resource_update':
        const cpuPercent = (message.cpu_used / message.cpu_limit * 100).toFixed(1);
        const memPercent = (message.memory_used / message.memory_limit * 100).toFixed(1);
        console.log(`\n📊 Resources: CPU ${cpuPercent}%, Memory ${memPercent}%`);
        break;
        
      case 'state_diff_entry':
        console.log(`\n📝 State change: ${message.key} (${message.change_type})`);
        break;
        
      case 'trace_completed':
        const duration = Date.now() - startTime;
        console.log('\n\n✅ Trace completed!');
        console.log(`   Total nodes: ${message.total_nodes}`);
        console.log(`   Server duration: ${message.duration_ms}ms`);
        console.log(`   Client duration: ${duration}ms`);
        ws.close();
        break;
        
      case 'trace_error':
        console.error('\n\n❌ Trace error:', message.error);
        ws.close();
        process.exit(1);
        break;
        
      default:
        console.log('\n⚠ Unknown message type:', message.type);
    }
  } catch (err) {
    console.error('Failed to parse message:', err);
  }
});

ws.on('error', (err) => {
  console.error('WebSocket error:', err.message);
  process.exit(1);
});

ws.on('close', () => {
  console.log('\nConnection closed');
});

// Handle Ctrl+C
process.on('SIGINT', () => {
  console.log('\n\nClosing connection...');
  ws.close();
  process.exit(0);
});
