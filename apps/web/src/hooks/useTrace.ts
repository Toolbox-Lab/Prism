// Hook for execution trace via backend API with WebSocket streaming support
import { useState, useCallback } from "react";
import { useWebSocket, TraceStreamCallbacks } from "./useWebSocket";

export interface TraceData {
  tx_hash: string;
  ledger_sequence: number;
  nodes: any[];
  resource_profile?: {
    cpu_used: number;
    memory_used: number;
    cpu_limit: number;
    memory_limit: number;
  };
  state_diff: any[];
  completed: boolean;
  error?: string;
}

export function useTrace(wsUrl?: string) {
  const [trace, setTrace] = useState<TraceData | null>(null);
  const [loading, setLoading] = useState(false);

  const callbacks: TraceStreamCallbacks = {
    onTraceStarted: (data) => {
      setTrace({
        tx_hash: data.tx_hash,
        ledger_sequence: data.ledger_sequence,
        nodes: [],
        state_diff: [],
        completed: false,
      });
      setLoading(true);
    },
    onTraceNode: (data) => {
      setTrace((prev) => {
        if (!prev) return null;
        return {
          ...prev,
          nodes: [...prev.nodes, data],
        };
      });
    },
    onResourceUpdate: (data) => {
      setTrace((prev) => {
        if (!prev) return null;
        return {
          ...prev,
          resource_profile: data,
        };
      });
    },
    onStateDiffEntry: (data) => {
      setTrace((prev) => {
        if (!prev) return null;
        return {
          ...prev,
          state_diff: [...prev.state_diff, data],
        };
      });
    },
    onTraceCompleted: (data) => {
      setTrace((prev) => {
        if (!prev) return null;
        return {
          ...prev,
          completed: true,
        };
      });
      setLoading(false);
    },
    onTraceError: (data) => {
      setTrace((prev) => {
        if (!prev) return null;
        return {
          ...prev,
          error: data.error,
          completed: true,
        };
      });
      setLoading(false);
    },
  };

  const { connected, error: wsError, requestTrace: wsRequestTrace } = useWebSocket(
    wsUrl || "",
    callbacks
  );

  const requestTrace = useCallback(
    (txHash: string, network: string) => {
      if (wsUrl && connected) {
        // Use WebSocket streaming
        wsRequestTrace(txHash);
      } else {
        // Fallback to REST API
        setLoading(true);
        // TODO: Call backend replay API
        setLoading(false);
      }
    },
    [wsUrl, connected, wsRequestTrace]
  );

  return { 
    trace, 
    loading, 
    requestTrace, 
    streaming: !!wsUrl && connected,
    streamError: wsError,
  };
}

