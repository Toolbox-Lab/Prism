import type { TransactionContext, FeeBreakdown } from "@/lib/types";

interface Props {
  context: TransactionContext;
}

function formatFee(stroops: number): string {
  return `${stroops.toLocaleString()} stroops`;
}

function formatOptFee(stroops?: number): string {
  return stroops !== undefined ? formatFee(stroops) : "N/A";
}

function FeeRow({
  label,
  value,
  sub,
  variant = "default",
}: {
  label: string;
  value: string;
  sub?: boolean;
  variant?: "default" | "accent" | "success" | "warning" | "muted" | "meta";
}) {
  const variantClass: Record<string, string> = {
    default: "text-gray-200",
    accent: "text-white font-semibold",
    success: "text-green-400",
    warning: "text-yellow-400",
    muted: "text-gray-400",
    meta: "text-blue-300",
  };

  return (
    <div className={`flex justify-between ${sub ? "pl-6" : ""}`}>
      <span className="text-gray-400">{label}</span>
      <span className={variantClass[variant]}>{value}</span>
    </div>
  );
}

function FeeBreakdownPanel({ fee }: { fee: FeeBreakdown }) {
  return (
    <section aria-labelledby="fee-breakdown-heading">
      <h3
        id="fee-breakdown-heading"
        className="text-xs font-semibold uppercase tracking-wider text-gray-500 mb-2"
      >
        Fee Breakdown
      </h3>
      <div className="space-y-1 text-sm">
        <FeeRow
          label="Bid Fee"
          value={formatOptFee(fee.bid_fee)}
          variant="meta"
        />
        <FeeRow
          label="Total Charged Fee"
          value={formatFee(fee.total_charged_fee)}
          variant="accent"
        />
        <FeeRow
          label="Inclusion Fee"
          value={formatFee(fee.inclusion_fee)}
          variant="success"
        />
        <FeeRow
          label="Resource Fee"
          value={formatFee(fee.resource_fee)}
          variant="warning"
        />
        {fee.resource_fee > 0 && (
          <>
            <FeeRow
              label="Refundable Resource Fee"
              value={formatFee(fee.refundable_resource_fee)}
              sub
              variant="muted"
            />
            <FeeRow
              label="Non-Refundable Resource Fee"
              value={formatFee(fee.non_refundable_fee)}
              sub
              variant="muted"
            />
          </>
        )}
      </div>
      {fee.resource_fee > 0 && (
        <p className="mt-2 text-xs text-gray-500">
          The refundable portion ({formatFee(fee.refundable_resource_fee)}) may
          be partially returned if unused resources are reclaimed after
          execution.
        </p>
      )}
    </section>
  );
}

function ResourcePanel({ context }: { context: TransactionContext }) {
  const { resources } = context;
  const cpuPct =
    resources.cpu_instructions_limit > 0
      ? Math.min(
          (resources.cpu_instructions_used / resources.cpu_instructions_limit) *
            100,
          100,
        )
      : 0;
  const memPct =
    resources.memory_bytes_limit > 0
      ? Math.min(
          (resources.memory_bytes_used / resources.memory_bytes_limit) * 100,
          100,
        )
      : 0;

  function barColor(pct: number): string {
    if (pct >= 90) return "bg-red-500";
    if (pct >= 70) return "bg-yellow-400";
    return "bg-green-500";
  }

  function UsageBar({
    label,
    used,
    limit,
    pct,
  }: {
    label: string;
    used: number;
    limit: number;
    pct: number;
  }) {
    return (
      <div>
        <div className="flex justify-between text-xs text-gray-400 mb-1">
          <span>{label}</span>
          <span>
            {used.toLocaleString()} / {limit.toLocaleString()} ({pct.toFixed(0)}
            %)
          </span>
        </div>
        <div
          className="w-full h-2 rounded bg-gray-700"
          role="progressbar"
          aria-valuenow={pct}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-label={`${label} usage`}
        >
          <div
            className={`h-2 rounded ${barColor(pct)}`}
            style={{ width: `${pct}%` }}
          />
        </div>
      </div>
    );
  }

  return (
    <section aria-labelledby="resource-usage-heading">
      <h3
        id="resource-usage-heading"
        className="text-xs font-semibold uppercase tracking-wider text-gray-500 mb-2"
      >
        Resource Usage
      </h3>
      <div className="space-y-3">
        <UsageBar
          label="CPU Instructions"
          used={resources.cpu_instructions_used}
          limit={resources.cpu_instructions_limit}
          pct={cpuPct}
        />
        <UsageBar
          label="Memory"
          used={resources.memory_bytes_used}
          limit={resources.memory_bytes_limit}
          pct={memPct}
        />
      </div>
    </section>
  );
}

export default function ContextSummary({ context }: Props) {
  return (
    <div className="rounded-lg border border-gray-700 bg-gray-900 p-4 space-y-6 text-sm">
      {}
      <section aria-labelledby="tx-summary-heading">
        <h3
          id="tx-summary-heading"
          className="text-xs font-semibold uppercase tracking-wider text-gray-500 mb-2"
        >
          Transaction Summary
        </h3>
        <div className="space-y-1">
          <div className="flex justify-between">
            <span className="text-gray-400">TX Hash</span>
            <span className="font-mono text-xs text-gray-200 truncate max-w-[60%]">
              {context.tx_hash}
            </span>
          </div>
          <div className="flex justify-between">
            <span className="text-gray-400">Ledger</span>
            <span className="text-gray-200">{context.ledger_sequence}</span>
          </div>
          {context.function_name && (
            <div className="flex justify-between">
              <span className="text-gray-400">Function</span>
              <span className="font-mono text-xs text-gray-200">
                {context.function_name}
              </span>
            </div>
          )}
        </div>
      </section>

      <FeeBreakdownPanel fee={context.fee} />
      <ResourcePanel context={context} />
    </div>
  );
}
