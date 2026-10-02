interface ResourceProfileProps {
  profile: {
    cpu_used: number;
    memory_used: number;
    cpu_limit: number;
    memory_limit: number;
    read_bytes: number;
    read_limit: number;
    write_bytes: number;
    write_limit: number;
  };
}

function ResourceBar({
  label,
  used,
  limit,
  format,
}: {
  label: string;
  used: number;
  limit: number;
  format?: (v: number) => string;
}) {
  const percentage = limit > 0 ? (used / limit) * 100 : 0;
  const displayValue = format ? format(used) : used.toLocaleString();
  const displayLimit = format ? format(limit) : limit.toLocaleString();

  return (
    <div>
      <div className="flex justify-between mb-2">
        <span className="text-sm font-medium">{label}</span>
        <span className="text-sm text-gray-600">
          {displayValue} / {displayLimit} ({percentage.toFixed(1)}%)
        </span>
      </div>
      <div className="w-full bg-gray-200 rounded-full h-4">
        <div
          className={`h-4 rounded-full transition-all duration-300 ${
            percentage > 90
              ? "bg-red-500"
              : percentage > 70
                ? "bg-yellow-500"
                : "bg-green-500"
          }`}
          style={{ width: `${Math.min(percentage, 100)}%` }}
        />
      </div>
    </div>
  );
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB"];
  const i = Math.min(Math.floor(Math.log2(bytes) / 10), units.length - 1);
  return `${(bytes / (1 << (i * 10))).toFixed(i > 0 ? 2 : 0)} ${units[i]}`;
}

export function ResourceProfile({ profile }: ResourceProfileProps) {
  return (
    <div className="bg-white rounded-lg shadow p-6">
      <h2 className="text-xl font-semibold mb-4">Resource Profile</h2>

      <div className="space-y-6">
        <ResourceBar
          label="CPU Instructions"
          used={profile.cpu_used}
          limit={profile.cpu_limit}
        />
        <ResourceBar
          label="Memory"
          used={profile.memory_used}
          limit={profile.memory_limit}
          format={(v) => formatBytes(v)}
        />
        <ResourceBar
          label="Read Bytes"
          used={profile.read_bytes}
          limit={profile.read_limit}
          format={(v) => formatBytes(v)}
        />
        <ResourceBar
          label="Write Bytes"
          used={profile.write_bytes}
          limit={profile.write_limit}
          format={(v) => formatBytes(v)}
        />
      </div>

      {profile.cpu_limit > 0 && profile.cpu_used / profile.cpu_limit > 0.9 && (
        <div className="mt-4 p-3 bg-yellow-50 border border-yellow-200 rounded">
          <p className="text-sm text-yellow-800">
            Resource usage is approaching limits. Consider optimizing contract
            code.
          </p>
        </div>
      )}
    </div>
  );
}

export default ResourceProfile;
