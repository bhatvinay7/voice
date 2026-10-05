"use client";

import React, { useState } from "react";
import { Header } from "../../components/Header";
import { AdminDataIngestion } from "../../components/AdminDataIngestion";
import { AdminGraphEditor } from "../../components/AdminGraphEditor";
import { AdminContractReview } from "../../components/AdminContractReview";
import { AdminCallSummaries } from "../../components/AdminCallSummaries";
import { AdminLogsConsole } from "../../components/AdminLogsConsole";
import { PrivateRoute } from "../../components/PrivateRoute";
import { useUserProfile, useGoogleLogin } from "../../lib/api";
import { motion } from "framer-motion";
import { ShieldAlert, Database, Cpu, Activity, Workflow } from "lucide-react";

export default function AdminPortalPage() {
  const [agentModeActive, setAgentModeActive] = useState(true);
  const { data: userData } = useUserProfile();
  const googleLoginMutation = useGoogleLogin();

  const user = userData?.user || {
    id: "user-admin",
    name: "System Administrator",
    email: "admin@estate.ae",
    phone: "+971500000999",
    is_verified: true,
    role: "admin",
  };



  return (
    <div className="min-h-screen bg-[#F8F9FA] text-slate-800 font-sans antialiased overflow-hidden selection:bg-purple-500/30 selection:text-purple-900">
      {/* Background Gradients */}
      <div className="fixed inset-0 z-0 pointer-events-none">
        <div className="absolute top-[-20%] right-[-10%] w-[50%] h-[50%] rounded-full bg-purple-100/60 blur-[120px]" />
        <div className="absolute bottom-[-10%] left-[-10%] w-[40%] h-[40%] rounded-full bg-indigo-100/60 blur-[100px]" />
        <div className="absolute inset-0 bg-[url('https://grainy-gradients.vercel.app/noise.svg')] opacity-5 mix-blend-overlay"></div>
      </div>

      <div className="relative z-10 flex flex-col min-h-screen">
        <Header
          user={user}
          agentModeActive={agentModeActive}
          onToggleAgentMode={setAgentModeActive}
        />

        <main className="flex-1 max-w-7xl mx-auto px-4 md:px-6 py-24 w-full">
          <PrivateRoute user={user} requiredRole="admin">
            <div className="space-y-12">
              <motion.div 
                initial={{ opacity: 0, y: 20 }}
                animate={{ opacity: 1, y: 0 }}
                className="relative overflow-hidden flex flex-col md:flex-row md:items-center justify-between gap-6 bg-white border border-slate-200 p-8 rounded-3xl shadow-xl shadow-slate-200/50 group"
              >
                <div className="absolute top-0 right-0 -mt-10 -mr-10 w-40 h-40 bg-purple-100 rounded-full blur-3xl pointer-events-none group-hover:bg-purple-200 transition-all duration-700" />
                
                <div className="relative z-10 flex items-start gap-5">
                  <div className="hidden sm:flex p-4 rounded-2xl bg-purple-50 border border-purple-200 text-purple-600 shadow-sm">
                    <ShieldAlert className="w-8 h-8" />
                  </div>
                  <div className="space-y-2">
                    <h2 className="text-3xl font-extrabold text-transparent bg-clip-text bg-gradient-to-r from-purple-800 to-indigo-600 tracking-tight">
                      SaaS Admin Control Center
                    </h2>
                    <p className="text-sm text-slate-500 max-w-xl leading-relaxed">
                      Manage PostgreSQL & Vector DB embeddings, Memgraph DAG workflow state machines, contract queues, and R2 audio summaries.
                    </p>
                  </div>
                </div>
                
                <div className="relative z-10 self-start md:self-center">
                  <span className="inline-flex items-center gap-2 text-xs font-mono bg-purple-50 text-purple-700 border border-purple-200 px-4 py-2 rounded-full font-bold shadow-sm">
                    <div className="w-2 h-2 rounded-full bg-purple-500 animate-pulse" />
                    ROLE: ADMINISTRATOR
                  </span>
                </div>
              </motion.div>

              <div className="grid grid-cols-1 gap-10">
                <motion.div initial={{ opacity: 0, y: 20 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: 0.1 }}>
                  <AdminDataIngestion />
                </motion.div>
                
                <motion.div initial={{ opacity: 0, y: 20 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: 0.2 }}>
                  <AdminGraphEditor />
                </motion.div>
                
                <motion.div initial={{ opacity: 0, y: 20 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: 0.3 }}>
                  <AdminContractReview />
                </motion.div>
                
                <motion.div initial={{ opacity: 0, y: 20 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: 0.4 }}>
                  <div className="grid grid-cols-1 lg:grid-cols-2 gap-10">
                    <AdminCallSummaries />
                    <AdminLogsConsole />
                  </div>
                </motion.div>
              </div>
            </div>
          </PrivateRoute>
        </main>
      </div>
    </div>
  );
}
