interface StateDiffViewerProps {
  entries: Array<{
    key: string;
    before?: string;
    after?: string;
    change_type: string;
  }>;
}

export function StateDiffViewer({ entries }: StateDiffViewerProps) {
  const getChangeColor = (changeType: string) => {
    switch (changeType.toLowerCase()) {
      case "created":
        return "bg-green-50 border-green-200";
      case "updated":
        return "bg-blue-50 border-blue-200";
      case "deleted":
        return "bg-red-50 border-red-200";
      default:
        return "bg-gray-50 border-gray-200";
    }
  };

  const getChangeBadge = (changeType: string) => {
    switch (changeType.toLowerCase()) {
      case "created":
        return <span className="px-2 py-1 text-xs font-medium bg-green-100 text-green-800 rounded">Created</span>;
      case "updated":
        return <span className="px-2 py-1 text-xs font-medium bg-blue-100 text-blue-800 rounded">Updated</span>;
      case "deleted":
        return <span className="px-2 py-1 text-xs font-medium bg-red-100 text-red-800 rounded">Deleted</span>;
      default:
        return <span className="px-2 py-1 text-xs font-medium bg-gray-100 text-gray-800 rounded">Unchanged</span>;
    }
  };

  return (
    <div className="bg-white rounded-lg shadow p-6">
      <h2 className="text-xl font-semibold mb-4">State Diff</h2>
      
      <div className="space-y-3">
        {entries.map((entry, idx) => (
          <div
            key={idx}
            className={`p-4 border rounded-lg ${getChangeColor(entry.change_type)}`}
          >
            <div className="flex items-center justify-between mb-2">
              <span className="font-mono text-sm font-medium">{entry.key}</span>
              {getChangeBadge(entry.change_type)}
            </div>
            
            <div className="grid grid-cols-2 gap-4 mt-3">
              <div>
                <div className="text-xs text-gray-600 mb-1">Before</div>
                <pre className="text-xs bg-white p-2 rounded border overflow-x-auto">
                  {entry.before || "(none)"}
                </pre>
              </div>
              <div>
                <div className="text-xs text-gray-600 mb-1">After</div>
                <pre className="text-xs bg-white p-2 rounded border overflow-x-auto">
                  {entry.after || "(none)"}
                </pre>
              </div>
            </div>
          </div>
        ))}
      </div>
      
      {entries.length === 0 && (
        <p className="text-gray-500 text-center py-8">
          No state changes detected yet.
        </p>
      )}
    </div>
  );
}

export default StateDiffViewer;
