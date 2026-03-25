interface ExecutionTimelineProps {
  nodes: any[];
}

export function ExecutionTimeline({ nodes }: ExecutionTimelineProps) {
  return (
    <div className="bg-white rounded-lg shadow p-6">
      <h2 className="text-xl font-semibold mb-4">Execution Timeline</h2>
      <div className="space-y-2">
        {nodes.map((node, idx) => (
          <div
            key={idx}
            className="p-3 border border-gray-200 rounded hover:bg-gray-50 transition-colors"
          >
            <div className="flex items-center justify-between">
              <div className="flex items-center space-x-3">
                <span className="text-xs font-mono text-gray-500">
                  #{idx}
                </span>
                <span className="font-mono text-sm">
                  {node.node?.event_type || "Unknown"}
                </span>
              </div>
              <span className="text-xs text-gray-500">
                Path: {node.path?.join(" → ") || "root"}
              </span>
            </div>
            {node.node?.data && (
              <pre className="mt-2 text-xs text-gray-600 overflow-x-auto">
                {JSON.stringify(node.node.data, null, 2)}
              </pre>
            )}
          </div>
        ))}
      </div>
      {nodes.length === 0 && (
        <p className="text-gray-500 text-center py-8">
          No trace nodes yet. Waiting for trace data...
        </p>
      )}
    </div>
  );
}

export default ExecutionTimeline;
