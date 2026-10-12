"use client";

import React, { useState, useRef } from "react";
import { Header } from "../../components/Header";
import { WebRtcCallWidget } from "../../components/WebRtcCallWidget";
import { VoiceAgentOrbsDemo } from "../../components/VoiceAgentOrbsDemo";
import { PrivateRoute } from "../../components/PrivateRoute";
import { PropertySearch } from "../../components/PropertySearch";
import { DashboardSidebar, DashboardTab } from "../../components/DashboardSidebar";
import { useUserProfile, useVerifyDocument, useListings } from "../../lib/api";
import { motion, AnimatePresence } from "framer-motion";
import { BadgeCheck, Fingerprint } from "lucide-react";

export default function UserDashboardPage() {
  const [agentModeActive, setAgentModeActive] = useState(true);
  const [activeTab, setActiveTab] = useState<DashboardTab>("assistant");
  const [isCallActive, setIsCallActive] = useState(false);

  // Generate a unique, stable call ID per dashboard session.
  // Using useRef ensures it doesn't change on re-renders.
  const callIdRef = useRef<string>(`call-${typeof crypto !== 'undefined' ? crypto.randomUUID() : Date.now()}`);
  const callId = callIdRef.current;

  const { data: userData } = useUserProfile();
  const { data: listingsData, isLoading: listingsLoading } = useListings();
  const verifyDocMutation = useVerifyDocument();

  const [nationalIdInput, setNationalIdInput] = useState("");

  const user = userData?.user || null;

  const handleVerifyNationalId = (e: React.FormEvent) => {
    e.preventDefault();
    if (!nationalIdInput || !user) return;
    verifyDocMutation.mutate({
      userId: user.id,
      nationalIdNumber: nationalIdInput,
    });
  };

  const renderContent = () => {
    switch (activeTab) {
      case "assistant":
        return (
          <motion.div
            key="assistant"
            initial={{ opacity: 0, y: 12 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -12 }}
            transition={{ duration: 0.2 }}
          >
            <WebRtcCallWidget
              callId={callId}
              userId={user?.id || ""}
              agentModeActive={agentModeActive}
              onToggleCallState={setIsCallActive}
            />
          </motion.div>
        );
      case "orbs":
        return (
          <motion.div
            key="orbs"
            initial={{ opacity: 0, y: 12 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -12 }}
            transition={{ duration: 0.2 }}
          >
            <VoiceAgentOrbsDemo />
          </motion.div>
        );
      case "search":
        return (
          <motion.div
            key="search"
            initial={{ opacity: 0, y: 12 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -12 }}
            transition={{ duration: 0.2 }}
            className="bg-theme-card border border-theme-border dark:border-white/15 rounded-3xl p-6 sm:p-8 space-y-6 relative overflow-hidden transition-colors duration-300 shadow-xl dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.1)]"
          >
            <div className="relative z-10">
              <PropertySearch
                listings={listingsData?.listings || []}
                userId={user?.id || ""}
                isVerified={user?.is_verified || false}
                agentModeActive={agentModeActive}
                isLoading={listingsLoading}
              />
            </div>
          </motion.div>
        );
      case "contracts":
        return (
          <motion.div
            key="contracts"
            initial={{ opacity: 0, y: 12 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -12 }}
            transition={{ duration: 0.2 }}
            className="bg-theme-card border border-theme-border dark:border-white/15 rounded-[2.2rem] p-6 sm:p-8 space-y-6 relative overflow-hidden transition-colors duration-300 shadow-xl dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.1)]"
          >
            <div className="flex flex-col sm:flex-row items-start sm:items-center justify-between gap-4 border-b border-theme-border dark:border-white/10 pb-6">
              <div>
                <h3 className="text-2xl font-bold text-theme-text font-serif">
                  Tenancy Contract RAG Clause Audit
                </h3>
                <p className="text-xs text-theme-sub mt-1">
                  Upload tenancy agreements (Ejari / MOU) for automated RAG clause extraction, party verification, and maintenance liability audit.
                </p>
              </div>
              <span className="text-xs bg-indigo-500/10 text-indigo-400 px-3.5 py-1.5 rounded-full border border-indigo-500/30 font-mono font-bold shadow-[inset_0_1px_0_0_rgba(255,255,255,0.1)]">
                Vector RAG Audit Active
              </span>
            </div>

            <div className="bg-theme-main p-6 rounded-2xl border border-theme-border dark:border-white/15 space-y-4 dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.06)]">
              <h4 className="text-sm font-bold text-white font-sans">Upload Contract Document</h4>
              <div className="border-2 border-dashed border-theme-border dark:border-white/20 rounded-2xl p-8 text-center space-y-3 hover:border-white/40 transition-colors cursor-pointer bg-theme-card dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.08)]">
                <div className="w-12 h-12 rounded-2xl bg-white/10 text-white flex items-center justify-center mx-auto border border-white/20 dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.15)]">
                  <BadgeCheck className="w-6 h-6 text-white" />
                </div>
                <div>
                  <p className="text-sm font-bold text-white">Drag & drop your Tenancy Contract (PDF / DOCX)</p>
                  <p className="text-xs text-zinc-400 mt-1 font-mono">Max size 25MB &bull; Automated Ejari & RERA Compliance Extraction</p>
                </div>
                <button className="px-5 py-2.5 bg-white hover:bg-zinc-200 text-black font-bold text-xs rounded-xl shadow-md transition-all border border-white/30 dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.3)]">
                  Browse Files
                </button>
              </div>
            </div>
          </motion.div>
        );
      case "kyc":
        return (
          <motion.div
            key="kyc"
            initial={{ opacity: 0, y: 12 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -12 }}
            transition={{ duration: 0.2 }}
            className="bg-theme-card border border-theme-border dark:border-white/15 rounded-3xl p-6 sm:p-8 space-y-6 relative overflow-hidden transition-colors duration-300 shadow-xl dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.1)]"
          >
            <div className="absolute top-0 right-0 -mt-10 -mr-10 w-40 h-40 bg-emerald-500/5 rounded-full blur-3xl pointer-events-none" />

            <div className="flex items-center gap-4 relative z-10">
              <div className="p-3.5 bg-emerald-500/10 rounded-2xl border border-emerald-500/20 text-emerald-400 dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.1)]">
                <Fingerprint className="w-7 h-7" />
              </div>
              <div>
                <h3 className="text-xl font-bold text-theme-text tracking-tight font-serif">
                  KYC Document Verification
                </h3>
                <p className="text-xs text-theme-sub mt-1">
                  Viewing slot reservations and tenancy contract signatures require a verified Government ID document.
                </p>
              </div>
            </div>

            <div className="relative z-10">
              {user?.is_verified ? (
                <div className="p-5 rounded-2xl bg-emerald-500/10 border border-emerald-500/30 text-emerald-300 text-sm font-semibold flex flex-col sm:flex-row items-start sm:items-center justify-between gap-4 shadow-inner dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.1)]">
                  <div className="flex items-center gap-3">
                    <BadgeCheck className="w-5 h-5 text-emerald-400 shrink-0" />
                    <span>
                      Government ID Verified: {user?.national_id_number || "ID-1990-1234567-1"}. Slot reservations unlocked!
                    </span>
                  </div>
                  <span className="text-[10px] font-mono text-emerald-400 bg-emerald-950/80 border border-emerald-800/50 px-3 py-1 rounded-full tracking-wider shadow-sm">
                    STATUS: ACTIVE VERIFIED
                  </span>
                </div>
              ) : (
                <form
                  onSubmit={handleVerifyNationalId}
                  className="flex flex-col sm:flex-row gap-4 bg-theme-main p-4 rounded-2xl border border-theme-border dark:border-white/15 dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.06)]"
                >
                  <input
                    type="text"
                    placeholder="Enter Government ID Number (e.g. ID-1990-1234567-1)"
                    value={nationalIdInput}
                    onChange={(e) => setNationalIdInput(e.target.value)}
                    className="flex-1 bg-theme-card border border-theme-border dark:border-white/15 text-theme-text rounded-xl px-4 py-3 text-sm focus:outline-none focus:border-emerald-500/50 focus:ring-1 focus:ring-emerald-500/50 transition-all placeholder:text-theme-sub dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.06)]"
                  />
                  <button
                    type="submit"
                    disabled={verifyDocMutation.isPending}
                    className="bg-emerald-600 hover:bg-emerald-500 text-white font-bold text-sm px-8 py-3 rounded-xl transition-all disabled:opacity-50 disabled:cursor-not-allowed whitespace-nowrap shadow-lg shadow-emerald-600/20 dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.2)]"
                  >
                    {verifyDocMutation.isPending ? "Verifying..." : "Verify Government ID"}
                  </button>
                </form>
              )}
            </div>
          </motion.div>
        );
      default:
        return null;
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
          <PrivateRoute user={user} requiredRole="user">
            <div className="flex flex-col lg:flex-row gap-5 lg:gap-6 items-start">
              {/* Dashboard Sidebar - Left Aligned */}
              <DashboardSidebar
                activeTab={activeTab}
                onSelectTab={setActiveTab}
                user={user}
                isCallActive={isCallActive}
                onToggleCall={() => setIsCallActive(!isCallActive)}
              />

              {/* Main Content Area */}
              <div className="flex-1 min-w-0 w-full">
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
