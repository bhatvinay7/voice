"use client";

import React, { useState } from "react";
import { Header } from "../../components/Header";
import { AdminDataIngestion } from "../../components/AdminDataIngestion";
import { AdminGraphEditor } from "../../components/AdminGraphEditor";
import { AdminContractReview } from "../../components/AdminContractReview";
import { AdminCallSummaries } from "../../components/AdminCallSummaries";
import { AdminLogsConsole } from "../../components/AdminLogsConsole";
import { AdminSidebar } from "../../components/admin/AdminSidebar";
import { AdminErrorBoundary } from "../../components/admin/AdminErrorBoundary";
import { PrivateRoute } from "../../components/PrivateRoute";
import { useUserProfile } from "../../lib/api";
import { motion, AnimatePresence } from "framer-motion";
import { ShieldAlert, Sparkles, Layers } from "lucide-react";
import { AdminTab } from "../../../types/admin";

export default function AdminPortalPage() {
  const [agentModeActive, setAgentModeActive] = useState(true);
  const [activeTab, setActiveTab] = useState<AdminTab>("all");
  const { data: userData, isLoading } = useUserProfile();

  const user = userData?.user ?? null;

  const renderContent = () => {
    switch (activeTab) {
      case "ingestion":
        return (
          <motion.div
            key="ingestion"
            initial={{ opacity: 0, y: 15 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -15 }}
            transition={{ duration: 0.2 }}
          >
            <AdminErrorBoundary title="Data Ingestion">
              <AdminDataIngestion />
            </AdminErrorBoundary>
          </motion.div>
        );
      case "workflows":
        return (
          <motion.div
            key="workflows"
            initial={{ opacity: 0, y: 15 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -15 }}
            transition={{ duration: 0.2 }}
          >
            <AdminErrorBoundary title="Workflows DAG">
              <AdminGraphEditor />
            </AdminErrorBoundary>
          </motion.div>
        );
      case "contracts":
        return (
          <motion.div
            key="contracts"
            initial={{ opacity: 0, y: 15 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -15 }}
            transition={{ duration: 0.2 }}
          >
            <AdminErrorBoundary title="Contract Review Queue">
              <AdminContractReview />
            </AdminErrorBoundary>
          </motion.div>
        );
      case "calls":
        return (
          <motion.div
            key="calls"
            initial={{ opacity: 0, y: 15 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -15 }}
            transition={{ duration: 0.2 }}
          >
            <AdminErrorBoundary title="Call Summaries">
              <AdminCallSummaries />
            </AdminErrorBoundary>
          </motion.div>
        );
      case "logs":
        return (
          <motion.div
            key="logs"
            initial={{ opacity: 0, y: 15 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -15 }}
            transition={{ duration: 0.2 }}
          >
            <AdminErrorBoundary title="Logs Console">
              <AdminLogsConsole />
            </AdminErrorBoundary>
          </motion.div>
        );
      case "all":
      default:
        return (
          <motion.div
            key="all"
            initial={{ opacity: 0, y: 15 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -15 }}
            transition={{ duration: 0.2 }}
            className="space-y-10"
          >
            <AdminErrorBoundary title="Data Ingestion">
              <AdminDataIngestion />
            </AdminErrorBoundary>
            <AdminErrorBoundary title="Workflows DAG">
              <AdminGraphEditor />
            </AdminErrorBoundary>
            <AdminErrorBoundary title="Contract Review Queue">
              <AdminContractReview />
            </AdminErrorBoundary>
            <div className="grid grid-cols-1 lg:grid-cols-2 gap-10">
              <AdminErrorBoundary title="Call Summaries">
                <AdminCallSummaries />
              </AdminErrorBoundary>
              <AdminErrorBoundary title="Logs Console">
                <AdminLogsConsole />
              </AdminErrorBoundary>
            </div>
          </motion.div>
        );
    }
  };

  return (
    <div className="min-h-screen bg-theme-main text-theme-text font-sans antialiased overflow-x-hidden transition-colors duration-300">
      <div className="relative z-10 flex flex-col min-h-screen">
        <Header
          user={user}
          agentModeActive={agentModeActive}
          onToggleAgentMode={setAgentModeActive}
        />

        <main className="flex-1 w-full max-w-[1920px] pt-20 pb-12 pl-2 sm:pl-3 lg:pl-4 xl:pl-5 pr-3 sm:pr-6 lg:pr-8 mx-auto">
          <PrivateRoute user={user} isLoading={isLoading} requiredRole="admin">
            <div className="flex flex-col lg:flex-row gap-5 lg:gap-6 items-start">
              {/* Admin Sidebar Navigation */}
              <AdminSidebar
                activeTab={activeTab}
                onSelectTab={setActiveTab}
                user={user}
              />

              {/* Main Content Area */}
              <div className="flex-1 min-w-0 w-full space-y-6">
                {/* Control Center Banner */}
                <motion.div
                  initial={{ opacity: 0, y: 12 }}
                  animate={{ opacity: 1, y: 0 }}
                  className="relative overflow-hidden flex flex-col md:flex-row md:items-center justify-between gap-6 bg-theme-card border border-theme-border dark:border-white/15 p-6 sm:p-8 rounded-3xl group transition-colors duration-300 shadow-xl dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.12)]"
                >
                  <div className="relative z-10 flex items-start gap-5">
                    <div className="hidden sm:flex p-3.5 rounded-2xl bg-cyan-500/10 border border-cyan-500/20 text-cyan-400">
                      <ShieldAlert className="w-7 h-7" />
                    </div>
                    <div className="space-y-1.5">
                      <div className="flex items-center gap-3">
                        <h2 className="text-2xl sm:text-3xl font-extrabold text-theme-text tracking-tight font-serif">
                          SaaS Admin Control Center
                        </h2>
                      </div>
                      <p className="text-xs sm:text-sm text-theme-sub max-w-xl leading-relaxed">
                        Manage PostgreSQL & Vector DB embeddings, Memgraph DAG workflow state machines, contract queues, and audio summaries.
                      </p>
                    </div>
                  </div>

                  <div className="relative z-10 self-start md:self-center flex items-center gap-2">
                    <span className="inline-flex items-center gap-2 text-xs font-mono bg-cyan-500/10 text-cyan-400 border border-cyan-500/30 px-3.5 py-1.5 rounded-full font-bold shadow-[inset_0_1px_0_0_rgba(255,255,255,0.08)]">
                      <span className="w-2 h-2 rounded-full bg-cyan-400 animate-pulse" />
                      ROLE: ADMINISTRATOR
                    </span>
                  </div>
                </motion.div>

                {/* Tabbed Content View */}
                <AnimatePresence mode="wait">
                  {renderContent()}
                </AnimatePresence>
              </div>
            </div>
          </PrivateRoute>
        </main>
      </div>
    </div>
  );
}
