"use client";

import React, { useState } from "react";
import { Header } from "../../components/Header";
import { WebRtcCallWidget } from "../../components/WebRtcCallWidget";
import { PrivateRoute } from "../../components/PrivateRoute";
import { PropertySearch } from "../../components/PropertySearch";
import { useUserProfile, useVerifyDocument, useGoogleLogin, useListings } from "../../lib/api";
import { motion } from "framer-motion";
import { User, BadgeCheck, FileWarning, Fingerprint, Phone } from "lucide-react";

export default function UserDashboardPage() {
  const [agentModeActive, setAgentModeActive] = useState(true);
  const { data: userData } = useUserProfile();
  const { data: listingsData, isLoading: listingsLoading } = useListings();
  const verifyDocMutation = useVerifyDocument();
  const googleLoginMutation = useGoogleLogin();

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



  return (
    <div className="min-h-screen bg-[#F8F9FA] text-slate-800 font-sans antialiased overflow-hidden selection:bg-blue-500/30 selection:text-blue-900">
      {/* Background Gradients */}
      <div className="fixed inset-0 z-0 pointer-events-none">
        <div className="absolute top-[-20%] left-[-10%] w-[50%] h-[50%] rounded-full bg-blue-100/60 blur-[120px]" />
        <div className="absolute bottom-[-10%] right-[-10%] w-[40%] h-[40%] rounded-full bg-cyan-100/60 blur-[100px]" />
        <div className="absolute inset-0 bg-[url('https://grainy-gradients.vercel.app/noise.svg')] opacity-5 mix-blend-overlay"></div>
      </div>

      <div className="relative z-10 flex flex-col min-h-screen">
        <Header
          user={user}
          agentModeActive={agentModeActive}
          onToggleAgentMode={setAgentModeActive}
        />

        <main className="flex-1 max-w-7xl mx-auto px-4 md:px-6 py-24 w-full">
          <PrivateRoute user={user} requiredRole="user">
            <div className="space-y-12">


              <motion.div initial={{ opacity: 0, y: 20 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: 0.1 }}>
                <WebRtcCallWidget callId="call-101" userId={user?.id || ""} agentModeActive={agentModeActive} />
              </motion.div>

              {/* KYC Government ID Verification Portal */}
              <motion.div 
                initial={{ opacity: 0, y: 20 }} 
                animate={{ opacity: 1, y: 0 }} 
                transition={{ delay: 0.2 }}
                className="bg-white border border-slate-200 rounded-3xl p-8 shadow-xl shadow-slate-200/50 space-y-6 relative overflow-hidden"
              >
                <div className="absolute top-0 right-0 -mt-10 -mr-10 w-40 h-40 bg-emerald-500/5 rounded-full blur-3xl pointer-events-none" />
                
                <div className="flex items-center gap-4 relative z-10">
                  <div className="p-3 bg-emerald-500/10 rounded-xl border border-emerald-500/20 text-emerald-400">
                    <Fingerprint className="w-6 h-6" />
                  </div>
                  <div>
                    <h3 className="text-xl font-bold text-slate-800 tracking-tight">KYC Document Verification</h3>
                    <p className="text-xs text-slate-500">
                      Viewing slot reservations and tenancy contract signatures require a verified Government ID document.
                    </p>
                  </div>
                </div>

                <div className="relative z-10">
                  {user?.is_verified ? (
                    <div className="p-5 rounded-2xl bg-emerald-500/10 border border-emerald-500/30 text-emerald-300 text-sm font-semibold flex items-center justify-between shadow-inner">
                      <div className="flex items-center gap-3">
                        <BadgeCheck className="w-5 h-5 text-emerald-400" />
                        <span>Government ID Verified: {user?.national_id_number || "ID-1990-1234567-1"}. Viewing slot reservation unlocked!</span>
                      </div>
                      <span className="text-[10px] font-mono text-emerald-400 bg-emerald-950/80 border border-emerald-800/50 px-3 py-1 rounded-full tracking-wider shadow-sm">
                        STATUS: ACTIVE
                      </span>
                    </div>
                  ) : (
                    <form onSubmit={handleVerifyNationalId} className="flex flex-col sm:flex-row gap-4 bg-slate-50 p-4 rounded-2xl border border-slate-200 shadow-inner">
                      <input
                        type="text"
                        placeholder="Enter Government ID Number (e.g. ID-1990-1234567-1)"
                        value={nationalIdInput}
                        onChange={(e) => setNationalIdInput(e.target.value)}
                        className="flex-1 bg-white border border-slate-200 text-slate-800 rounded-xl px-4 py-3 text-sm focus:outline-none focus:border-emerald-500/50 focus:ring-1 focus:ring-emerald-500/50 transition-all shadow-sm"
                      />
                      <button
                        type="submit"
                        disabled={verifyDocMutation.isPending}
                        className="bg-emerald-600 hover:bg-emerald-500 text-white font-bold text-sm px-8 py-3 rounded-xl transition-all shadow-[0_0_20px_-5px_rgba(16,185,129,0.4)] disabled:opacity-50 disabled:cursor-not-allowed whitespace-nowrap"
                      >
                        {verifyDocMutation.isPending ? "Verifying..." : "Verify Government ID"}
                      </button>
                    </form>
                  )}
                </div>
              </motion.div>

              {/* Property Search Widget */}
              <motion.div 
                initial={{ opacity: 0, y: 20 }} 
                animate={{ opacity: 1, y: 0 }} 
                transition={{ delay: 0.3 }}
                className="bg-white border border-slate-200 rounded-3xl p-8 shadow-xl shadow-slate-200/50 space-y-6 relative overflow-hidden"
              >
                <div className="absolute top-0 left-0 -mt-20 -ml-20 w-64 h-64 bg-indigo-500/5 rounded-full blur-3xl pointer-events-none" />
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
            </div>
          </PrivateRoute>
        </main>
      </div>
    </div>
  );
}
