"use client";

import React, { useState } from "react";
import { Activity, Calendar, Clock, Download, FileText, MessageSquare, Sparkles, Volume2, X } from "lucide-react";
import type { StreamEventRecord } from "../../../../types/log";
import { useAdminSessionDetails } from "../../../lib/api";
import { formatCurrency, formatDate, formatDuration, formatTimeOnly } from "../../../lib/formatters";
import { formatEventPayload } from "../../../lib/admin-formatters";
import { Avatar } from "../../ui/Avatar";
import { ConversationThread } from "./ConversationThread";
import { RecordingAudio, isPlayableUrl } from "./RecordingAudio";

type DetailTab = "summary" | "conversation" | "audio" | "activity";

const TABS: Array<{ id: DetailTab; label: string; icon: typeof FileText }> = [
  { id: "summary", label: "Summary", icon: FileText },
  { id: "conversation", label: "Conversation", icon: MessageSquare },
  { id: "audio", label: "Audio", icon: Volume2 },
  { id: "activity", label: "Activity", icon: Activity },
];

interface SessionDetailsPanelProps {
  sessionId: string;
  onClose: () => void;
}

function Metric({ icon: Icon, label, value, hint }: { icon: typeof Clock; label: string; value: string; hint?: string }) {
  return (
    <div className="stat-tile min-w-0">
      <span className="flex items-center gap-1.5 text-[11px] text-theme-sub"><Icon className="h-3.5 w-3.5 text-cyan-400" />{label}</span>
      <p className="mt-1 truncate font-display text-base font-semibold text-theme-text">{value}</p>
      {hint && <p className="truncate text-[11px] text-theme-sub">{hint}</p>}
    </div>
  );
}

export function SessionDetailsPanel({ sessionId, onClose }: SessionDetailsPanelProps): React.ReactElement {
  const { data, isLoading, isError } = useAdminSessionDetails(sessionId);
  const [tab, setTab] = useState<DetailTab>("summary");
  const summary = data?.summary;
  const audioSource = [data?.endpoints?.audio_url, summary?.audio_presigned_url].find(isPlayableUrl);
  const conversation = data?.conversation ?? [];
  const activity = (data?.raw_events ?? []).filter((event: StreamEventRecord) => event.stream !== "stream:conversation");

  const renderBody = () => {
    if (isLoading) return <div className="space-y-3">{[0, 1, 2].map((i) => <div key={i} className="skeleton h-16 rounded-xl" />)}</div>;
    if (isError) return <p className="py-10 text-center text-sm text-rose-400">This call could not be loaded. Please try again.</p>;
    switch (tab) {
      case "conversation":
        return <ConversationThread events={conversation} callerName={data?.user?.name} callerEmail={data?.user?.email} startedAt={summary?.created_at} />;
      case "audio":
        return audioSource ? (
          <div className="space-y-4 rounded-2xl border border-theme-border bg-theme-card-hover p-5">
            <RecordingAudio callId={sessionId} src={audioSource} />
            <a href={audioSource} download={`${sessionId}.wav`} className="btn-primary w-full sm:w-auto">
              <Download className="h-4 w-4" /> Download recording
            </a>
          </div>
        ) : <p className="py-10 text-center text-sm text-theme-sub">No recording is available for this call.</p>;
      case "activity":
        return activity.length ? (
          <ul className="divide-y divide-[var(--border-subtle)] rounded-2xl border border-theme-border text-xs">
            {activity.map((event: StreamEventRecord, idx: number) => (
              <li key={`${event.timestamp}-${idx}`} className="grid grid-cols-[72px_1fr] gap-3 px-4 py-2.5">
                <span className="tabular-nums text-theme-sub">{formatTimeOnly(event.timestamp)}</span>
                <div className="min-w-0">
                  <span className="mr-2 rounded bg-cyan-500/10 px-1.5 py-0.5 text-[10px] font-semibold text-cyan-400">{event.event_type}</span>
                  <span className="break-words text-theme-text">{formatEventPayload(event.payload_json)}</span>
                </div>
              </li>
            ))}
          </ul>
        ) : <p className="py-10 text-center text-sm text-theme-sub">No additional activity recorded.</p>;
      case "summary":
      default:
        return (
          <div className="space-y-4">
            <div className="grid grid-cols-2 gap-3 xl:grid-cols-4">
              <Metric icon={Clock} label="Duration" value={summary?.duration_seconds != null ? formatDuration(summary.duration_seconds) : "—"} />
              <Metric icon={Calendar} label="Started" value={summary?.created_at ? formatTimeOnly(summary.created_at) : "—"} hint={summary?.created_at ? formatDate(summary.created_at) : undefined} />
              <Metric icon={MessageSquare} label="Messages" value={String(conversation.length)} />
              <Metric icon={Sparkles} label="Budget" value={summary?.qualified_budget_aed ? formatCurrency(summary.qualified_budget_aed) : "—"} hint={summary?.preferred_location || undefined} />
            </div>
            <div className="rounded-2xl border border-theme-border bg-theme-card-hover p-5">
              <p className="text-[11px] font-semibold uppercase tracking-wider text-cyan-400">Call summary</p>
              <p className="mt-2 whitespace-pre-wrap break-words text-sm leading-relaxed text-theme-text">
                {summary?.summary?.trim() || "A summary has not been generated for this call yet."}
              </p>
            </div>
          </div>
        );
    }
  };

  return (
    <section className="flex h-full min-h-0 flex-col overflow-hidden rounded-2xl border border-theme-border bg-theme-card">
      <header className="flex items-start gap-3 border-b border-theme-border p-5">
        <Avatar name={data?.user?.name} email={data?.user?.email} size="lg" />
        <div className="min-w-0 flex-1">
          <h3 className="truncate text-lg font-semibold text-theme-text">{data?.user?.name || "Guest caller"}</h3>
          <p className="truncate text-xs text-theme-sub">
            {[data?.user?.email, data?.user?.phone].filter(Boolean).join(" · ") || "No contact details"}
          </p>
          <p className="mt-1 truncate text-[11px] text-theme-sub">Call ID {sessionId}</p>
        </div>
        <button id="call-details-close" type="button" onClick={onClose} aria-label="Close details" className="page-btn">
          <X className="h-4 w-4" />
        </button>
      </header>

      <div role="tablist" className="flex gap-1 overflow-x-auto border-b border-theme-border px-3">
        {TABS.map(({ id, label, icon: Icon }) => (
          <button
            key={id}
            id={`call-tab-${id}`}
            role="tab"
            type="button"
            aria-selected={tab === id}
            onClick={() => setTab(id)}
            className={`inline-flex shrink-0 items-center gap-1.5 border-b-2 px-3 py-3 text-xs font-semibold transition-colors ${
              tab === id ? "border-cyan-400 text-theme-text" : "border-transparent text-theme-sub hover:text-theme-text"
            }`}
          >
            <Icon className="h-3.5 w-3.5" />{label}
            {id === "conversation" && conversation.length > 0 && <span className="rounded-full bg-theme-card-hover px-1.5 text-[10px]">{conversation.length}</span>}
          </button>
        ))}
      </div>

      <div className="min-h-0 flex-1 overflow-auto p-5">{renderBody()}</div>
    </section>
  );
}
