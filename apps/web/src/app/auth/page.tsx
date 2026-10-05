"use client";

import React, { useState } from "react";
import { useRouter } from "next/navigation";
import { motion } from "framer-motion";
import { useGoogleLogin } from "../../lib/api";
import { Building, Sparkles, Key, Mail, Lock } from "lucide-react";

export default function AuthPage() {
  const [isLogin, setIsLogin] = useState(true);
  const router = useRouter();
  const googleLoginMutation = useGoogleLogin();

  const handleAuth = (e: React.FormEvent) => {
    e.preventDefault();
    googleLoginMutation.mutate(
      {
        idToken: "mock-google-id-token-123",
        email: "user@estate.app",
        name: "John Doe",
      },
      {
        onSuccess: () => {
          router.push("/dashboard");
        },
      }
    );
  };

  return (
    <div className="relative min-h-screen bg-[#F8F9FA] flex items-center justify-center overflow-hidden font-sans text-slate-800">
      {/* Background Animated Cartoon/SVG Elements */}
      <div className="absolute inset-0 pointer-events-none overflow-hidden">
        {/* Soft Background Gradient */}
        <div className="absolute top-[-20%] left-[-10%] w-[70%] h-[70%] bg-gradient-to-br from-indigo-100 to-blue-50 rounded-full blur-3xl opacity-60 mix-blend-multiply" />
        <div className="absolute bottom-[-10%] right-[-10%] w-[60%] h-[60%] bg-gradient-to-tl from-purple-100 to-pink-50 rounded-full blur-3xl opacity-60 mix-blend-multiply" />

        {/* Floating Shapes */}
        <motion.div
          animate={{ y: [0, -20, 0], rotate: [0, 10, 0] }}
          transition={{ duration: 6, repeat: Infinity, ease: "easeInOut" }}
          className="absolute top-[15%] left-[10%] opacity-20 text-indigo-400"
        >
          <Building size={120} strokeWidth={1} />
        </motion.div>
        <motion.div
          animate={{ y: [0, 30, 0], x: [0, -20, 0], rotate: [0, -15, 0] }}
          transition={{ duration: 8, repeat: Infinity, ease: "easeInOut" }}
          className="absolute bottom-[20%] right-[15%] opacity-20 text-purple-400"
        >
          <Sparkles size={100} strokeWidth={1} />
        </motion.div>
        <motion.div
          animate={{ scale: [1, 1.1, 1], rotate: [0, 180, 360] }}
          transition={{ duration: 25, repeat: Infinity, ease: "linear" }}
          className="absolute top-[20%] right-[20%] opacity-10 text-rose-400"
        >
          <Key size={80} strokeWidth={1.5} />
        </motion.div>
      </div>

      {/* Auth Card */}
      <motion.div
        initial={{ opacity: 0, scale: 0.95, y: 20 }}
        animate={{ opacity: 1, scale: 1, y: 0 }}
        transition={{ duration: 0.5, ease: "easeOut" }}
        className="relative z-10 w-full max-w-md"
      >
        <div className="bg-white/80 backdrop-blur-xl shadow-[0_20px_60px_-15px_rgba(0,0,0,0.05)] border border-white rounded-[2rem] p-8 md:p-10">
          <div className="text-center mb-8">
            <div className="w-14 h-14 bg-gradient-to-tr from-indigo-500 to-blue-500 rounded-2xl mx-auto flex items-center justify-center shadow-lg shadow-indigo-500/30 mb-4 transform -rotate-6">
              <span className="text-white font-extrabold text-2xl rotate-6">EA</span>
            </div>
            <h1 className="text-2xl font-extrabold text-slate-800 tracking-tight">
              {isLogin ? "Welcome back" : "Create an account"}
            </h1>
            <p className="text-sm text-slate-500 mt-2">
              {isLogin ? "Enter your details to access your dashboard" : "Sign up to start finding your dream home"}
            </p>
          </div>

          <form onSubmit={handleAuth} className="space-y-4">
            <div className="space-y-1">
              <label className="text-xs font-semibold text-slate-600 uppercase tracking-wider pl-1">Email</label>
              <div className="relative">
                <Mail className="absolute left-4 top-1/2 -translate-y-1/2 w-5 h-5 text-slate-400" />
                <input
                  type="email"
                  placeholder="name@example.com"
                  className="w-full bg-slate-50 border border-slate-200 text-slate-800 rounded-xl pl-12 pr-4 py-3.5 focus:outline-none focus:ring-2 focus:ring-indigo-500/20 focus:border-indigo-500 transition-all font-medium placeholder:font-normal placeholder:text-slate-400"
                  required
                />
              </div>
            </div>

            <div className="space-y-1">
              <label className="text-xs font-semibold text-slate-600 uppercase tracking-wider pl-1">Password</label>
              <div className="relative">
                <Lock className="absolute left-4 top-1/2 -translate-y-1/2 w-5 h-5 text-slate-400" />
                <input
                  type="password"
                  placeholder="••••••••"
                  className="w-full bg-slate-50 border border-slate-200 text-slate-800 rounded-xl pl-12 pr-4 py-3.5 focus:outline-none focus:ring-2 focus:ring-indigo-500/20 focus:border-indigo-500 transition-all font-medium placeholder:font-normal placeholder:text-slate-400"
                  required
                />
              </div>
            </div>

            {isLogin && (
              <div className="flex justify-end pt-1">
                <a href="#" className="text-xs font-semibold text-indigo-600 hover:text-indigo-700 transition-colors">
                  Forgot password?
                </a>
              </div>
            )}

            <button
              type="submit"
              disabled={googleLoginMutation.isPending}
              className="w-full bg-slate-800 hover:bg-slate-900 text-white font-bold py-3.5 rounded-xl shadow-lg shadow-slate-900/20 transition-all active:scale-[0.98] disabled:opacity-70 mt-4 flex items-center justify-center"
            >
              {googleLoginMutation.isPending ? (
                <div className="w-5 h-5 border-2 border-white/30 border-t-white rounded-full animate-spin" />
              ) : (
                isLogin ? "Sign In" : "Sign Up"
              )}
            </button>
          </form>

          <div className="mt-8 flex items-center justify-center gap-2 text-sm">
            <span className="text-slate-500">{isLogin ? "Don't have an account?" : "Already have an account?"}</span>
            <button
              type="button"
              onClick={() => setIsLogin(!isLogin)}
              className="font-bold text-indigo-600 hover:text-indigo-700 transition-colors focus:outline-none"
            >
              {isLogin ? "Sign up" : "Log in"}
            </button>
          </div>
        </div>
      </motion.div>
    </div>
  );
}
