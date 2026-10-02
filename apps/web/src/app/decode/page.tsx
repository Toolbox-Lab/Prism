"use client";

import { useState } from "react";
import {
  Header,
  Footer,
  DecodeForm,
  ErrorCard,
  ContractErrorView,
} from "@/components";
import type { DiagnosticReport } from "@/lib/types";

export default function DecodePage() {
  const [activeTab, setActiveTab] = useState<"diagnose" | "lookup">("diagnose");
  const [report, setReport] = useState<DiagnosticReport | null>(null);

  return (
    <div className="min-h-screen bg-slate-950 text-slate-100 flex flex-col selection:bg-violet-500/30 selection:text-violet-200">
      <Header />

      <main className="flex-1 max-w-5xl w-full mx-auto px-4 py-8 md:py-12">
        <div className="text-center mb-10">
          <div className="inline-flex items-center gap-2 px-3 py-1 rounded-full bg-violet-950/40 border border-violet-800/40 text-violet-300 text-xs font-medium mb-4 animate-pulse">
            <span>🔬</span> Soroban Transaction Debugger
          </div>
          <h1 className="text-3xl md:text-5xl font-extrabold tracking-tight bg-gradient-to-r from-slate-100 via-violet-200 to-violet-400 bg-clip-text text-transparent mb-4">
            Decode Transaction Errors
          </h1>
          <p className="text-slate-400 max-w-xl mx-auto text-base md:text-lg">
            Diagnose cryptic Soroban execution failures, map raw error codes,
            and find suggested remedies instantly.
          </p>
        </div>

        {}
        <div className="flex justify-center border-b border-slate-800 mb-8 max-w-md mx-auto">
          <button
            onClick={() => setActiveTab("diagnose")}
            className={`flex-1 pb-3 text-sm font-semibold transition-all focus:outline-none border-b-2 ${
              activeTab === "diagnose"
                ? "text-violet-400 border-violet-500"
                : "text-slate-500 border-transparent hover:text-slate-300"
            }`}
          >
            🔍 Diagnose Error
          </button>
          <button
            onClick={() => setActiveTab("lookup")}
            className={`flex-1 pb-3 text-sm font-semibold transition-all focus:outline-none border-b-2 ${
              activeTab === "lookup"
                ? "text-violet-400 border-violet-500"
                : "text-slate-500 border-transparent hover:text-slate-300"
            }`}
          >
            📋 Contract Error Reference
          </button>
        </div>

        {}
        <div className="transition-all duration-300">
          {activeTab === "diagnose" ? (
            <div className="space-y-6">
              <DecodeForm onDecode={setReport} />
              {report && (
                <div className="animate-in fade-in slide-in-from-bottom-4 duration-500">
                  <ErrorCard report={report} />
                </div>
              )}
            </div>
          ) : (
            <div className="animate-in fade-in slide-in-from-bottom-4 duration-500">
              <ContractErrorView />
            </div>
          )}
        </div>
      </main>

      <Footer />
    </div>
  );
}
