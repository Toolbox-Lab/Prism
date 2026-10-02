import { WebSocketServer, WebSocket } from "ws";

export interface TraceStreamMessage {
  type:
    | "trace_started"
    | "trace_node"
    | "resource_update"
    | "state_diff_entry"
    | "trace_completed"
    | "trace_error";
  trace_id?: string;
  [key: string]: any;
}

export function createTraceStream(port: number) {
  const wss = new WebSocketServer({ port });

  wss.on("connection", (ws: WebSocket) => {
    console.log("New trace stream client connected");

    ws.on("message", (data: Buffer) => {
      try {
        const message = JSON.parse(data.toString());

        if (message.type === "subscribe" && message.tx_hash) {
          console.log(`Client subscribed to trace: ${message.tx_hash}`);

          ws.send(
            JSON.stringify({
              type: "subscribed",
              tx_hash: message.tx_hash,
            }),
          );
        }
      } catch (err) {
        console.error("Failed to parse WebSocket message:", err);
      }
    });

    ws.on("close", () => {
      console.log("Trace stream client disconnected");
    });

    ws.on("error", (err) => {
      console.error("WebSocket error:", err);
    });
  });

  console.log(`Trace stream WebSocket server listening on port ${port}`);
  return wss;
}
