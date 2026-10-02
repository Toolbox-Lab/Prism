"use client";

import { useState } from "react";
import contractTaxonomy from "../../lib/contract-taxonomy.json";

interface TaxonomyEntry {
  code: number;
  name: string;
  summary: string;
  detailed_explanation: string;
  common_causes: Array<{ description: string }>;
  suggested_fixes: Array<{ description: string }>;
}

export default function ContractErrorView() {
  const [selectedCode, setSelectedCode] = useState<number>(0);
  const [customCode, setCustomCode] = useState<string>("");
  const [isCustomMode, setIsCustomMode] = useState<boolean>(false);

  const standardContractErrors = contractTaxonomy as Record<
    string,
    TaxonomyEntry
  >;
  const activeError = isCustomMode
    ? standardContractErrors[customCode]
    : standardContractErrors[String(selectedCode)];

  const handleCustomCodeChange = (val: string) => {
    setCustomCode(val);
    setIsCustomMode(val.trim() !== "");
  };

  return (
    <div className="bg-slate-900 border border-slate-800 rounded-xl p-6 shadow-xl max-w-3xl mx-auto my-6 text-slate-100">
      <div className="flex items-center justify-between mb-6">
        <h2 className="text-xl font-bold tracking-tight text-violet-400">
          📋 Contract Error Lookup
        </h2>
        <span className="text-xs px-2.5 py-1 rounded-full bg-violet-950/50 border border-violet-800/50 text-violet-300 font-mono">
          HostError::Contract
        </span>
      </div>

      <p className="text-sm text-slate-400 mb-6">
        Soroban smart contracts return numeric codes when they revert. Select a
        standard code or enter a custom code to view its details.
      </p>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-4 mb-6">
        <div>
          <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-2">
            Select Standard Code
          </label>
          <select
            value={selectedCode}
            onChange={(e) => {
              setSelectedCode(Number(e.target.value));
              setIsCustomMode(false);
              setCustomCode("");
            }}
            className="w-full bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-slate-200 focus:outline-none focus:border-violet-500 transition-colors"
          >
            {Object.values(standardContractErrors).map((err) => (
              <option key={err.code} value={err.code}>
                Code {err.code}: {err.name}
              </option>
            ))}
          </select>
        </div>

        <div>
          <label className="block text-xs font-semibold text-slate-400 uppercase tracking-wider mb-2">
            Or Type Any Code
          </label>
          <input
            type="number"
            value={customCode}
            onChange={(e) => handleCustomCodeChange(e.target.value)}
            placeholder="e.g. 42"
            className="w-full bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-slate-200 placeholder-slate-600 focus:outline-none focus:border-violet-500 transition-colors"
          />
        </div>
      </div>

      {activeError ? (
        <div className="border-t border-slate-800 pt-6">
          <div className="flex items-start justify-between mb-4">
            <div>
              <span className="text-xs font-bold text-violet-400 uppercase tracking-wider">
                Category: Contract
              </span>
              <h3 className="text-lg font-bold text-slate-100 flex items-center gap-2 mt-1">
                {activeError.name}
                <span className="text-sm font-mono text-slate-500 bg-slate-950 px-2 py-0.5 rounded border border-slate-800">
                  Code {activeError.code}
                </span>
              </h3>
            </div>
            <span className="text-xs px-2.5 py-1 rounded bg-red-950/40 border border-red-900/50 text-red-400 font-medium">
              Error
            </span>
          </div>

          <div className="bg-slate-950 rounded-lg p-4 border border-slate-800 mb-4">
            <h4 className="text-xs font-bold text-slate-500 uppercase tracking-wider mb-1">
              Summary
            </h4>
            <p className="text-slate-300 text-sm">{activeError.summary}</p>
          </div>

          <div className="mb-4">
            <h4 className="text-xs font-bold text-slate-500 uppercase tracking-wider mb-1">
              Detailed Explanation
            </h4>
            <p className="text-slate-400 text-sm leading-relaxed">
              {activeError.detailed_explanation}
            </p>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-4 mt-6">
            <div>
              <h4 className="text-xs font-bold text-slate-500 uppercase tracking-wider mb-2">
                Common Causes
              </h4>
              <ul className="space-y-2">
                {activeError.common_causes.map((cause, i) => (
                  <li
                    key={i}
                    className="text-sm text-slate-300 flex items-start gap-2"
                  >
                    <span className="text-red-400 mt-1">•</span>
                    <span>{cause.description}</span>
                  </li>
                ))}
              </ul>
            </div>

            <div>
              <h4 className="text-xs font-bold text-slate-500 uppercase tracking-wider mb-2">
                Suggested Fixes
              </h4>
              <ul className="space-y-2">
                {activeError.suggested_fixes.map((fix, i) => (
                  <li
                    key={i}
                    className="text-sm text-slate-300 flex items-start gap-2"
                  >
                    <span className="text-emerald-400 mt-1">✓</span>
                    <span>{fix.description}</span>
                  </li>
                ))}
              </ul>
            </div>
          </div>
        </div>
      ) : isCustomMode && customCode.trim() !== "" ? (
        <div className="border-t border-slate-800 pt-6">
          <div className="flex items-start justify-between mb-4">
            <div>
              <span className="text-xs font-bold text-amber-400 uppercase tracking-wider">
                Category: Contract (Custom)
              </span>
              <h3 className="text-lg font-bold text-slate-100 flex items-center gap-2 mt-1">
                CustomContractError
                <span className="text-sm font-mono text-slate-500 bg-slate-950 px-2 py-0.5 rounded border border-slate-800">
                  Code {customCode}
                </span>
              </h3>
            </div>
            <span className="text-xs px-2.5 py-1 rounded bg-red-950/40 border border-red-900/50 text-red-400 font-medium">
              Error
            </span>
          </div>

          <div className="bg-slate-950 rounded-lg p-4 border border-slate-800 mb-4">
            <h4 className="text-xs font-bold text-slate-500 uppercase tracking-wider mb-1">
              Summary
            </h4>
            <p className="text-slate-300 text-sm">
              The contract reverted with custom error code {customCode}.
            </p>
          </div>

          <div className="mb-4">
            <h4 className="text-xs font-bold text-slate-500 uppercase tracking-wider mb-1">
              Detailed Explanation
            </h4>
            <p className="text-slate-400 text-sm leading-relaxed">
              This is a custom contract-defined error code. Custom error codes
              are defined by the contract author inside their{" "}
              {"`#[contracterror]`"} enum. To map this code to its name, you
              need the contract{"'s"} WASM specification or source code.
            </p>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-4 mt-6">
            <div>
              <h4 className="text-xs font-bold text-slate-500 uppercase tracking-wider mb-2">
                Common Causes
              </h4>
              <ul className="space-y-2">
                <li className="text-sm text-slate-300 flex items-start gap-2">
                  <span className="text-red-400 mt-1">•</span>
                  <span>
                    A business logic requirement or assertion in the contract
                    {"'s"} code was not met.
                  </span>
                </li>
              </ul>
            </div>

            <div>
              <h4 className="text-xs font-bold text-slate-500 uppercase tracking-wider mb-2">
                Suggested Fixes
              </h4>
              <ul className="space-y-2">
                <li className="text-sm text-slate-300 flex items-start gap-2">
                  <span className="text-emerald-400 mt-1">✓</span>
                  <span>
                    Locate the source code of the contract and find its{" "}
                    {"`#[contracterror]`"} enum.
                  </span>
                </li>
                <li className="text-sm text-slate-300 flex items-start gap-2">
                  <span className="text-emerald-400 mt-1">✓</span>
                  <span>
                    Match the numeric code {customCode} with the integer values
                    assigned to the enum variants.
                  </span>
                </li>
              </ul>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}
