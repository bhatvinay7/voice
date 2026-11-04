"use client";

import React, { useState } from "react";
import { Bot, Check, Copy } from "lucide-react";
import type { StreamEventRecord } from "../../../../types/log";
import { getConversationText } from "../../../lib/admin-formatters";
import { formatTimeOnly } from "../../../lib/formatters";
import { Avatar } from "../../ui/Avatar";

interface ConversationThreadProps {
  events: StreamEventRecord[];
  callerName?: string;
  callerEmail?: string;
  startedAt?: string;
}

function offsetLabel(startedAt: string | undefined, timestamp: string, index: number): string {
  const start = startedAt ? new Date(startedAt).getTime() : Number.NaN;
  const at = new Date(timestamp).getTime();
  if (Number.isNaN(start) || Number.isNaN(at) || at < start) return `Turn ${index + 1}`;
  const seconds = Math.round((at - start) / 1000);
  return `${String(Math.floor(seconds / 60)).padStart(2, "0")}:${String(seconds % 60).padStart(2, "0")}`;
}

export function ConversationThread({ events, callerName, callerEmail, startedAt }: ConversationThreadProps): React.ReactElement {
  const [copiedIdx, setCopiedIdx] = useState<number | null>(null);

  const handleCopy = (text: string, idx: number) => {
    navigator.clipboard.writeText(text).then(() => {
      setCopiedIdx(idx);
      setTimeout(() => setCopiedIdx(null), 1500);
    }).catch(() => setCopiedIdx(null));
  };

  if (!events.length) {
    return <p className="py-10 text-center text-sm text-theme-sub">No conversation was captured for this call.</p>;
  }

  return (
    <ol className="space-y-4">
      {events.map((event, idx) => {
        const text = getConversationText(event.payload_json);
        const isUser = event.event_type === "USER_PROMPT";
        return (
          <li key={`${event.timestamp}-${idx}`} className={`flex gap-3 ${isUser ? "flex-row-reverse" : ""}`}>
            {isUser ? (
              <Avatar name={callerName} email={callerEmail} size="sm" />
            ) : (
              <span className="edge-highlight inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-cyan-500/10 text-cyan-400">
                <Bot className="h-4 w-4" />
              </span>
            )}
            <div
              className={`group relative min-w-0 max-w-[85%] rounded-2xl border px-4 py-3 ${
                isUser
                  ? "rounded-tr-sm border-cyan-500/25 bg-cyan-500/[0.06]"
                  : "rounded-tl-sm border-theme-border bg-theme-card-hover"
              }`}
            >
              <div className={`flex items-center gap-2 text-[11px] text-theme-sub ${isUser ? "justify-end" : ""}`}>
                <span className="font-semibold text-theme-text">{isUser ? callerName || "Caller" : "Assistant"}</span>
                <span className="tabular-nums text-cyan-400">{offsetLabel(startedAt, event.timestamp, idx)}</span>
                <span className="tabular-nums">{formatTimeOnly(event.timestamp)}</span>
              </div>
              <p className="mt-1 whitespace-pre-wrap break-words text-sm leading-relaxed text-theme-text">{text}</p>
              <button
                type="button"
                onClick={() => handleCopy(text, idx)}
                title="Copy message"
                className="absolute right-2 top-2 rounded-md bg-theme-main/80 p-1 text-theme-sub opacity-0 transition-opacity hover:text-theme-text group-hover:opacity-100"
              >
                {copiedIdx === idx ? <Check className="h-3 w-3 text-emerald-400" /> : <Copy className="h-3 w-3" />}
              </button>
            </div>
          </li>
        );
      })}
    </ol>
  );
}
