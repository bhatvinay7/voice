"use client";

import React, { useState } from "react";
import Link from "next/link";
import { motion, AnimatePresence } from "framer-motion";
import {
  ShieldAlert,
  Database,
  Workflow,
  FileCheck,
  Headphones,
  Terminal,
  Layers,
  ArrowLeft,
  Activity,
  Sliders,
  Menu,
  X,
  Server,
  Zap,
} from "lucide-react";
import { AdminTab, AdminNavItem } from "../../../types/admin";
import { UserProfile } from "../../../types/user";

interface AdminSidebarProps {
  activeTab: AdminTab;
  onSelectTab: (tab: AdminTab) => void;
  user: UserProfile | null;
}

export const ADMIN_NAV_ITEMS: AdminNavItem[] = [
  {
    id: "all",
    label: "Overview & All Modules",
    shortLabel: "All Services",
    icon: Layers,
    badge: "Unified",
    badgeColor: "bg-white/10 text-white border-white/20",
    description: "Complete control center overview",
  },
  {
    id: "ingestion",
    label: "Data Ingestion & Vectors",
    shortLabel: "Ingestion",
    icon: Database,
    badge: "Postgres",
    badgeColor: "bg-cyan-500/20 text-cyan-400 border-cyan-500/30",
    description: "Vector embeddings & listing crawl",
  },
  {
    id: "workflows",
    label: "Workflow DAG Editor",
    shortLabel: "DAG Workflows",
    icon: Workflow,
    badge: "Memgraph",
    badgeColor: "bg-emerald-500/20 text-emerald-400 border-emerald-500/30",
    description: "Graph state machine pipeline",
  },
  {
    id: "contracts",
    label: "Tenancy Contract Review",
    shortLabel: "Contracts",
    icon: FileCheck,
    badge: "Ejari Queue",
    badgeColor: "bg-indigo-500/20 text-indigo-400 border-indigo-500/30",
    description: "Legal clause audit & compliance",
  },
  {
    id: "calls",
    label: "Voice Call Summaries",
    shortLabel: "Call Recordings",
    icon: Headphones,
    badge: "S3 Vault",
    badgeColor: "bg-sky-500/20 text-sky-400 border-sky-500/30",
    description: "Presigned audio & text analytics",
  },
  {
    id: "logs",
    label: "System Console & Logs",
    shortLabel: "Diagnostics",
    icon: Terminal,
    badge: "Live Stream",
    badgeColor: "bg-teal-500/20 text-teal-400 border-teal-500/30",
    description: "Orchestrator runtime telemetry",
  },
];

export function AdminSidebar({
  activeTab,
  onSelectTab,
  user,
}: AdminSidebarProps): React.ReactElement {
  const [mobileOpen, setMobileOpen] = useState(false);

  const sidebarContent = (
    <div className="flex flex-col h-full space-y-6">
      {/* Administrator Header Card */}
      <div className="relative overflow-hidden bg-theme-main p-4 rounded-2xl border border-theme-border dark:border-white/15 shadow-md dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.08)]">
        <div className="flex items-center gap-3 relative z-10">
          <div className="w-10 h-10 rounded-xl bg-cyan-500/10 border border-cyan-500/30 text-cyan-400 flex items-center justify-center font-bold shadow-inner">
            <ShieldAlert className="w-5 h-5" />
          </div>
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-1.5">
              <h3 className="text-sm font-bold text-white truncate">
                {user?.name || "System Admin"}
              </h3>
            </div>
            <span className="inline-flex items-center gap-1.5 text-[10px] font-mono text-cyan-400 font-bold uppercase tracking-wider">
              <span className="w-1.5 h-1.5 rounded-full bg-cyan-400 animate-pulse" />
              Role: Administrator
            </span>
          </div>
        </div>

        {/* Back to User Dashboard Link */}
        <div className="mt-3.5 pt-3 border-t border-theme-border dark:border-white/10 flex items-center justify-between text-[11px] relative z-10">
          <span className="text-zinc-400 font-mono">Consumer Portal:</span>
          <Link
            href="/dashboard"
            className="inline-flex items-center gap-1 text-white font-bold bg-white/10 hover:bg-white/20 px-2.5 py-1 rounded-full border border-white/25 transition-colors font-mono text-[10px] shadow-[inset_0_1px_0_0_rgba(255,255,255,0.1)]"
          >
            <ArrowLeft className="w-3 h-3" />
            User Dashboard
          </Link>
        </div>
      </div>

      {/* Admin Content Navigation List */}
      <div className="space-y-1.5 flex-1">
        <div className="flex items-center justify-between px-3 mb-2">
          <p className="text-[10px] font-mono font-bold text-zinc-400 uppercase tracking-wider">
            Admin Modules ({ADMIN_NAV_ITEMS.length})
          </p>
          <span className="text-[9px] font-mono text-cyan-400 font-bold">
            v2.4 Live
          </span>
        </div>

        <div className="space-y-1.5">
          {ADMIN_NAV_ITEMS.map((item) => {
            const Icon = item.icon;
            const isActive = activeTab === item.id;

            return (
              <button
                key={item.id}
                onClick={() => {
                  onSelectTab(item.id);
                  setMobileOpen(false);
                }}
                className={`w-full flex items-center justify-between p-3 rounded-2xl transition-all duration-200 group text-left relative overflow-hidden ${
                  isActive
                    ? "bg-white/10 text-white border border-white/30 dark:border-white/40 shadow-md shadow-black/50 dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.25)] font-bold"
                    : "text-zinc-400 hover:text-white hover:bg-white/5 border border-transparent hover:border-white/10 dark:hover:border-white/15"
                }`}
              >
                {isActive && (
                  <motion.div
                    layoutId="activeAdminSidebarGlow"
                    className="absolute left-0 top-1/2 -translate-y-1/2 w-1.5 h-7 bg-cyan-400 rounded-r-full shadow-[0_0_14px_rgba(56,189,248,0.95)]"
                  />
                )}

                <div className="flex items-center gap-3 relative z-10 pl-1">
                  <div
                    className={`p-2 rounded-xl transition-colors ${
                      isActive
                        ? "bg-cyan-500/20 text-cyan-400 border border-cyan-500/40 dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.2)]"
                        : "bg-theme-main text-zinc-400 group-hover:text-white border border-theme-border dark:border-white/10 dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.06)]"
                    }`}
                  >
                    <Icon className="w-4 h-4" />
                  </div>
                  <div>
                    <span className="text-xs font-bold block leading-tight text-white">
                      {item.label}
                    </span>
                    <span className="text-[10px] text-zinc-400 block font-normal mt-0.5 line-clamp-1">
                      {item.description}
                    </span>
                  </div>
                </div>

                {item.badge && (
                  <span
                    className={`text-[9px] font-mono font-bold px-2 py-0.5 rounded-full border shadow-[inset_0_1px_0_0_rgba(255,255,255,0.1)] relative z-10 ${item.badgeColor}`}
                  >
                    {item.badge}
                  </span>
                )}
              </button>
            );
          })}
        </div>
      </div>

      {/* Admin Infrastructure Metrics Widget */}
      <div className="mt-auto pt-4">
        <div className="relative p-4 rounded-2xl bg-theme-main border border-theme-border dark:border-white/15 shadow-md dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.08)]">
          <div className="flex items-center justify-between mb-2">
            <span className="text-[10px] font-mono font-bold text-white uppercase tracking-wider flex items-center gap-1.5">
              <Server className="w-3.5 h-3.5 text-cyan-400" />
              Engine Health
            </span>
            <span className="text-[9px] font-mono text-emerald-400 bg-emerald-500/10 px-2 py-0.5 rounded-md border border-emerald-500/30 font-bold">
              All 6 Online
            </span>
          </div>

          <div className="grid grid-cols-2 gap-2 text-[10px] font-mono text-zinc-400 pt-2 border-t border-theme-border dark:border-white/10">
            <div>
              <span className="block text-[9px] text-zinc-500">DB Latency</span>
              <span className="text-white font-bold">&lt; 3.2 ms</span>
            </div>
            <div>
              <span className="block text-[9px] text-zinc-500">Vector Index</span>
              <span className="text-cyan-400 font-bold">Sync Active</span>
            </div>
          </div>
        </div>
      </div>
    </div>
  );

  return (
    <>
      {/* Mobile Sidebar Bar */}
      <div className="lg:hidden flex items-center justify-between p-4 bg-theme-card border border-theme-border dark:border-white/15 rounded-2xl mb-6 shadow-md dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.1)]">
        <div className="flex items-center gap-3">
          <div className="p-2.5 bg-cyan-500/10 text-cyan-400 rounded-xl border border-cyan-500/20">
            <Sliders className="w-5 h-5" />
          </div>
          <div>
            <h4 className="text-xs font-bold text-white">Admin Module Menu</h4>
            <p className="text-[10px] text-zinc-400 font-mono">
              Active: {ADMIN_NAV_ITEMS.find((i) => i.id === activeTab)?.label}
            </p>
          </div>
        </div>

        <button
          onClick={() => setMobileOpen(!mobileOpen)}
          className="p-2.5 bg-theme-main border border-theme-border dark:border-white/15 rounded-xl text-theme-text hover:bg-theme-card-hover transition-colors flex items-center gap-2 text-xs font-bold shadow-[inset_0_1px_0_0_rgba(255,255,255,0.08)]"
        >
          <span>{mobileOpen ? "Close" : "Menu"}</span>
          {mobileOpen ? <X className="w-4 h-4" /> : <Menu className="w-4 h-4" />}
        </button>
      </div>

      {/* Mobile Drawer */}
      <AnimatePresence>
        {mobileOpen && (
          <motion.div
            initial={{ opacity: 0, height: 0 }}
            animate={{ opacity: 1, height: "auto" }}
            exit={{ opacity: 0, height: 0 }}
            className="lg:hidden bg-theme-card border border-theme-border dark:border-white/15 rounded-3xl p-5 mb-6 backdrop-blur-xl shadow-2xl overflow-hidden dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.12)]"
          >
            {sidebarContent}
          </motion.div>
        )}
      </AnimatePresence>

      {/* Desktop Sticky Left Sidebar */}
      <aside className="hidden lg:block w-72 xl:w-80 shrink-0 self-start">
        <div className="sticky top-24 bg-theme-card border border-theme-border dark:border-white/20 backdrop-blur-xl rounded-3xl p-5 shadow-xl min-h-[calc(100vh-120px)] flex flex-col transition-colors duration-300 dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.15)]">
          {sidebarContent}
        </div>
      </aside>
    </>
  );
}
