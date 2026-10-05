"use client";

import React, { JSX } from "react";
import { useAdminLogs, useAdminCallSummaries } from "../lib/api";

export function AdminLogsConsole(): React.ReactElement {
  const { data: logsData, isLoading: logsLoading } = useAdminLogs();
  const { data: summariesData, isLoading: summariesLoading } = useAdminCallSummaries();

  return (
    <div className="space-y-6">
      <div className="bg-slate-900/90 border border-slate-800 rounded-2xl p-6 shadow-2xl space-y-4">
        <div className="flex justify-between items-center">
          <div>
            <h3 className="text-xl font-bold text-white">Upstash Redis Streams Logs Console</h3>
            <p className="text-xs text-slate-400">
              Live event streams (`stream:audit`, `stream:conversation`, `stream:logs`) mapped by sessionId & traceId
            </p>
          </div>
          <span className="text-xs bg-emerald-950 text-emerald-400 px-3 py-1 rounded-full border border-emerald-800 font-mono">
            Consumer Group: postgres-db-sync-group (XREADGROUP / XACK)
          </span>
        </div>

        {logsLoading ? (
          <p className="text-xs text-slate-400">Loading diagnostic logs...</p>
        ) : (
          <div className="bg-slate-950 p-4 rounded-xl font-mono text-xs max-h-72 overflow-y-auto space-y-2 border border-slate-800">
            {logsData?.logs.map((log, idx) => (
              <div key={idx} className="p-2 rounded bg-slate-900/80 border border-slate-800 text-slate-300">
                <span className="text-cyan-400 font-bold">[{log.timestamp}]</span>{" "}
                <span className="text-purple-400">[{log.stream}]</span>{" "}
                <span className="text-emerald-400">[{log.event_type}]</span>{" "}
                <span className="text-slate-400">session={log.session_id} trace={log.trace_id}</span>
                <p className="text-slate-200 mt-1 break-all">{log.payload_json}</p>
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="bg-slate-900/90 border border-slate-800 rounded-2xl p-6 shadow-2xl space-y-4">
        <div>
          <h3 className="text-xl font-bold text-white">Call Transcripts & AI Summaries</h3>
          <p className="text-xs text-slate-400">
            Cloudflare R2 transcript dumps & PostgreSQL structured summaries
          </p>
        </div>

        {summariesLoading ? (
          <p className="text-xs text-slate-400">Loading call summaries...</p>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            {summariesData?.summaries.map((summary) => (
              <div key={summary.call_id} className="bg-slate-950 p-4 rounded-xl border border-slate-800 space-y-2">
                <div className="flex justify-between items-center text-xs">
                  <span className="font-bold text-white">Call ID: {summary.call_id}</span>
                  <span className="text-emerald-400 font-semibold">${summary.qualified_budget.toLocaleString()} Budget</span>
                </div>
                <p className="text-xs text-slate-300">{summary.summary}</p>
                <div className="text-[10px] text-slate-500 font-mono space-y-1">
                  <p>R2 Transcript: {summary.r2_transcript_path}</p>
                  <p>R2 Audio: {summary.r2_audio_path}</p>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
};
