"use client";

import { useState } from "react";

export type BreakpointCondition =
  | { FunctionEntry: { contract_id: string | null; function_name: string } }
  | { HostFunction: { function_name: string } }
  | { ContractCall: { target_contract_id: string } }
  | { BudgetThreshold: { cpu_instructions: number } };

export interface Breakpoint {
  id: number;
  condition: BreakpointCondition;
  enabled: boolean;
  label: string | null;
  hit_count: number;
}

type Kind = "FunctionEntry" | "HostFunction" | "ContractCall" | "BudgetThreshold";

interface BreakpointManagerProps {
  breakpoints?: Breakpoint[];
  onChange?: (breakpoints: Breakpoint[]) => void;
}

export function describeCondition(condition: BreakpointCondition): string {
  if ("FunctionEntry" in condition) {
    const { contract_id, function_name } = condition.FunctionEntry;
    return contract_id
      ? `Function entry: ${function_name} @ ${contract_id}`
      : `Function entry: ${function_name}`;
  }
  if ("HostFunction" in condition) {
    return `Host function: ${condition.HostFunction.function_name}`;
  }
  if ("ContractCall" in condition) {
    return `Contract call: ${condition.ContractCall.target_contract_id}`;
  }
  return `CPU ≥ ${condition.BudgetThreshold.cpu_instructions.toLocaleString()} instructions`;
}

const inputClass =
  "w-full bg-slate-950 border border-slate-800 rounded-lg px-3 py-2.5 text-slate-200 placeholder-slate-600 focus:outline-none focus:border-violet-500 transition-colors";
const labelClass =
  "block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-2";

export default function BreakpointManager({
  breakpoints: initial = [],
  onChange,
}: BreakpointManagerProps) {
  const [breakpoints, setBreakpoints] = useState<Breakpoint[]>(initial);
  const [nextId, setNextId] = useState<number>(
    initial.reduce((max, bp) => Math.max(max, bp.id), 0) + 1,
  );
  const [kind, setKind] = useState<Kind>("FunctionEntry");
  const [contractId, setContractId] = useState("");
  const [functionName, setFunctionName] = useState("");
  const [cpuLimit, setCpuLimit] = useState("");
  const [error, setError] = useState<string | null>(null);

  const update = (next: Breakpoint[]) => {
    setBreakpoints(next);
    onChange?.(next);
  };

  const buildCondition = (): BreakpointCondition | string => {
    const contract = contractId.trim();
    const fn = functionName.trim();
    switch (kind) {
      case "FunctionEntry":
        if (!fn) return "Function name is required.";
        return {
          FunctionEntry: { contract_id: contract || null, function_name: fn },
        };
      case "HostFunction":
        if (!fn) return "Host function name is required.";
        return { HostFunction: { function_name: fn } };
      case "ContractCall":
        if (!contract) return "Contract address is required.";
        return { ContractCall: { target_contract_id: contract } };
      case "BudgetThreshold": {
        const limit = Number(cpuLimit);
        if (!Number.isInteger(limit) || limit <= 0) {
          return "CPU instruction limit must be a positive integer.";
        }
        return { BudgetThreshold: { cpu_instructions: limit } };
      }
    }
  };

  const handleAdd = (e: React.FormEvent) => {
    e.preventDefault();
    const condition = buildCondition();
    if (typeof condition === "string") {
      setError(condition);
      return;
    }
    setError(null);
    update([
      ...breakpoints,
      { id: nextId, condition, enabled: true, label: null, hit_count: 0 },
    ]);
    setNextId(nextId + 1);
    setContractId("");
    setFunctionName("");
    setCpuLimit("");
  };

  const toggle = (id: number) =>
    update(
      breakpoints.map((bp) =>
        bp.id === id ? { ...bp, enabled: !bp.enabled } : bp,
      ),
    );

  const remove = (id: number) => update(breakpoints.filter((bp) => bp.id !== id));

  const showContract = kind === "FunctionEntry" || kind === "ContractCall";
  const showFunction = kind === "FunctionEntry" || kind === "HostFunction";

  return (
    <div className="bg-slate-900 border border-slate-800 rounded-xl p-6 shadow-xl max-w-3xl mx-auto my-6 text-slate-100">
      <h2 className="text-xl font-bold tracking-tight text-violet-400 mb-4">
        Breakpoints
      </h2>

      <form onSubmit={handleAdd} className="space-y-4">
        <div>
          <label htmlFor="bp-kind" className={labelClass}>
            Type
          </label>
          <select
            id="bp-kind"
            value={kind}
            onChange={(e) => {
              setKind(e.target.value as Kind);
              setError(null);
            }}
            className={inputClass}
          >
            <option value="FunctionEntry">Function entry</option>
            <option value="HostFunction">Host function</option>
            <option value="ContractCall">Contract call</option>
            <option value="BudgetThreshold">CPU budget threshold</option>
          </select>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {showContract && (
            <div>
              <label htmlFor="bp-contract" className={labelClass}>
                Contract address{kind === "FunctionEntry" ? " (optional)" : ""}
              </label>
              <input
                id="bp-contract"
                type="text"
                value={contractId}
                onChange={(e) => setContractId(e.target.value)}
                placeholder="C..."
                className={inputClass}
              />
            </div>
          )}
          {showFunction && (
            <div>
              <label htmlFor="bp-function" className={labelClass}>
                Function name
              </label>
              <input
                id="bp-function"
                type="text"
                value={functionName}
                onChange={(e) => setFunctionName(e.target.value)}
                placeholder="e.g. transfer"
                className={inputClass}
              />
            </div>
          )}
          {kind === "BudgetThreshold" && (
            <div>
              <label htmlFor="bp-cpu" className={labelClass}>
                CPU instruction limit
              </label>
              <input
                id="bp-cpu"
                type="number"
                min="1"
                value={cpuLimit}
                onChange={(e) => setCpuLimit(e.target.value)}
                placeholder="e.g. 5000000"
                className={inputClass}
              />
            </div>
          )}
        </div>

        {error && (
          <p role="alert" className="text-sm text-red-400">
            {error}
          </p>
        )}

        <div className="flex justify-end">
          <button
            type="submit"
            className="bg-violet-600 hover:bg-violet-500 text-white font-medium px-6 py-2.5 rounded-lg transition-colors focus:outline-none focus:ring-2 focus:ring-violet-500 focus:ring-offset-2 focus:ring-offset-slate-900"
          >
            Add Breakpoint
          </button>
        </div>
      </form>

      <ul className="mt-6 space-y-2">
        {breakpoints.length === 0 && (
          <li className="text-sm text-slate-500">No breakpoints set.</li>
        )}
        {breakpoints.map((bp) => (
          <li
            key={bp.id}
            className="flex items-center gap-3 bg-slate-950 border border-slate-800 rounded-lg px-4 py-3"
          >
            <button
              type="button"
              role="switch"
              aria-checked={bp.enabled}
              aria-label={`${bp.enabled ? "Disable" : "Enable"} breakpoint ${bp.id}`}
              onClick={() => toggle(bp.id)}
              className={`relative inline-flex h-6 w-11 shrink-0 items-center rounded-full transition-colors focus:outline-none focus:ring-2 focus:ring-violet-500 ${
                bp.enabled ? "bg-violet-600" : "bg-slate-700"
              }`}
            >
              <span
                className={`inline-block h-4 w-4 rounded-full bg-white transition-transform ${
                  bp.enabled ? "translate-x-6" : "translate-x-1"
                }`}
              />
            </button>
            <span
              className={`flex-1 break-all font-mono text-sm ${
                bp.enabled ? "text-slate-200" : "text-slate-500 line-through"
              }`}
            >
              {describeCondition(bp.condition)}
            </span>
            <span className="text-xs text-slate-400 whitespace-nowrap">
              {bp.hit_count} {bp.hit_count === 1 ? "hit" : "hits"}
            </span>
            <button
              type="button"
              onClick={() => remove(bp.id)}
              aria-label={`Delete breakpoint ${bp.id}`}
              className="text-slate-400 hover:text-red-400 transition-colors px-2"
            >
              ✕
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
