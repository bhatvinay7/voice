"use client";

import React, { useState } from "react";
import { useAdminCallSummaries, useAdminSessionDetails } from "../lib/api";
import {
  formatCurrency,
  getCallBudget,
  formatDate,
  formatDuration,
  formatCallTiming,
  formatTimeOnly,
} from "../lib/formatters";
import { StreamEventRecord, CallSummaryRecord } from "../../types/log";
import { formatEventPayload, getConversationText } from "../lib/admin-formatters";
import {
  Volume2,
  Clock,
  Calendar,
  Download,
  Copy,
  Check,
  User,
  Bot,
  Activity,
  PhoneCall,
  Sparkles,
} from "lucide-react";

interface SessionDetailsModalProps {
  sessionId: string;
  onClose: () => void;
}

const SessionDetailsModal: React.FC<SessionDetailsModalProps> = ({ sessionId, onClose }) => {
  const { data, isLoading } = useAdminSessionDetails(sessionId);
  const [copiedIdx, setCopiedIdx] = useState<number | null>(null);

  const handleCopy = (text: string, idx: number) => {
    navigator.clipboard.writeText(text);
    setCopiedIdx(idx);
    setTimeout(() => setCopiedIdx(null), 1500);
  };

  const summary = data?.summary;
  const audioSource = [data?.endpoints?.audio_url, summary?.audio_presigned_url]
    .find((url) => typeof url === "string" && url.startsWith("https://"));

  const startTime = summary?.created_at ? new Date(summary.created_at).getTime() : null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4 backdrop-blur-md">
      <div className="bg-theme-card border border-theme-border w-full max-w-4xl max-h-[92vh] rounded-2xl overflow-hidden shadow-2xl flex flex-col">
        {/* Modal Header */}
        <div className="flex justify-between items-center p-5 border-b border-theme-border bg-theme-main">
          <div className="flex items-center space-x-3">
            <div className="w-10 h-10 rounded-xl bg-cyan-500/10 border border-cyan-500/30 flex items-center justify-center text-cyan-400">
              <PhoneCall className="w-5 h-5" />
            </div>
            <div>
              <div className="flex items-center space-x-2">
                <h3 className="text-base font-bold text-cyan-400 font-serif">Call Session: {sessionId}</h3>
                {summary && (
                  <span className="text-[10px] font-mono bg-cyan-500/20 text-cyan-300 px-2 py-0.5 rounded-full border border-cyan-500/40">
                    {formatDuration(summary.duration_seconds)}
                  </span>
                )}
              </div>
              <p className="text-xs text-theme-sub mt-0.5">
                {data?.user
                  ? `Caller: ${data.user.name} (${data.user.phone}) • ${data.user.email}`
                  : `User ID: ${summary?.user_id || "Unregistered"}`}
              </p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="w-8 h-8 rounded-xl flex items-center justify-center bg-white dark:bg-black text-black dark:text-white border border-slate-300 dark:border-white/20 hover:bg-zinc-100 dark:hover:bg-zinc-900 transition-colors font-bold text-sm"
          >
            ✕
          </button>
        </div>

        {/* Modal Content */}
        <div className="flex-1 overflow-y-auto p-6 space-y-6 bg-theme-main">
          {isLoading ? (
            <div className="flex items-center justify-center py-12 text-theme-sub space-x-2">
              <div className="w-4 h-4 border-2 border-cyan-500 border-t-transparent rounded-full animate-spin" />
              <span className="text-sm">Loading session recordings & conversation...</span>
            </div>
          ) : (
            <>
              {/* Timing & Metrics Bar */}
              <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 bg-theme-card p-4 rounded-xl border border-theme-border text-xs">
                <div>
                  <span className="text-theme-sub text-[10px] block flex items-center gap-1">
                    <Clock className="w-3 h-3 text-cyan-400" /> Call Duration
                  </span>
                  <span className="font-bold text-cyan-400 text-sm">
                    {summary?.duration_seconds != null ? formatDuration(summary.duration_seconds) : "—"}
                  </span>
                  <span className="text-[10px] text-theme-sub block">
                    {summary?.duration_seconds != null ? `(${summary.duration_seconds}s total)` : "Duration not recorded"}
                  </span>
                </div>
                <div>
                  <span className="text-theme-sub text-[10px] block flex items-center gap-1">
                    <Calendar className="w-3 h-3 text-emerald-400" /> Start Timing
                  </span>
                  <span className="font-bold text-theme-text text-sm">
                    {summary?.created_at ? formatTimeOnly(summary.created_at) : "—"}
                  </span>
                  <span className="text-[10px] text-theme-sub block truncate">
                    {summary?.created_at ? formatDate(summary.created_at) : "—"}
                  </span>
                </div>
                <div>
                  <span className="text-theme-sub text-[10px] block flex items-center gap-1">
                    <Activity className="w-3 h-3 text-purple-400" /> Conversation Turns
                  </span>
                  <span className="font-bold text-purple-400 text-sm">
                    {data?.conversation?.length ?? 0} Turns
                  </span>
                  <span className="text-[10px] text-theme-sub block">Captured conversation events</span>
                </div>
                <div>
                  <span className="text-theme-sub text-[10px] block flex items-center gap-1">
                    <Sparkles className="w-3 h-3 text-amber-400" /> Qualified Budget
                  </span>
                  <span className="font-bold text-emerald-400 text-sm">
                    {summary?.qualified_budget_aed
                      ? formatCurrency(summary.qualified_budget_aed)
                      : "—"}
                  </span>
                  <span className="text-[10px] text-theme-sub block truncate">
                    {summary?.preferred_location || "Location not recorded"}
                  </span>
                </div>
              </div>

              {/* Audio Player Card */}
              <div className="bg-gradient-to-r from-cyan-950/20 via-theme-card to-emerald-950/20 p-4 rounded-xl border border-cyan-500/30 space-y-3">
                <div className="flex justify-between items-center">
                  <div className="flex items-center space-x-2">
                    <Volume2 className="w-4 h-4 text-cyan-400 animate-pulse" />
                    <span className="text-xs font-bold text-theme-text font-serif">
                      Call Audio Recording (Cloudflare R2 Vault)
                    </span>
                  </div>
                  {audioSource && (
                    <a
                      href={audioSource}
                      download={`${sessionId}.wav`}
                      className="flex items-center space-x-1 text-[11px] font-semibold text-cyan-400 hover:text-cyan-300 transition-colors"
                    >
                      <Download className="w-3.5 h-3.5" />
                      <span>Download .WAV</span>
                    </a>
                  )}
                </div>

                {audioSource ? (
                  <audio controls preload="none" src={audioSource} className="w-full h-10 rounded-lg" />
                ) : (
                  <p className="text-xs text-theme-sub">No recording is available for this session.</p>
                )}
              </div>

              {/* AI Conversation Summary */}
              {summary?.summary && (
                <div className="bg-theme-card p-4 rounded-xl border border-theme-border space-y-1.5 text-xs">
                  <span className="text-[10px] font-bold text-cyan-400 uppercase tracking-wider block">
                    AI Conversation Summary & Intent
                  </span>
                  <p className="text-theme-text leading-relaxed italic">{summary.summary}</p>
                </div>
              )}

              {/* Conversation Timeline */}
              <div className="space-y-4">
                <div className="flex justify-between items-center border-b border-theme-border pb-2">
                  <h4 className="text-xs font-bold text-emerald-400 uppercase tracking-wider">
                    Full Conversation Timeline
                  </h4>
                  <span className="text-[10px] text-theme-sub font-mono">
                    {data?.conversation?.length ?? 0} recorded exchanges
                  </span>
                </div>

                <div className="space-y-3">
                  {data?.conversation?.map((ev: StreamEventRecord, idx: number) => {
                    const textContent = getConversationText(ev.payload_json);
                    const isUser = ev.event_type === "USER_PROMPT";

                    // Compute relative turn offset
                    let offsetStr = `Turn ${idx + 1}`;
                    if (startTime && ev.timestamp) {
                      const evTime = new Date(ev.timestamp).getTime();
                      if (!Number.isNaN(evTime) && evTime >= startTime) {
                        const diffSec = Math.round((evTime - startTime) / 1000);
                        const mins = Math.floor(diffSec / 60);
                        const secs = diffSec % 60;
                        offsetStr = `+${mins.toString().padStart(2, "0")}:${secs.toString().padStart(2, "0")}`;
                      }
                    }

                    return (
                      <div
                        key={idx}
                        className={`flex gap-3 max-w-[85%] ${isUser ? "ml-auto flex-row-reverse" : "mr-auto"}`}
                      >
                        <div
                          className={`w-7 h-7 rounded-lg flex items-center justify-center shrink-0 ${
                            isUser
                              ? "bg-cyan-500/20 text-cyan-400 border border-cyan-500/40"
                              : "bg-emerald-500/20 text-emerald-400 border border-emerald-500/40"
                          }`}
                        >
                          {isUser ? <User className="w-3.5 h-3.5" /> : <Bot className="w-3.5 h-3.5" />}
                        </div>

                        <div
                          className={`p-3.5 rounded-2xl border text-xs space-y-1 relative group ${
                            isUser
                              ? "bg-cyan-950/20 border-cyan-500/30 text-right rounded-tr-none"
                              : "bg-theme-card border-theme-border text-left rounded-tl-none"
                          }`}
                        >
                          <div className={`flex items-center gap-2 text-[10px] text-theme-sub ${isUser ? "justify-end" : "justify-start"}`}>
                            <span className="font-semibold text-theme-text">
                              {isUser ? (data?.user?.name || "Caller") : "AI Real Estate Agent"}
                            </span>
                            <span>•</span>
                            <span className="font-mono text-cyan-400">{offsetStr}</span>
                            <span>•</span>
                            <span>{formatTimeOnly(ev.timestamp)}</span>
                          </div>

                          <p className="text-theme-text text-xs leading-relaxed">{textContent}</p>

                          <button
                            onClick={() => handleCopy(textContent, idx)}
                            className="absolute top-2 right-2 opacity-0 group-hover:opacity-100 transition-opacity p-1 rounded bg-black/40 text-theme-sub hover:text-white"
                            title="Copy message text"
                          >
                            {copiedIdx === idx ? (
                              <Check className="w-3 h-3 text-emerald-400" />
                            ) : (
                              <Copy className="w-3 h-3" />
                            )}
                          </button>
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>

              {/* Logs & Diagnostics */}
              {data?.raw_events && data.raw_events.some((e: StreamEventRecord) => e.stream !== "stream:conversation") && (
                <div className="space-y-3 mt-6">
                  <h4 className="text-xs font-bold text-cyan-400 uppercase tracking-wider border-b border-theme-border pb-2">
                    Audit & Stream Logs
                  </h4>
                  <div className="space-y-2 bg-theme-card rounded-xl p-3 border border-theme-border font-mono text-[10px]">
                    {data.raw_events
                      .filter((e: StreamEventRecord) => e.stream !== "stream:conversation")
                      .map((ev: StreamEventRecord, idx: number) => (
                        <div
                          key={idx}
                          className="grid grid-cols-[140px_1fr] gap-3 border-b border-theme-border/50 pb-2 mb-2 last:border-0 last:pb-0 last:mb-0"
                        >
                          <span className="text-theme-sub">{formatTimeOnly(ev.timestamp)}</span>
                          <div>
                            <span
                              className={`px-1.5 py-0.5 rounded mr-2 font-bold ${
                                ev.stream === "stream:audit"
                                  ? "bg-cyan-500/20 text-cyan-400"
                                  : "bg-rose-500/20 text-rose-400"
                              }`}
                            >
                              {ev.event_type}
                            </span>
                            <span className="text-theme-text truncate inline-block align-bottom">
                              {formatEventPayload(ev.payload_json)}
                            </span>
                          </div>
                        </div>
                      ))}
                  </div>
                </div>
              )}
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
      <div className="bg-theme-card border border-theme-border rounded-2xl p-6 space-y-6 transition-colors duration-300">
        <div>
          <h3 className="text-xl font-bold text-theme-text font-serif">
            Audio Records & Conversation Summaries
          </h3>
          <p className="text-xs text-theme-sub">
            Review audio recordings, structured call transcripts, qualified buyer budgets, and exact timing diagnostics. Click any session to open the full conversation timeline and audio player.
          </p>
        </div>

        {isLoading ? (
          <div className="py-8 text-center text-xs text-theme-sub">
            Loading call records...
          </div>
        ) : (
          <div className="space-y-4">
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            {data?.summaries.map((record: CallSummaryRecord) => (
                <div
                  key={record.call_id}
                  onClick={() => setSelectedSessionId(record.call_id)}
                  className="cursor-pointer bg-theme-main p-5 rounded-xl border border-theme-border space-y-3 hover:border-cyan-500/50 hover:bg-theme-card transition-all group"
                >
                  <div className="flex justify-between items-start">
                    <div>
                      <h4 className="text-sm font-bold text-cyan-400 group-hover:text-cyan-300 transition-colors">
                        {record.call_id}
                      </h4>
                      <p className="text-xs text-theme-sub flex items-center gap-1 mt-0.5">
                        <Calendar className="w-3 h-3 text-cyan-400" />
                        {formatCallTiming(record.created_at)}
                      </p>
                    </div>
                  </div>

                  {/* Audio Preview Widget */}
                  <div
                    onClick={(e) => e.stopPropagation()}
                    className="bg-black/20 p-2 rounded-lg border border-theme-border/60 flex items-center gap-2"
                  >
                    <Volume2 className="w-3.5 h-3.5 text-cyan-400 shrink-0" />
                    {record.audio_presigned_url?.startsWith("https://") ? (
                      <audio controls preload="none" src={record.audio_presigned_url} className="w-full h-7" />
                    ) : (
                      <span className="text-[11px] text-theme-sub">Recording unavailable</span>
                    )}
                  </div>

                </div>
              ))}
              {!data?.summaries.length && (
                <p className="col-span-full py-8 text-center text-sm text-theme-sub">
                  No saved call recordings are available yet.
                </p>
              )}
            </div>
          </div>
        )}
      </div>

      {selectedSessionId && (
        <SessionDetailsModal
          sessionId={selectedSessionId}
          onClose={() => setSelectedSessionId(null)}
        />
      )}
    </>
  );
};
