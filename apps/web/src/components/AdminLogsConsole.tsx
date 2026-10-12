"use client";

import React from "react";
import { useAdminLogs, useAdminCallSummaries } from "../lib/api";
import { formatCallTiming } from "../lib/formatters";
import { formatEventPayload } from "../lib/admin-formatters";

export function AdminLogsConsole(): React.ReactElement {
  const { data: logsData, isLoading: logsLoading } = useAdminLogs();
  const { data: summariesData, isLoading: summariesLoading } = useAdminCallSummaries();

  return (
    <div className="space-y-6">
      <div className="bg-theme-card border border-theme-border rounded-2xl p-6 space-y-4 transition-colors duration-300">
        <div className="flex justify-between items-center">
          <div>
            <h3 className="text-xl font-bold text-theme-text font-serif">Upstash Redis Streams Logs Console</h3>
            <p className="text-xs text-theme-sub">
              Live event streams (`stream:audit`, `stream:conversation`, `stream:logs`) mapped by sessionId & traceId
            </p>
          </div>
        </div>

        {logsLoading ? (
          <p className="text-xs text-theme-sub">Loading diagnostic logs...</p>
        ) : (
          <div className="bg-theme-main p-4 rounded-xl font-mono text-xs max-h-72 overflow-y-auto space-y-2 border border-theme-border">
            {logsData?.logs.map((log, idx) => (
              <div key={idx} className="p-2 rounded bg-theme-card border border-theme-border text-theme-text">
                <span className="text-cyan-400 font-bold">[{formatCallTiming(log.timestamp)}]</span>{" "}
                <span className="text-indigo-400">[{log.stream}]</span>{" "}
                <span className="text-emerald-400">[{log.event_type}]</span>{" "}
                <span className="text-theme-sub">session={log.session_id} trace={log.trace_id}</span>
                <p className="text-theme-text mt-1 break-words">{formatEventPayload(log.payload_json)}</p>
              </div>
            ))}
            {!logsData?.logs.length && <p className="text-theme-sub">No events have been recorded yet.</p>}
          </div>
        )}
      </div>

      <div className="bg-theme-card border border-theme-border rounded-2xl p-6 space-y-4 transition-colors duration-300">
        <div>
          <h3 className="text-xl font-bold text-theme-text font-serif">Call Transcripts & AI Summaries</h3>
          <p className="text-xs text-theme-sub">
            Recorded conversations and saved session details
          </p>
        </div>

        {summariesLoading ? (
          <p className="text-xs text-theme-sub">Loading call summaries...</p>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            {summariesData?.summaries.map((summary) => (
              <div key={summary.call_id} className="bg-theme-main p-4 rounded-xl border border-theme-border space-y-2">
                <div className="flex justify-between items-center text-xs">
                  <span className="font-bold text-theme-text">Call ID: {summary.call_id}</span>
                </div>
                {summary.summary && <p className="text-xs text-theme-sub">{summary.summary}</p>}
                <p className="text-xs text-theme-sub">{formatCallTiming(summary.created_at)}</p>
                {summary.audio_presigned_url && (
                  <audio controls preload="none" src={summary.audio_presigned_url} className="w-full h-8" />
                )}
                <div className="flex gap-4 text-xs">
                  {summary.text_presigned_url && <a className="text-cyan-400 hover:text-cyan-300" href={summary.text_presigned_url}>Transcript</a>}
                  {!summary.audio_presigned_url && <span className="text-theme-sub">Recording unavailable</span>}
                </div>
              </div>
            ))}
            {!summariesData?.summaries.length && (
              <p className="col-span-full py-8 text-center text-sm text-theme-sub">
                No saved call recordings are available yet.
              </p>
            )}
          </div>
        )}
      </div>
    </div>
  );
};
