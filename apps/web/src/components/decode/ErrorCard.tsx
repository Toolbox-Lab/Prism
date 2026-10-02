"use client";

import type { DiagnosticReport } from "@/lib/types";

interface ErrorCardProps {
  report: DiagnosticReport;
}

export default function ErrorCard({ report }: ErrorCardProps) {
  const getSeverityStyles = (severity: string) => {
    switch (severity.toLowerCase()) {
      case "fatal":
        return "bg-red-950/40 border-red-900/50 text-red-400";
      case "error":
        return "bg-red-950/40 border-red-900/50 text-red-400";
      case "warning":
        return "bg-amber-950/40 border-amber-900/50 text-amber-400";
      case "info":
        return "bg-blue-950/40 border-blue-900/50 text-blue-400";
      default:
        return "bg-slate-950/40 border-slate-900/50 text-slate-400";
    }
  };

  return (
    <div className="bg-slate-900 border border-slate-800 rounded-xl p-6 shadow-xl max-w-3xl mx-auto my-6 text-slate-100">
      <div className="flex items-start justify-between mb-4">
        <div>
          <span className="text-xs font-bold text-violet-400 uppercase tracking-wider">
            Category: {report.error_category}
          </span>
          <h3 className="text-xl font-bold text-slate-100 flex items-center gap-2 mt-1">
            {report.error_name}
            <span className="text-sm font-mono text-slate-500 bg-slate-950 px-2 py-0.5 rounded border border-slate-800">
              Code {report.error_code}
            </span>
          </h3>
        </div>
        <span
          className={`text-xs px-2.5 py-1 rounded border font-medium ${getSeverityStyles(report.severity)}`}
        >
          {report.severity.toUpperCase()}
        </span>
      </div>

      <div className="bg-slate-950 rounded-lg p-4 border border-slate-800 mb-6">
        <h4 className="text-xs font-bold text-slate-500 uppercase tracking-wider mb-1">
          Summary
        </h4>
        <p className="text-slate-300 text-sm">{report.summary}</p>
      </div>

      <div className="mb-6">
        <h4 className="text-xs font-bold text-slate-500 uppercase tracking-wider mb-1">
          Detailed Explanation
        </h4>
        <p className="text-slate-400 text-sm leading-relaxed whitespace-pre-line">
          {report.detailed_explanation}
        </p>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-6 mt-6 border-t border-slate-800 pt-6">
        <div>
          <h4 className="text-xs font-bold text-slate-500 uppercase tracking-wider mb-3">
            Common Causes
          </h4>
          {report.root_causes && report.root_causes.length > 0 ? (
            <ul className="space-y-3">
              {report.root_causes.map((cause, i) => (
                <li
                  key={i}
                  className="text-sm text-slate-300 flex items-start gap-2"
                >
                  <span className="text-red-400 mt-1">•</span>
                  <div>
                    <p>{cause.description}</p>
                    <span className="text-[10px] text-slate-500 uppercase tracking-wider font-mono">
                      Likelihood: {cause.likelihood}
                    </span>
                  </div>
                </li>
              ))}
            </ul>
          ) : (
            <p className="text-sm text-slate-500 italic">
              No common causes documented.
            </p>
          )}
        </div>

        <div>
          <h4 className="text-xs font-bold text-slate-500 uppercase tracking-wider mb-3">
            Suggested Fixes
          </h4>
          {report.suggested_fixes && report.suggested_fixes.length > 0 ? (
            <ul className="space-y-3">
              {report.suggested_fixes.map((fix, i) => (
                <li
                  key={i}
                  className="text-sm text-slate-300 flex items-start gap-2"
                >
                  <span className="text-emerald-400 mt-1">✓</span>
                  <div>
                    <p>{fix.description}</p>
                    <div className="flex gap-2 mt-1">
                      <span className="text-[10px] text-slate-500 uppercase tracking-wider font-mono">
                        Difficulty: {fix.difficulty}
                      </span>
                      {fix.requires_upgrade && (
                        <span className="text-[10px] text-amber-500 uppercase tracking-wider font-mono">
                          Requires Upgrade
                        </span>
                      )}
                    </div>
                  </div>
                </li>
              ))}
            </ul>
          ) : (
            <p className="text-sm text-slate-500 italic">
              No suggested fixes documented.
            </p>
          )}
        </div>
      </div>
    </div>
  );
}
