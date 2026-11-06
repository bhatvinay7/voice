"use client";

import React from "react";
import { Clock, MapPin, Volume2, VolumeX } from "lucide-react";
import type { CallSummaryRecord } from "../../../../types/log";
import { formatCallTiming, formatDuration } from "../../../lib/formatters";
import { Avatar } from "../../ui/Avatar";
import { isPlayableUrl } from "./RecordingAudio";

interface CallLogListProps {
  records: CallSummaryRecord[];
  selectedId: string | null;
  onSelect: (callId: string) => void;
}

function displayName(record: CallSummaryRecord): string {
  return record.user_name?.trim() || record.user_email?.trim() || "Guest caller";
}

export function CallLogList({ records, selectedId, onSelect }: CallLogListProps): React.ReactElement {
  if (!records.length) {
    return (
      <div className="rounded-2xl border border-dashed border-theme-border py-16 text-center text-sm text-theme-sub">
        No calls match your filters.
      </div>
    );
  }

  return (
    <ul className="divide-y divide-[var(--border-subtle)] overflow-hidden rounded-2xl border border-theme-border bg-theme-card">
      {records.map((record) => {
        const isActive = record.call_id === selectedId;
        const hasAudio = isPlayableUrl(record.audio_presigned_url);
        return (
          <li key={record.call_id}>
            <button
              id={`call-row-${record.call_id}`}
              type="button"
              onClick={() => onSelect(record.call_id)}
              aria-current={isActive ? "true" : undefined}
              className={`group flex w-full min-w-0 items-start gap-3 px-4 py-3.5 text-left transition-colors ${
                isActive ? "bg-cyan-500/[0.07] shadow-[inset_3px_0_0_0_var(--accent-cyan)]" : "hover:bg-theme-card-hover"
              }`}
            >
              <Avatar name={record.user_name} email={record.user_email} size="sm" />
              <div className="min-w-0 flex-1">
                <div className="flex items-center justify-between gap-2">
                  <p className="truncate text-sm font-semibold text-theme-text">{displayName(record)}</p>
                  <span className="shrink-0 text-[11px] text-theme-sub">{formatCallTiming(record.created_at)}</span>
                </div>
                <p className="mt-0.5 line-clamp-1 break-words text-xs text-theme-sub">
                  {record.summary?.trim() || "Summary is being prepared"}
                </p>
                <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-theme-sub">
                  <span className="inline-flex items-center gap-1">
                    <Clock className="h-3 w-3" />
                    {record.duration_seconds != null ? formatDuration(record.duration_seconds) : "—"}
                  </span>
                  {record.preferred_location && (
                    <span className="inline-flex min-w-0 items-center gap-1">
                      <MapPin className="h-3 w-3 shrink-0" />
                      <span className="truncate">{record.preferred_location}</span>
                    </span>
                  )}
                  <span className={`inline-flex items-center gap-1 ${hasAudio ? "text-cyan-400" : ""}`}>
                    {hasAudio ? <Volume2 className="h-3 w-3" /> : <VolumeX className="h-3 w-3" />}
                    {hasAudio ? "Recording" : "No audio"}
                  </span>
                </div>
              </div>
            </button>
          </li>
        );
      })}
    </ul>
  );
}
