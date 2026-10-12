"use client";

import React, { useState } from "react";
import Link from "next/link";
import { motion, AnimatePresence } from "framer-motion";
import {
  Search,
  ShieldCheck,
  Sparkles,
  Menu,
  X,
  FileCheck,
  Sliders,
  BrainCircuit,
  ShieldAlert,
  Database,
  Workflow,
  Headphones,
  Terminal,
  ExternalLink,
} from "lucide-react";
import { UserProfile } from "../../types/user";
import { UserProfileCard } from "./sidebar/UserProfileCard";
import { LiveKitEngineCard } from "./sidebar/LiveKitEngineCard";

export type DashboardTab = "assistant" | "orbs" | "search" | "contracts" | "kyc";

interface DashboardSidebarProps {
  activeTab: DashboardTab;
  onSelectTab: (tab: DashboardTab) => void;
  user: UserProfile | null;
  isCallActive?: boolean;
  onToggleCall?: () => void;
}

export function DashboardSidebar({
  activeTab,
  onSelectTab,
  user,
  isCallActive = false,
  onToggleCall,
}: DashboardSidebarProps): React.ReactElement {
  const [mobileOpen, setMobileOpen] = useState(false);

  const mainNavItems = [
    {
      id: "assistant" as DashboardTab,
      label: "AI Voice Assistant",
      icon: BrainCircuit,
      badge: "LIVE",
      badgeColor: "bg-theme-main text-theme-text border-theme-border",
      description: "WebRTC voice AI assistant",
    },
    {
      id: "orbs" as DashboardTab,
      label: "Voice Agent Orbs",
      icon: Sparkles,
      badge: "Models",
      badgeColor: "bg-cyan-500/20 text-cyan-400 border-cyan-500/30",
      description: "Bilingual voice presets & neural models",
    },
    {
      id: "search" as DashboardTab,
      label: "Property Search",
      icon: Search,
      description: "AI natural language listing search",
    },
    {
      id: "contracts" as DashboardTab,
      label: "Tenancy Contracts",
      icon: FileCheck,
      badge: "RAG Audit",
      badgeColor: "bg-indigo-500/20 text-indigo-400 border-indigo-500/30",
      description: "Contract upload & clause extraction",
    },
  ];

  const secondaryNavItems = [
    {
      id: "kyc" as DashboardTab,
      label: "KYC Verification",
      icon: ShieldCheck,
      badge: user?.is_verified ? "Verified" : "Verification Needed",
      badgeColor: user?.is_verified
        ? "bg-emerald-500/20 text-emerald-400 border-emerald-500/30"
        : "bg-rose-500/20 text-rose-400 border-rose-500/30",
      description: "Government ID verification portal",
    },
  ];

  const adminNavItems = [
    {
      label: "Admin Control Center",
      href: "/admin",
      icon: ShieldAlert,
      badge: "ROOT",
      badgeColor: "bg-cyan-500/20 text-cyan-400 border-cyan-500/30",
      description: "Unified control overview",
    },
    {
      label: "Data Ingestion & Vectors",
      href: "/admin",
      icon: Database,
      badge: "Postgres",
      badgeColor: "bg-cyan-500/20 text-cyan-400 border-cyan-500/30",
      description: "Listing embeddings & crawl",
    },
    {
      label: "DAG Workflow Editor",
      href: "/admin",
      icon: Workflow,
      badge: "Memgraph",
      badgeColor: "bg-emerald-500/20 text-emerald-400 border-emerald-500/30",
      description: "Graph state machine",
    },
    {
      label: "Tenancy Contracts Review",
      href: "/admin",
      icon: FileCheck,
      badge: "Ejari Queue",
      badgeColor: "bg-indigo-500/20 text-indigo-400 border-indigo-500/30",
      description: "Audit & compliance queue",
    },
    {
      label: "Voice Call Summaries",
      href: "/admin",
      icon: Headphones,
      badge: "S3 Vault",
      badgeColor: "bg-sky-500/20 text-sky-400 border-sky-500/30",
      description: "Presigned audio transcripts",
    },
    {
      label: "Live Telemetry & Logs",
      href: "/admin",
      icon: Terminal,
      badge: "Stream",
      badgeColor: "bg-teal-500/20 text-teal-400 border-teal-500/30",
      description: "Tokio runtime diagnostics",
    },
  ];

  const sidebarContent = (
    <div className="flex flex-col h-full space-y-6">
      <UserProfileCard
        user={user}
        onSelectKyc={() => {
          onSelectTab("kyc");
          setMobileOpen(false);
        }}
      />

      {/* Main Studio Navigation */}
      <div className="space-y-1">
        <p className="text-[10px] font-mono font-bold text-theme-sub uppercase tracking-wider px-3 mb-2">
          User Navigation
        </p>
        <div className="space-y-1.5">
          {mainNavItems.map((item) => {
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
                    ? "bg-theme-main text-theme-text border border-theme-border dark:border-white/40 shadow-md font-bold"
                    : "text-theme-sub hover:text-theme-text hover:bg-black/5 dark:hover:bg-white/5 border border-transparent hover:border-theme-border dark:hover:border-white/15"
                }`}
              >
                {isActive && (
                  <motion.div
                    layoutId="activeSidebarGlow"
                    className="absolute left-0 top-1/2 -translate-y-1/2 w-1.5 h-7 bg-theme-accent rounded-r-full shadow-[0_0_14px_rgba(255,255,255,0.95)]"
                  />
                )}

                <div className="flex items-center gap-3 relative z-10 pl-1">
                  <div
                    className={`p-2 rounded-xl transition-colors ${
                      isActive
                        ? "bg-theme-card text-theme-text border border-theme-border dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.2)]"
                        : "bg-theme-main text-theme-sub group-hover:text-theme-text border border-theme-border dark:border-white/10 dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.06)]"
                    }`}
                  >
                    <Icon className="w-4 h-4" />
                  </div>
                  <div>
                    <span className="text-xs font-bold block leading-tight text-theme-text">{item.label}</span>
                    <span className="text-[10px] text-theme-sub block font-normal mt-0.5 line-clamp-1">
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

      {/* Security Navigation */}
      <div className="space-y-1">
        <p className="text-[10px] font-mono font-bold text-theme-sub uppercase tracking-wider px-3 mb-2">
          Security & Verification
        </p>
        <div className="space-y-1.5">
          {secondaryNavItems.map((item) => {
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
                    ? "bg-emerald-500/15 text-theme-text border border-emerald-500/40 dark:border-emerald-400/50 shadow-md shadow-emerald-500/5 dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.15)] font-bold"
                    : "text-theme-sub hover:text-theme-text hover:bg-black/5 dark:hover:bg-white/5 border border-transparent hover:border-theme-border dark:hover:border-white/15"
                }`}
              >
                <div className="flex items-center gap-3 relative z-10 pl-1">
                  <div
                    className={`p-2 rounded-xl transition-colors ${
                      isActive
                        ? "bg-emerald-500/20 text-emerald-400 border border-emerald-500/40 dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.15)]"
                        : "bg-theme-main text-theme-sub group-hover:text-theme-text border border-theme-border dark:border-white/10 dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.06)]"
                    }`}
                  >
                    <Icon className="w-4 h-4" />
                  </div>
                  <div>
                    <span className="text-xs font-bold block leading-tight text-theme-text">{item.label}</span>
                    <span className="text-[10px] text-theme-sub block font-normal mt-0.5 line-clamp-1">
                      {item.description}
                    </span>
                  </div>
                </div>

                {item.badge && (
                  <span
                    className={`text-[9px] font-mono font-bold px-2 py-0.5 rounded-full border relative z-10 shadow-[inset_0_1px_0_0_rgba(255,255,255,0.1)] ${item.badgeColor}`}
                  >
                    {item.badge}
                  </span>
                )}
              </button>
            );
          })}
        </div>
      </div>

      {/* Admin Module Content List (Only rendered for Admin role) */}
      {user?.role === "admin" && (
        <div className="space-y-1 pt-2 border-t border-theme-border dark:border-white/10">
          <div className="flex items-center justify-between px-3 mb-2">
            <p className="text-[10px] font-mono font-bold text-cyan-400 uppercase tracking-wider flex items-center gap-1.5">
              <ShieldAlert className="w-3 h-3" />
              Admin Portal Tools
            </p>
            <span className="text-[9px] font-mono text-theme-sub">
              Admin Only
            </span>
          </div>
          <div className="space-y-1.5">
            {adminNavItems.map((adminItem) => {
              const Icon = adminItem.icon;
              return (
                <Link
                  key={adminItem.label}
                  href={adminItem.href}
                  className="w-full flex items-center justify-between p-2.5 rounded-2xl transition-all duration-200 group text-left relative overflow-hidden text-theme-sub hover:text-theme-text hover:bg-cyan-500/10 border border-transparent hover:border-cyan-500/30"
                >
                  <div className="flex items-center gap-3 relative z-10 pl-1">
                    <div className="p-2 rounded-xl bg-theme-main text-cyan-400 border border-cyan-500/20 group-hover:border-cyan-500/40">
                      <Icon className="w-4 h-4" />
                    </div>
                    <div>
                      <span className="text-xs font-bold block leading-tight text-theme-text flex items-center gap-1">
                        {adminItem.label}
                        <ExternalLink className="w-2.5 h-2.5 text-theme-sub group-hover:text-cyan-400" />
                      </span>
                      <span className="text-[10px] text-theme-sub block font-normal mt-0.5 line-clamp-1">
                        {adminItem.description}
                      </span>
                    </div>
                  </div>

                  {adminItem.badge && (
                    <span className={`text-[9px] font-mono font-bold px-2 py-0.5 rounded-full border relative z-10 ${adminItem.badgeColor}`}>
                      {adminItem.badge}
                    </span>
                  )}
                </Link>
              );
            })}
          </div>
        </div>
      )}

      {/* LiveKit Engine Status Card */}
      <LiveKitEngineCard
        isCallActive={isCallActive}
        onToggleCall={onToggleCall}
      />
    </div>
  );

  return (
    <>
      {/* Mobile Sidebar Bar */}
      <div className="lg:hidden flex items-center justify-between p-4 bg-theme-card border border-theme-border dark:border-white/15 rounded-2xl mb-6 shadow-md dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.1)]">
        <div className="flex items-center gap-3">
          <div className="p-2.5 bg-theme-main text-theme-text rounded-xl border border-theme-border dark:border-white/20">
            <Sliders className="w-5 h-5" />
          </div>
          <div>
            <h4 className="text-xs font-bold text-theme-text">Dashboard Menu</h4>
            <p className="text-[10px] text-theme-sub font-mono">
              Active: {mainNavItems.find((i) => i.id === activeTab)?.label || secondaryNavItems.find((i) => i.id === activeTab)?.label}
            </p>
          </div>
        </div>

        <button
          onClick={() => setMobileOpen(!mobileOpen)}
          className="p-2.5 bg-theme-main border border-theme-border dark:border-white/15 rounded-xl text-theme-text hover:bg-theme-card-hover transition-colors flex items-center gap-2 text-xs font-bold shadow-[inset_0_1px_0_0_rgba(255,255,255,0.08)]"
        >
          <span>{mobileOpen ? "Close Menu" : "Menu"}</span>
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
