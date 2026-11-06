"use client";

import React, { useState } from "react";
import { useRouter } from "next/navigation";
import { motion } from "framer-motion";
import { Shield, Lock, Mail, ArrowRight, AlertCircle, ArrowLeft, CheckCircle2 } from "lucide-react";
import { useAdminLogin } from "../../../lib/api";
import { AuthSplitLayout } from "../../../components/auth/AuthSplitLayout";
import { adminAuthShowcase } from "../../../lib/auth-showcase";

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
    <AuthSplitLayout
      content={adminAuthShowcase}
      topSlot={
        <button
          onClick={() => router.push("/")}
          className="inline-flex items-center gap-2 text-xs text-theme-sub hover:text-theme-text transition bg-theme-card px-4 py-2 rounded-full border border-theme-border"
        >
          <ArrowLeft className="w-3.5 h-3.5" />
          Back to home
        </button>
      }
    >
      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.4 }}
        className="w-full relative z-10"
      >
        <div className="auth-card p-8 sm:p-10 text-theme-text relative overflow-hidden">
          {/* Top Specular Gradient Line */}
          <div className="absolute top-0 left-0 right-0 h-1 bg-gradient-to-r from-cyan-500 via-white to-cyan-400" />

          {/* Header */}
          <div className="text-center mb-8 space-y-3">
            <div className="w-14 h-14 bg-cyan-500/10 border border-cyan-500/20 text-cyan-400 rounded-2xl flex items-center justify-center mx-auto shadow-inner">
              <Shield className="w-7 h-7" />
            </div>
            <div>
              <h1 className="text-2xl sm:text-3xl font-semibold tracking-tight font-display text-theme-text">
                Team sign in
              </h1>
              <p className="text-sm text-theme-sub mt-1 leading-relaxed">
                Access is limited to authorised team members.
              </p>
            </div>
            <div className="inline-flex items-center gap-1.5 text-[11px] bg-cyan-500/10 text-cyan-400 border border-cyan-500/30 px-3 py-1 rounded-full font-semibold">
              <span className="w-1.5 h-1.5 rounded-full bg-cyan-400 animate-pulse" />
              Restricted area
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
          <form onSubmit={handleSubmit} className="space-y-5 bg-transparent">
            <div className="space-y-1.5">
              <label className="text-sm font-medium text-theme-text flex items-center gap-1.5">
                <Mail className="w-3.5 h-3.5 text-cyan-400" />
                Work email
              </label>
              <input
                type="email"
                required
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                placeholder="name@company.com"
                className="w-full px-4 py-3 bg-theme-main border border-theme-border rounded-2xl text-sm text-theme-text placeholder:text-theme-sub focus:outline-none focus:border-cyan-400/80 transition"
              />
            </div>

            <div className="space-y-1.5">
              <label className="text-sm font-medium text-theme-text flex items-center gap-1.5">
                <Lock className="w-3.5 h-3.5 text-cyan-400" />
                Password
              </label>
              <input
                type="password"
                required
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder="••••••••"
                className="w-full px-4 py-3 bg-theme-main border border-theme-border rounded-2xl text-sm text-theme-text placeholder:text-theme-sub focus:outline-none focus:border-cyan-400/80 transition"
              />
            </div>

            <button
              type="submit"
              disabled={adminLoginMutation.isPending}
              className="btn-primary w-full mt-2"
            >
              {adminLoginMutation.isPending ? (
                <span>Signing in…</span>
              ) : (
                <>
                  <span>Sign in</span>
                  <ArrowRight className="w-4 h-4" />
                </>
              )}
            </button>
          </form>

          {/* Footer note */}
          <div className="mt-8 pt-6 border-t border-theme-border/50 text-center">
            <p className="text-xs text-theme-sub">
              Looking for your customer account?{" "}
              <button
                onClick={() => router.push("/auth")}
                className="text-cyan-400 hover:underline font-semibold"
              >
                Customer sign in
              </button>
            </p>
          </div>
        </div>
      </motion.div>
    </AuthSplitLayout>
  );
}
