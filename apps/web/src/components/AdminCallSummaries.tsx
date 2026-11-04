"use client";

import React, { useEffect, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { PhoneCall, RefreshCw, Search } from "lucide-react";
import { useAdminCallSummaries } from "../lib/api";
import { adminDefaultPageSize, adminPageSizeOptions, adminSearchDebounceMs } from "../lib/admin-config";
import { Pagination } from "./ui/Pagination";
import { CallLogList } from "./admin/calls/CallLogList";
import { SessionDetailsPanel } from "./admin/calls/SessionDetailsPanel";

function useDebouncedValue(value: string, delayMs: number): string {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => setDebounced(value), delayMs);
    return () => clearTimeout(timer);
  }, [value, delayMs]);
  return debounced;
}

export const AdminCallSummaries: React.FC = () => {
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(adminDefaultPageSize);
  const [searchInput, setSearchInput] = useState("");
  const search = useDebouncedValue(searchInput, adminSearchDebounceMs);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const { data, isLoading, isFetching, isError, refetch } = useAdminCallSummaries({ page, pageSize, search });

  useEffect(() => setPage(1), [search, pageSize]);

  useEffect(() => {
    if (!selectedId) return;
    const onKey = (event: KeyboardEvent) => event.key === "Escape" && setSelectedId(null);
    const previous = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    document.addEventListener("keydown", onKey);
    return () => {
      document.body.style.overflow = previous;
      document.removeEventListener("keydown", onKey);
    };
  }, [selectedId]);

  const records = data?.summaries ?? [];
  const meta = data?.pagination ?? { page, page_size: pageSize, total: 0, total_pages: 1 };

  const renderList = () => {
    if (isLoading) return <div className="space-y-2">{[0, 1, 2, 3, 4].map((i) => <div key={i} className="skeleton h-[76px] rounded-xl" />)}</div>;
    if (isError) return <p className="rounded-2xl border border-rose-500/20 bg-rose-500/5 py-10 text-center text-sm text-rose-400">Calls could not be loaded.</p>;
    return <CallLogList records={records} selectedId={selectedId} onSelect={setSelectedId} />;
  };

  return (
    <section className="space-y-5">
      <header className="flex flex-col gap-4 sm:flex-row sm:items-end sm:justify-between">
        <div className="min-w-0">
          <span className="eyebrow-pill"><PhoneCall className="h-3 w-3" /> Calls</span>
          <h2 className="mt-3 text-2xl font-semibold text-theme-text">Call logs</h2>
          <p className="text-sm text-theme-sub">Recordings, summaries and full conversations for every customer call.</p>
        </div>
        <div className="flex w-full items-center gap-2 sm:w-auto">
          <label className="relative block min-w-0 flex-1 sm:w-72">
            <span className="sr-only">Search calls</span>
            <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-theme-sub" />
            <input
              id="call-search"
              type="search"
              value={searchInput}
              onChange={(event) => setSearchInput(event.target.value)}
              placeholder="Search name, email, location…"
              className="w-full rounded-xl border border-theme-border bg-theme-card py-2.5 pl-9 pr-3 text-sm text-theme-text outline-none placeholder:text-theme-sub focus:border-cyan-400/60 focus:ring-4 focus:ring-cyan-400/10"
            />
          </label>
          <button id="call-refresh" type="button" onClick={() => refetch()} aria-label="Refresh" className="page-btn h-10 w-10">
            <RefreshCw className={`h-4 w-4 ${isFetching ? "animate-spin" : ""}`} />
          </button>
        </div>
      </header>

      <div className="min-w-0 space-y-4">
        {renderList()}
        {!isLoading && !isError && (
          <Pagination
            meta={meta}
            onPageChange={setPage}
            pageSizeOptions={adminPageSizeOptions}
            onPageSizeChange={setPageSize}
          />
        )}
      </div>

      <AnimatePresence>
        {selectedId && (
          <motion.div
            key="call-modal"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            onClick={() => setSelectedId(null)}
            className="fixed inset-0 z-[60] flex items-center justify-center bg-black/70 backdrop-blur-sm"
            role="dialog"
            aria-modal="true"
          >
            <motion.div
              initial={{ opacity: 0, scale: 0.96, y: 12 }}
              animate={{ opacity: 1, scale: 1, y: 0 }}
              exit={{ opacity: 0, scale: 0.96, y: 12 }}
              transition={{ duration: 0.18 }}
              onClick={(event) => event.stopPropagation()}
              className="h-[90vh] w-[90vw] max-w-[1600px] overflow-hidden rounded-2xl shadow-2xl"
            >
              <SessionDetailsPanel sessionId={selectedId} onClose={() => setSelectedId(null)} />
            </motion.div>
          </motion.div>
        )}
      </AnimatePresence>
    </section>
  );
};
