"use client";

import React, { useState } from "react";
import { useRouter } from "next/navigation";
import { motion } from "framer-motion";
import { Shield, Lock, Mail, ArrowRight, AlertCircle, ArrowLeft, CheckCircle2 } from "lucide-react";
import { useAdminLogin } from "../../../lib/api";

export default function AdminLoginPage() {
  const router = useRouter();
  const adminLoginMutation = useAdminLogin();

  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [errorMsg, setErrorMsg] = useState("");
  const [successMsg, setSuccessMsg] = useState("");

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    setErrorMsg("");
    setSuccessMsg("");

    adminLoginMutation.mutate(
      { email, password },
      {
        onSuccess: (data) => {
          setSuccessMsg(`Welcome, ${data.user.name}. Role: ${data.user.role.toUpperCase()}`);
          setTimeout(() => {
            router.push("/admin");
          }, 800);
        },
        onError: (err: Error) => {
          setErrorMsg(err.message || "Admin login failed. Please ensure your account has administrator privileges.");
        },
      }
    );
  };

  return (
    <div className="min-h-screen bg-theme-main text-theme-text font-sans antialiased flex flex-col justify-center items-center p-4 sm:p-6 md:p-8 relative overflow-hidden transition-colors duration-300">
      {/* Background Radial Glow */}
      <div className="absolute top-1/4 left-1/2 -translate-x-1/2 -translate-y-1/2 w-[600px] h-[600px] bg-cyan-500/5 blur-[120px] rounded-full pointer-events-none" />

      {/* Top back navigation */}
      <div className="absolute top-8 left-8 z-20">
        <button
          onClick={() => router.push("/")}
          className="inline-flex items-center gap-2 text-xs font-mono text-theme-sub hover:text-theme-text transition bg-theme-card dark:bg-black dark:text-white px-4 py-2 rounded-full border border-theme-border dark:border-white/20"
        >
          <ArrowLeft className="w-3.5 h-3.5" />
          Back to Portal
        </button>
      </div>

      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.4 }}
        className="w-full max-w-md relative z-10"
      >
        <div className="rounded-[2.5rem] border border-theme-border dark:border-white/15 bg-theme-card dark:bg-black backdrop-blur-2xl p-8 sm:p-10 text-theme-text dark:text-white shadow-2xl relative overflow-hidden dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.12)]">
          {/* Top Specular Gradient Line */}
          <div className="absolute top-0 left-0 right-0 h-1 bg-gradient-to-r from-cyan-500 via-white to-cyan-400" />

          {/* Header */}
          <div className="text-center mb-8 space-y-3">
            <div className="w-14 h-14 bg-cyan-500/10 border border-cyan-500/20 text-cyan-400 rounded-2xl flex items-center justify-center mx-auto shadow-inner">
              <Shield className="w-7 h-7" />
            </div>
            <div>
              <h1 className="text-2xl sm:text-3xl font-extrabold tracking-tight font-serif text-theme-text dark:text-white">
                Admin Control Login
              </h1>
              <p className="text-xs text-theme-sub dark:text-zinc-400 mt-1 leading-relaxed">
                Role-Based Endpoint Access. Role is verified dynamically from the database.
              </p>
            </div>
            <div className="inline-flex items-center gap-1.5 text-[10px] font-mono bg-cyan-500/10 text-cyan-400 border border-cyan-500/30 px-3 py-1 rounded-full font-bold">
              <span className="w-1.5 h-1.5 rounded-full bg-cyan-400 animate-pulse" />
              RESTRICTED: ADMIN ROLE REQUIRED
            </div>
          </div>

          {/* Alerts */}
          {errorMsg && (
            <motion.div
              initial={{ opacity: 0, height: 0 }}
              animate={{ opacity: 1, height: "auto" }}
              className="mb-6 p-4 rounded-2xl bg-rose-500/10 border border-rose-500/20 flex items-start gap-3 text-xs text-rose-400"
            >
              <AlertCircle className="w-4 h-4 shrink-0 mt-0.5" />
              <span>{errorMsg}</span>
            </motion.div>
          )}

          {successMsg && (
            <motion.div
              initial={{ opacity: 0, height: 0 }}
              animate={{ opacity: 1, height: "auto" }}
              className="mb-6 p-4 rounded-2xl bg-emerald-500/10 border border-emerald-500/20 flex items-start gap-3 text-xs text-emerald-400"
            >
              <CheckCircle2 className="w-4 h-4 shrink-0 mt-0.5" />
              <span>{successMsg}</span>
            </motion.div>
          )}

          {/* Form */}
          <form onSubmit={handleSubmit} className="space-y-5 bg-white dark:bg-black">
            <div className="space-y-1.5">
              <label className="text-xs font-bold text-black dark:text-white font-mono flex items-center gap-1.5">
                <Mail className="w-3.5 h-3.5 text-cyan-400" />
                Admin Email Address
              </label>
              <input
                type="email"
                required
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                placeholder="name@company.com"
                className="w-full px-4 py-3 bg-white dark:bg-black border border-slate-300 dark:border-white/20 rounded-2xl text-sm text-black dark:text-white placeholder:text-slate-400 dark:placeholder:text-zinc-500 focus:outline-none focus:border-cyan-400/80 transition"
              />
            </div>

            <div className="space-y-1.5">
              <label className="text-xs font-bold text-black dark:text-white font-mono flex items-center gap-1.5">
                <Lock className="w-3.5 h-3.5 text-cyan-400" />
                Password
              </label>
              <input
                type="password"
                required
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder="••••••••"
                className="w-full px-4 py-3 bg-white dark:bg-black border border-slate-300 dark:border-white/20 rounded-2xl text-sm text-black dark:text-white placeholder:text-slate-400 dark:placeholder:text-zinc-500 focus:outline-none focus:border-cyan-400/80 transition"
              />
            </div>

            <button
              type="submit"
              disabled={adminLoginMutation.isPending}
              className="w-full mt-2 py-3.5 px-6 rounded-2xl bg-white dark:bg-black hover:bg-zinc-100 dark:hover:bg-zinc-900 border border-slate-300 dark:border-white/20 text-black dark:text-white font-bold text-sm flex items-center justify-center gap-2 transition-all shadow-xl active:scale-95 cursor-pointer disabled:opacity-50"
            >
              {adminLoginMutation.isPending ? (
                <span>Authenticating with DB...</span>
              ) : (
                <>
                  <span>Sign In as Administrator</span>
                  <ArrowRight className="w-4 h-4" />
                </>
              )}
            </button>
          </form>

          {/* Footer note */}
          <div className="mt-8 pt-6 border-t border-theme-border/50 text-center">
            <p className="text-xs text-theme-sub">
              Need normal user access?{" "}
              <button
                onClick={() => router.push("/auth")}
                className="text-cyan-400 hover:underline font-bold"
              >
                Sign In as Client
              </button>
            </p>
          </div>
        </div>
      </motion.div>
    </div>
  );
}
