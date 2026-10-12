"use client";

import React from "react";
import { UserProfile } from "../../types/user";
import { motion } from "framer-motion";
import { Lock, ShieldAlert, Loader2 } from "lucide-react";
import { useRouter } from "next/navigation";

interface PrivateRouteProps {
  user: UserProfile | null;
  isLoading?: boolean;
  requiredRole?: "user" | "admin";
  children: React.ReactNode;
}

export function PrivateRoute({
  user,
  isLoading = false,
  requiredRole = "user",
  children,
}: PrivateRouteProps): React.ReactElement | null {
  const router = useRouter();

  if (isLoading) {
    return (
      <div className="flex flex-col items-center justify-center min-h-[400px] space-y-4">
        <Loader2 className="w-10 h-10 text-cyan-400 animate-spin" />
        <p className="text-sm font-mono text-theme-sub">Verifying role & access permissions...</p>
      </div>
    );
  }

  if (!user) {
    return (
      <motion.div 
        initial={{ opacity: 0, scale: 0.95 }}
        animate={{ opacity: 1, scale: 1 }}
        className="bg-theme-card border border-theme-border dark:border-white/15 rounded-3xl p-10 text-center space-y-6 max-w-md mx-auto my-20 shadow-2xl transition-colors duration-300"
      >
        <div className="w-16 h-16 bg-cyan-500/10 text-cyan-400 rounded-full flex items-center justify-center mx-auto border border-cyan-500/20 shadow-sm">
          <Lock className="w-8 h-8" />
        </div>
        <div className="space-y-2">
          <h3 className="text-2xl font-bold text-theme-text tracking-tight font-serif">
            {requiredRole === "admin" ? "Admin Authentication Required" : "Authentication Required"}
          </h3>
          <p className="text-sm text-theme-sub leading-relaxed">
            {requiredRole === "admin"
              ? "Please sign in with administrator credentials to access the SaaS Admin Control Center."
              : "Please sign in to access this private section and interact with our AI voice agents."}
          </p>
        </div>
        <div className="space-y-3 pt-2">
          {requiredRole === "admin" ? (
            <button
              onClick={() => router.push("/admin/login")}
              className="w-full bg-cyan-500 hover:bg-cyan-400 text-slate-950 font-bold text-sm px-6 py-3 rounded-full transition-all shadow-md active:scale-95"
            >
              Sign In to Admin Portal
            </button>
          ) : (
            <button
              onClick={() => router.push("/auth")}
              className="w-full bg-theme-text text-theme-main font-bold text-sm px-6 py-3 rounded-full hover:opacity-90 transition-all shadow-md active:scale-95"
            >
              Sign In to Continue
            </button>
          )}
        </div>
      </motion.div>
    );
  }

  if (requiredRole === "admin" && user.role !== "admin") {
    return (
      <motion.div 
        initial={{ opacity: 0, scale: 0.95 }}
        animate={{ opacity: 1, scale: 1 }}
        className="bg-theme-card border border-rose-500/30 rounded-3xl p-10 text-center space-y-6 max-w-lg mx-auto my-20 shadow-2xl transition-colors duration-300"
      >
        <div className="w-16 h-16 bg-rose-500/10 text-rose-400 rounded-full flex items-center justify-center mx-auto border border-rose-500/20 shadow-sm">
          <ShieldAlert className="w-8 h-8" />
        </div>
        <div className="space-y-2">
          <h3 className="text-2xl font-bold text-theme-text tracking-tight font-serif">
            Administrator Access Required
          </h3>
          <p className="text-sm text-theme-sub leading-relaxed">
            Your account (<span className="text-theme-text font-mono font-medium">{user.email}</span>) holds role{" "}
            <span className="text-rose-400 font-mono font-bold uppercase">{user.role}</span>.
            Administrator privileges in the database are required to manage embeddings, workflows, and review queues.
          </p>
        </div>

        <div className="flex flex-col sm:flex-row gap-3 pt-2">
          <button
            onClick={() => router.push("/admin/login")}
            className="flex-1 bg-cyan-500 hover:bg-cyan-400 text-slate-950 font-bold text-xs sm:text-sm px-5 py-3 rounded-full transition-all shadow-md active:scale-95"
          >
            Sign In with Admin Account
          </button>
          <button
            onClick={() => router.push("/dashboard")}
            className="flex-1 bg-theme-card-hover text-theme-text border border-theme-border font-bold text-xs sm:text-sm px-5 py-3 rounded-full hover:bg-white/10 transition-all active:scale-95"
          >
            Return to Dashboard
          </button>
        </div>
      </motion.div>
    );
  }

  return <>{children}</>;
}
