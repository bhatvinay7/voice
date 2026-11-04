"use client";

import React, { useEffect, useMemo, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { Activity, PhoneCall, Search, X } from "lucide-react";
import { useAdminLogs, useAdminCallSummaries } from "../lib/api";
import { adminDefaultPageSize } from "../lib/admin-config";
import { formatCallTiming } from "../lib/formatters";
import { formatEventPayload } from "../lib/admin-formatters";
import { SessionDetailsPanel } from "./admin/calls/SessionDetailsPanel";

type LogEntry = NonNullable<ReturnType<typeof useAdminLogs>["data"]>["logs"][number];

function prettyJson(raw: string): string {
  try {
    return JSON.stringify(JSON.parse(raw), null, 2);
  } catch {
    return raw;
  }
}

/** 90% viewport modal shared by log + call details. */
function Modal({ open, onClose, children }: { open: boolean; onClose: () => void; children: React.ReactNode }) {
  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => event.key === "Escape" && onClose();
    const previous = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    document.addEventListener("keydown", onKey);
    return () => {
      document.body.style.overflow = previous;
      document.removeEventListener("keydown", onKey);
    };
  }, [open, onClose]);

  return (
    <AnimatePresence>
      {open && (
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          onClick={onClose}
          role="dialog"
          aria-modal="true"
          className="fixed inset-0 z-[60] flex items-center justify-center bg-black/70 backdrop-blur-sm"
        >
          <motion.div
            initial={{ opacity: 0, scale: 0.96, y: 12 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.96, y: 12 }}
            transition={{ duration: 0.18 }}
            onClick={(event) => event.stopPropagation()}
            className="h-[90vh] w-[90vw] max-w-[1600px] overflow-hidden rounded-2xl shadow-2xl"
          >
            {children}
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

function LogDetails({ log, onClose }: { log: LogEntry; onClose: () => void }) {
  const fields: [string, string][] = [
    ["Time", formatCallTiming(log.timestamp)],
    ["Stream", log.stream],
    ["Event", log.event_type],
    ["Session", log.session_id],
    ["Trace", log.trace_id],
  ];
  return (
    <section className="flex h-full min-h-0 flex-col overflow-hidden rounded-2xl border border-theme-border bg-theme-card">
      <header className="flex items-start gap-3 border-b border-theme-border p-5">
        <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-emerald-500/10 text-emerald-400">
          <Activity className="h-5 w-5" />
        </div>
        <div className="min-w-0 flex-1">
          <h3 className="truncate text-lg font-semibold text-theme-text">{log.event_type}</h3>
          <p className="truncate text-xs text-theme-sub">{log.stream} · {formatCallTiming(log.timestamp)}</p>
        </div>
        <button id="log-details-close" type="button" onClick={onClose} aria-label="Close details" className="page-btn">
          <X className="h-4 w-4" />
        </button>
      </header>
      <div className="min-h-0 flex-1 space-y-5 overflow-auto p-5">
        <dl className="grid gap-3 sm:grid-cols-2 lg:grid-cols-5">
          {fields.map(([label, value]) => (
            <div key={label} className="min-w-0 rounded-xl border border-theme-border bg-theme-main p-3">
              <dt className="text-[10px] font-semibold uppercase tracking-wider text-theme-sub">{label}</dt>
              <dd className="mt-1 break-all font-mono text-xs text-theme-text">{value || "—"}</dd>
            </div>
          ))}
        </dl>
        <div>
          <p className="mb-2 text-[11px] font-semibold uppercase tracking-wider text-cyan-400">Payload</p>
          <pre className="overflow-auto whitespace-pre-wrap break-words rounded-xl border border-theme-border bg-theme-main p-4 font-mono text-xs leading-relaxed text-theme-text">
            {prettyJson(log.payload_json)}
          </pre>
        </div>
      </div>
    </section>
  );
}

export function AdminLogsConsole(): React.ReactElement {
  const { data: logsData, isLoading: logsLoading } = useAdminLogs();
  const { data: summariesData, isLoading: summariesLoading } = useAdminCallSummaries({ page: 1, pageSize: adminDefaultPageSize, search: "" });
  const [query, setQuery] = useState("");
  const [selectedLog, setSelectedLog] = useState<LogEntry | null>(null);
  const [selectedCall, setSelectedCall] = useState<string | null>(null);

  const logs = useMemo(() => {
    const all = logsData?.logs ?? [];
    const q = query.trim().toLowerCase();
    if (!q) return all;
    return all.filter((log) =>
      [log.stream, log.event_type, log.session_id, log.trace_id, log.payload_json].some((v) => v?.toLowerCase().includes(q)),
    );
  }, [logsData, query]);

  return (
    <section className="space-y-6">
      <header className="flex flex-col gap-4 sm:flex-row sm:items-end sm:justify-between">
        <div className="min-w-0">
          <span className="eyebrow-pill"><Activity className="h-3 w-3" /> Activity</span>
          <h2 className="mt-3 text-2xl font-semibold text-theme-text">Activity log</h2>
          <p className="text-sm text-theme-sub">Live events from audit, conversation and system streams. Click a row for full details.</p>
        </div>
        <label className="relative block w-full sm:w-80">
          <span className="sr-only">Search events</span>
          <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-theme-sub" />
          <input
            id="log-search"
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search event, stream, session…"
            className="w-full rounded-xl border border-theme-border bg-theme-card py-2.5 pl-9 pr-3 text-sm text-theme-text outline-none placeholder:text-theme-sub focus:border-cyan-400/60 focus:ring-4 focus:ring-cyan-400/10"
          />
        </label>
      </header>

      <div className="overflow-hidden rounded-2xl border border-theme-border bg-theme-card">
        {logsLoading ? (
          <div className="space-y-2 p-3">{[0, 1, 2, 3, 4].map((i) => <div key={i} className="skeleton h-14 rounded-xl" />)}</div>
        ) : logs.length === 0 ? (
          <p className="py-12 text-center text-sm text-theme-sub">No events have been recorded yet.</p>
        ) : (
          <ul className="max-h-[70vh] divide-y divide-theme-border overflow-y-auto">
            {logs.map((log, idx) => (
              <li key={`${log.trace_id}-${idx}`}>
                <button
                  id={`log-row-${idx}`}
                  type="button"
                  onClick={() => setSelectedLog(log)}
                  className="grid w-full grid-cols-1 gap-1 px-4 py-3 text-left transition-colors hover:bg-theme-card-hover md:grid-cols-[170px_140px_200px_1fr] md:items-center md:gap-4"
                >
                  <span className="font-mono text-[11px] text-cyan-400">{formatCallTiming(log.timestamp)}</span>
                  <span className="w-fit rounded-full border border-indigo-500/30 bg-indigo-500/10 px-2.5 py-1 font-mono text-[10px] leading-none text-indigo-400">{log.stream}</span>
                  <span className="truncate font-mono text-xs font-semibold text-emerald-400">{log.event_type}</span>
                  <span className="truncate text-xs text-theme-sub">{formatEventPayload(log.payload_json)}</span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>

      <div className="space-y-4">
        <div>
          <h3 className="text-lg font-semibold text-theme-text">Recent calls</h3>
          <p className="text-xs text-theme-sub">Click a call to open its summary, recording and conversation.</p>
        </div>
        {summariesLoading ? (
          <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">{[0, 1, 2].map((i) => <div key={i} className="skeleton h-28 rounded-xl" />)}</div>
        ) : (
          <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
            {summariesData?.summaries.map((summary) => (
              <button
                key={summary.call_id}
                id={`log-call-${summary.call_id}`}
                type="button"
                onClick={() => setSelectedCall(summary.call_id)}
                className="min-w-0 space-y-2 rounded-xl border border-theme-border bg-theme-card p-4 text-left transition-colors hover:border-cyan-400/40 hover:bg-theme-card-hover"
              >
                <div className="flex items-center gap-2 text-xs">
                  <PhoneCall className="h-3.5 w-3.5 shrink-0 text-cyan-400" />
                  <span className="truncate font-semibold text-theme-text">{summary.call_id}</span>
                </div>
                <p className="line-clamp-2 text-xs text-theme-sub">{summary.summary || "No summary yet."}</p>
                <p className="text-[11px] text-theme-sub">{formatCallTiming(summary.created_at)}</p>
              </button>
            ))}
            {!summariesData?.summaries.length && (
              <p className="col-span-full py-8 text-center text-sm text-theme-sub">No saved call recordings are available yet.</p>
            )}
          </div>
        )}
      </div>

      <Modal open={!!selectedLog} onClose={() => setSelectedLog(null)}>
        {selectedLog && <LogDetails log={selectedLog} onClose={() => setSelectedLog(null)} />}
      </Modal>
      <Modal open={!!selectedCall} onClose={() => setSelectedCall(null)}>
        {selectedCall && <SessionDetailsPanel sessionId={selectedCall} onClose={() => setSelectedCall(null)} />}
      </Modal>
    </section>
  );
}
