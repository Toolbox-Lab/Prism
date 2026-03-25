"use client";

import { useState } from "react";
import { useTrace } from "@/hooks/useTrace";
import { ExecutionTimeline } from "@/components/trace/ExecutionTimeline";
import { StateDiffViewer } from "@/components/trace/StateDiffViewer";
import { ResourceProfile } from "@/components/trace/ResourceProfile";
import { LoadingSpinner } from "@/components/shared/LoadingSpinner";

export default function TracePage() {
  const [txHash, setTxHash] = useState("");
  const [network, setNetwork] = useState("testnet");
  
  // WebSocket URL for streaming trace updates
  const wsUrl = typeof window !== "undefined" 
    ? `ws://${window.location.hostname}:8080` 
    : "";
  
  const { trace, loading, requestTrace, streaming, streamError } = useTrace(wsUrl);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (txHash) {
      requestTrace(txHash, network);
    }
  };

  return (
    <main className="container mx-auto p-6">
      <h1 className="text-3xl font-bold mb-6">Execution Trace</h1>
      
      <form onSubmit={handleSubmit} className="mb-8 space-y-4">
        <div>
          <label htmlFor="txHash" className="block text-sm font-medium mb-2">
            Transaction Hash
          </label>
          <input
            id="txHash"
            type="text"
            value={txHash}
            onChange={(e) => setTxHash(e.target.value)}
            placeholder="Enter transaction hash..."
            className="w-full px-4 py-2 border rounded-lg focus:ring-2 focus:ring-blue-500"
          />
        </div>
        
        <div>
          <label htmlFor="network" className="block text-sm font-medium mb-2">
            Network
          </label>
          <select
            id="network"
            value={network}
            onChange={(e) => setNetwork(e.target.value)}
            className="w-full px-4 py-2 border rounded-lg focus:ring-2 focus:ring-blue-500"
          >
            <option value="testnet">Testnet</option>
            <option value="mainnet">Mainnet</option>
            <option value="futurenet">Futurenet</option>
          </select>
        </div>
        
        <button
          type="submit"
          disabled={loading || !txHash}
          className="px-6 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed"
        >
          {loading ? "Tracing..." : "Trace Transaction"}
        </button>
        
        {streaming && (
          <div className="text-sm text-green-600">
            ✓ Streaming enabled - trace will appear incrementally
          </div>
        )}
        
        {streamError && (
          <div className="text-sm text-red-600">
            ⚠ Streaming unavailable: {streamError}
          </div>
        )}
      </form>

      {loading && (
        <div className="flex items-center justify-center py-12">
          <LoadingSpinner />
          <span className="ml-3 text-gray-600">
            {trace?.nodes.length 
              ? `Processing trace nodes (${trace.nodes.length} received)...` 
              : "Reconstructing state and starting replay..."}
          </span>
        </div>
      )}

      {trace && (
        <div className="space-y-8">
          <div className="bg-white rounded-lg shadow p-6">
            <h2 className="text-xl font-semibold mb-4">Transaction Info</h2>
            <dl className="grid grid-cols-2 gap-4">
              <div>
                <dt className="text-sm text-gray-600">Transaction Hash</dt>
                <dd className="font-mono text-sm">{trace.tx_hash}</dd>
              </div>
              <div>
                <dt className="text-sm text-gray-600">Ledger Sequence</dt>
                <dd className="font-mono text-sm">{trace.ledger_sequence}</dd>
              </div>
              <div>
                <dt className="text-sm text-gray-600">Trace Nodes</dt>
                <dd className="font-mono text-sm">
                  {trace.nodes.length} {trace.completed ? "(complete)" : "(streaming...)"}
                </dd>
              </div>
              <div>
                <dt className="text-sm text-gray-600">State Changes</dt>
                <dd className="font-mono text-sm">{trace.state_diff.length}</dd>
              </div>
            </dl>
            
            {trace.error && (
              <div className="mt-4 p-4 bg-red-50 border border-red-200 rounded-lg">
                <p className="text-red-800 font-medium">Error</p>
                <p className="text-red-600 text-sm mt-1">{trace.error}</p>
              </div>
            )}
          </div>

          {trace.resource_profile && (
            <ResourceProfile profile={trace.resource_profile} />
          )}

          {trace.nodes.length > 0 && (
            <ExecutionTimeline nodes={trace.nodes} />
          )}

          {trace.state_diff.length > 0 && (
            <StateDiffViewer entries={trace.state_diff} />
          )}
        </div>
      )}
    </main>
  );
}

