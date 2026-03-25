// WebSocket hook for streaming trace data during replay
import { useEffect, useRef, useState, useCallback } from "react";

export interface TraceNode {
  node: any;
  path: number[];
}

export interface ResourceUpdate {
  cpu_used: number;
  memory_used: number;
  cpu_limit: number;
  memory_limit: number;
}

export interface StateDiffEntry {
  key: string;
  before?: string;
  after?: string;
  change_type: string;
}

export interface TraceStreamCallbacks {
  onTraceStarted?: (data: { tx_hash: string; ledger_sequence: number }) => void;
  onTraceNode?: (data: TraceNode) => void;
  onResourceUpdate?: (data: ResourceUpdate) => void;
  onStateDiffEntry?: (data: StateDiffEntry) => void;
  onTraceCompleted?: (data: { total_nodes: number; duration_ms: number }) => void;
  onTraceError?: (data: { error: string }) => void;
}

export function useWebSocket(url: string, callbacks?: TraceStreamCallbacks) {
  const wsRef = useRef<WebSocket | null>(null);
  const [connected, setConnected] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!url) return;

    const ws = new WebSocket(url);
    wsRef.current = ws;

    ws.onopen = () => {
      console.log("WebSocket connected");
      setConnected(true);
      setError(null);
    };

    ws.onmessage = (event) => {
      try {
        const message = JSON.parse(event.data);
        
        switch (message.type) {
          case "trace_started":
            callbacks?.onTraceStarted?.(message);
            break;
          case "trace_node":
            callbacks?.onTraceNode?.(message);
            break;
          case "resource_update":
            callbacks?.onResourceUpdate?.(message);
            break;
          case "state_diff_entry":
            callbacks?.onStateDiffEntry?.(message);
            break;
          case "trace_completed":
            callbacks?.onTraceCompleted?.(message);
            break;
          case "trace_error":
            callbacks?.onTraceError?.(message);
            setError(message.error);
            break;
          default:
            console.warn("Unknown message type:", message.type);
        }
      } catch (err) {
        console.error("Failed to parse WebSocket message:", err);
      }
    };

    ws.onerror = (event) => {
      console.error("WebSocket error:", event);
      setError("WebSocket connection error");
    };

    ws.onclose = () => {
      console.log("WebSocket disconnected");
      setConnected(false);
    };

    return () => {
      ws.close();
    };
  }, [url, callbacks]);

  const sendMessage = useCallback((message: any) => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      wsRef.current.send(JSON.stringify(message));
    } else {
      console.warn("WebSocket is not connected");
    }
  }, []);

  const requestTrace = useCallback((txHash: string) => {
    sendMessage({ tx_hash: txHash });
  }, [sendMessage]);

  return { 
    connected, 
    error, 
    sendMessage, 
    requestTrace 
  };
}
