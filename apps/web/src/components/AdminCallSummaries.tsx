"use client";

import React, { useState } from "react";
import { useAdminCallSummaries, useAdminSessionDetails } from "../lib/api";

const SessionDetailsModal: React.FC<{ sessionId: string; onClose: () => void }> = ({ sessionId, onClose }) => {
  const { data, isLoading } = useAdminSessionDetails(sessionId);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4 backdrop-blur-sm">
      <div className="bg-slate-900 border border-slate-700 w-full max-w-4xl max-h-[90vh] rounded-2xl overflow-hidden shadow-2xl flex flex-col">
        <div className="flex justify-between items-center p-5 border-b border-slate-800 bg-slate-900/50">
          <div>
            <h3 className="text-lg font-bold text-cyan-400">Session View: {sessionId}</h3>
            {data?.user && (
              <p className="text-xs text-slate-400">User: {data.user.name} ({data.user.phone}) - {data.user.email}</p>
            )}
          </div>
          <button onClick={onClose} className="text-slate-400 hover:text-white">✕</button>
        </div>

        <div className="flex-1 overflow-y-auto p-6 space-y-6 bg-slate-950">
          {isLoading ? (
            <p className="text-slate-400 text-sm">Loading detailed session data...</p>
          ) : (
            <>
              {/* Conversation Timeline */}
              <div className="space-y-3">
                <h4 className="text-sm font-bold text-emerald-400 border-b border-slate-800 pb-2">Conversation Timeline</h4>
                <div className="space-y-2">
                  {data?.conversation?.map((ev: any, idx: number) => {
                    const payload = JSON.parse(ev.payload_json);
                    const isUser = ev.event_type === "USER_PROMPT";
                    return (
                      <div key={idx} className={`p-3 rounded-lg border text-xs max-w-[80%] ${isUser ? 'bg-indigo-900/30 border-indigo-800/50 ml-auto text-right' : 'bg-slate-800/40 border-slate-700 mr-auto text-left'}`}>
                        <p className="text-[10px] text-slate-400 mb-1">{isUser ? 'User' : 'AI Agent'} • {new Date(ev.timestamp).toLocaleTimeString()}</p>
                        <p className="text-slate-200">{payload.text || payload.response_text}</p>
                      </div>
                    );
                  })}
                </div>
              </div>

              {/* Logs & Diagnostics */}
              <div className="space-y-3 mt-6">
                <h4 className="text-sm font-bold text-orange-400 border-b border-slate-800 pb-2">Logs & Operations</h4>
                <div className="space-y-2 bg-slate-900 rounded-lg p-3 border border-slate-800 font-mono text-[10px]">
                  {data?.raw_events?.filter((e: any) => e.stream !== 'stream:conversation').map((ev: any, idx: number) => (
                    <div key={idx} className="grid grid-cols-[100px_1fr] gap-3 border-b border-slate-800/50 pb-2 mb-2 last:border-0 last:pb-0 last:mb-0">
                      <span className="text-slate-500">{new Date(ev.timestamp).toLocaleTimeString()}</span>
                      <div>
                        <span className={`px-1.5 py-0.5 rounded mr-2 ${ev.stream === 'stream:audit' ? 'bg-purple-900/50 text-purple-300' : 'bg-red-900/50 text-red-300'}`}>
                          {ev.event_type}
                        </span>
                        <span className="text-slate-300 truncate inline-block align-bottom">{ev.payload_json}</span>
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
};

export const AdminCallSummaries: React.FC = () => {
  const { data, isLoading } = useAdminCallSummaries();
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);

  return (
    <>
      <div className="bg-slate-900/90 border border-slate-800 rounded-2xl p-6 shadow-2xl space-y-6">
        <div>
          <h3 className="text-xl font-bold text-white">Audio Records & Conversation Summaries</h3>
          <p className="text-xs text-slate-400">
            Cloudflare R2 vault transcript dumps, user phone details, qualified budget insights, and async summary worker records. Click a session for a structured deep dive.
          </p>
        </div>

        {isLoading ? (
          <p className="text-xs text-slate-400">Loading call summaries...</p>
        ) : (
          <div className="space-y-4">
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              {data?.summaries.map((record: any) => (
                <div
                  key={record.call_id}
                  onClick={() => setSelectedSessionId(record.call_id)}
                  className="cursor-pointer bg-slate-950 p-5 rounded-xl border border-slate-800 space-y-3 shadow-lg hover:border-cyan-800/80 hover:bg-slate-900/50 transition-all"
                >
                  <div className="flex justify-between items-start">
                    <div>
                      <h4 className="text-sm font-bold text-cyan-400">{record.call_id}</h4>
                      <p className="text-xs text-slate-300 font-medium">User ID: {record.user_id}</p>
                    </div>
                    <span className="text-[10px] font-mono bg-cyan-950 text-cyan-300 px-2.5 py-1 rounded-full border border-cyan-800/60">
                      Duration: {record.duration_seconds}s
                    </span>
                  </div>

                  <div className="bg-slate-900 p-3 rounded-lg border border-slate-800 text-xs text-slate-300 space-y-1">
                    <p className="font-semibold text-slate-200">Summary:</p>
                    <p className="italic text-slate-400 line-clamp-2">{record.summary}</p>
                  </div>

                  <div className="grid grid-cols-2 gap-2 text-[11px]">
                    <div className="bg-slate-900/80 p-2 rounded-lg border border-slate-800/80">
                      <span className="text-slate-400 block text-[10px]">Qualified Budget</span>
                      <span className="font-bold text-emerald-400">${record.qualified_budget_aed?.toLocaleString() || record.qualified_budget?.toLocaleString()}</span>
                    </div>
                    <div className="bg-slate-900/80 p-2 rounded-lg border border-slate-800/80">
                      <span className="text-slate-400 block text-[10px]">Location Preference</span>
                      <span className="font-bold text-blue-400 truncate block">{record.preferred_location}</span>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>

      {selectedSessionId && (
        <SessionDetailsModal sessionId={selectedSessionId} onClose={() => setSelectedSessionId(null)} />
      )}
    </>
  );
};
