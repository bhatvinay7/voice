"use client";

import React, { useState } from "react";
import Link from "next/link";
import { Header } from "../components/Header";
import { VoiceAgentOrbsDemo } from "../components/VoiceAgentOrbsDemo";
import { useUserProfile } from "../lib/api";
import { motion } from "framer-motion";
import {
  Mic,
  ArrowRight,
  ShieldCheck,
  CheckCircle2,
  Building2,
  Sparkles,
  FileCheck,
  PhoneCall,
  Compass,
  Cpu,
  Globe2,
  Zap,
} from "lucide-react";

export default function HomePage() {
  const [agentModeActive, setAgentModeActive] = useState(true);
  const { data: userData } = useUserProfile();
  const user = userData?.user || null;

  return (
    <div className="min-h-screen bg-theme-main text-theme-text font-sans antialiased selection:bg-indigo-200 selection:text-indigo-950 overflow-x-hidden transition-colors duration-300">
      {/* Soft Ambient Light Gradient Background */}
      <div className="fixed inset-0 pointer-events-none z-0 overflow-hidden">
        <div className="absolute top-[-10%] left-[50%] -translate-x-1/2 w-[900px] h-[600px] rounded-full bg-gradient-to-tr from-indigo-500/10 via-cyan-500/10 to-transparent blur-[130px]" />
        <div className="absolute bottom-[-10%] right-[-5%] w-[600px] h-[600px] rounded-full bg-gradient-to-br from-cyan-500/10 via-indigo-500/10 to-transparent blur-[140px]" />
      </div>

      <div className="relative z-10 flex flex-col min-h-screen">
        <Header
          user={user}
          agentModeActive={agentModeActive}
          onToggleAgentMode={setAgentModeActive}
        />

        <main className="flex-1 max-w-7xl mx-auto px-4 sm:px-6 md:px-8 py-10 sm:py-16 md:py-24 w-full space-y-12 sm:space-y-16 md:space-y-24">
          
          {/* HERO SECTION */}
          <section className="relative flex flex-col items-center text-center space-y-6 sm:space-y-8 pt-4 sm:pt-8 md:pt-16">
            
            {/* Top Product Badge */}
            <motion.div
              initial={{ opacity: 0, y: 15 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.5 }}
              className="inline-flex items-center gap-2.5 bg-theme-card border border-theme-border dark:border-white/15 px-3.5 sm:px-4 py-1.5 sm:py-2 rounded-full text-xs font-semibold text-theme-text shadow-sm dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.1)]"
            >
              <Sparkles className="w-3.5 h-3.5 sm:w-4 sm:h-4 text-theme-accent animate-pulse" />
              <span>dwin Voice AI Real Estate Assistant</span>
            </motion.div>

            {/* Headline with Sarvam & x.ai Style Serif Typography */}
            <div className="max-w-4xl space-y-4 sm:space-y-6 px-2">
              <motion.h1
                initial={{ opacity: 0, y: 20 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ duration: 0.6, delay: 0.1 }}
                className="font-serif text-4xl sm:text-6xl lg:text-8xl font-normal tracking-tight leading-[1.08] text-theme-text"
              >
                Find & Book <br />
                <span className="font-serif italic text-theme-text">
                  Homes by Voice AI.
                </span>
              </motion.h1>

              <motion.p
                initial={{ opacity: 0, y: 20 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ duration: 0.6, delay: 0.2 }}
                className="text-sm sm:text-lg md:text-xl text-theme-sub max-w-2xl mx-auto font-normal leading-relaxed"
              >
                Build, tune, and deploy intelligent voice agents that carry human-like real estate conversations at ultra-low latency.
              </motion.p>
            </div>

            {/* CTA Action Buttons */}
            <motion.div
              initial={{ opacity: 0, y: 20 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.6, delay: 0.3 }}
              className="flex flex-col sm:flex-row items-center justify-center w-full sm:w-auto gap-3.5 pt-2 sm:pt-4 px-4"
            >
              <Link
                href="/dashboard"
                className="w-full sm:w-auto group relative inline-flex items-center justify-center gap-3 px-8 py-3.5 sm:py-4 font-bold text-sm tracking-wide text-theme-text bg-theme-card hover:bg-black/10 dark:hover:bg-white/10 border border-theme-border dark:border-white/20 rounded-2xl shadow-lg transition-all hover:scale-105 active:scale-95"
              >
                <Mic className="w-5 h-5 text-cyan-400 group-hover:scale-110 transition-transform" />
                <span>Start Voice Search</span>
                <ArrowRight className="w-4 h-4 text-theme-text group-hover:translate-x-1 transition-transform" />
              </Link>

              <Link
                href="/auth"
                className="w-full sm:w-auto inline-flex items-center justify-center gap-2 px-8 py-3.5 sm:py-4 font-bold text-sm tracking-wide text-theme-text bg-theme-card hover:bg-black/10 dark:hover:bg-white/10 border border-theme-border dark:border-white/15 rounded-2xl shadow-sm transition-all dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.1)]"
              >
                <Building2 className="w-4 h-4 text-theme-accent" />
                <span>Sign In / Register</span>
              </Link>
            </motion.div>

            {/* Quick Metrics Bar */}
            <div className="grid grid-cols-2 md:grid-cols-4 gap-3 sm:gap-4 w-full max-w-4xl pt-4 sm:pt-8">
              <div className="bg-theme-card border border-theme-border dark:border-white/15 rounded-2xl p-4 sm:p-5 shadow-md text-center dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.08)]">
                <div className="font-serif text-2xl sm:text-3xl font-normal text-theme-accent">10,000+</div>
                <div className="text-[11px] sm:text-xs font-semibold text-theme-sub mt-1">Verified Listings</div>
              </div>
              <div className="bg-theme-card border border-theme-border dark:border-white/15 rounded-2xl p-4 sm:p-5 shadow-md text-center dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.08)]">
                <div className="font-serif text-2xl sm:text-3xl font-normal text-theme-accent">24/7</div>
                <div className="text-[11px] sm:text-xs font-semibold text-theme-sub mt-1">Viewing Assistant</div>
              </div>
              <div className="bg-theme-card border border-theme-border dark:border-white/15 rounded-2xl p-4 sm:p-5 shadow-md text-center dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.08)]">
                <div className="font-serif text-2xl sm:text-3xl font-normal text-theme-accent">Bilingual</div>
                <div className="text-[11px] sm:text-xs font-semibold text-theme-sub mt-1">Voice AI Intelligence</div>
              </div>
              <div className="bg-theme-card border border-theme-border dark:border-white/15 rounded-2xl p-4 sm:p-5 shadow-md text-center dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.08)]">
                <div className="font-serif text-2xl sm:text-3xl font-normal text-theme-accent">Instant</div>
                <div className="text-[11px] sm:text-xs font-semibold text-theme-sub mt-1">Contract Analysis</div>
              </div>
            </div>

            {/* Platform Tech Stack Trust Bar */}
            <div className="pt-6 sm:pt-10 flex flex-wrap items-center justify-center gap-4 sm:gap-8 text-[11px] sm:text-xs font-mono text-theme-sub opacity-90">
              <span className="flex items-center gap-1.5"><Zap className="w-3.5 h-3.5 text-cyan-400" /> WebRTC Audio Stream</span>
              <span className="flex items-center gap-1.5"><Cpu className="w-3.5 h-3.5 text-indigo-400" /> PostgreSQL Vector RAG</span>
              <span className="flex items-center gap-1.5"><Globe2 className="w-3.5 h-3.5 text-cyan-400" /> Bilingual Neural Engine</span>
              <span className="flex items-center gap-1.5"><ShieldCheck className="w-3.5 h-3.5 text-emerald-400" /> &lt;180ms Latency</span>
            </div>
          </section>

          {/* INTERACTIVE VOICE AGENT ORBS DEMO */}
          <VoiceAgentOrbsDemo />

          {/* HOW IT WORKS (Vaani AI & Observe.ai Card Design) */}
          <section className="space-y-8 sm:space-y-12">
            <div className="text-center space-y-2 sm:space-y-3">
              <span className="text-xs font-bold text-cyan-400 uppercase tracking-widest bg-cyan-500/10 px-3.5 py-1.5 rounded-full border border-cyan-500/20 shadow-[inset_0_1px_0_0_rgba(255,255,255,0.08)]">
                Smart Property Discovery
              </span>
              <h2 className="font-serif text-3xl sm:text-4xl md:text-5xl font-normal text-theme-text tracking-tight">
                How dwin Works For You
              </h2>
            </div>

            <div className="grid grid-cols-1 md:grid-cols-3 gap-6 sm:gap-8">
              
              {/* Step 1 - Vaani AI Style */}
              <motion.div
                whileHover={{ y: -6 }}
                className="bg-theme-card border border-theme-border dark:border-white/15 rounded-[2.2rem] p-7 sm:p-8 shadow-xl transition-all flex flex-col justify-between space-y-6 relative overflow-hidden group hover:border-white/40 dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.12)]"
              >
                <div className="space-y-4">
                  <div className="w-12 h-12 rounded-2xl bg-theme-main border border-theme-border dark:border-white/10 flex items-center justify-center text-theme-text shadow-sm dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.06)]">
                    <Mic className="w-6 h-6 text-cyan-400" />
                  </div>
                  <span className="text-xs font-bold text-cyan-400 tracking-wider uppercase block font-mono">Step 01</span>
                  <h3 className="font-sans text-xl sm:text-2xl font-bold text-theme-text tracking-tight">Speak Your Search</h3>
                  <p className="text-sm text-theme-sub leading-relaxed font-normal">
                    &ldquo;Find me a 2-bedroom pet-friendly apartment near Marina under 120k AED per year.&rdquo;
                  </p>
                </div>
                <div className="pt-4 border-t border-theme-border dark:border-white/10 flex items-center gap-2 text-xs font-semibold text-cyan-400 font-mono">
                  <Compass className="w-4 h-4" />
                  <span>Voice Query Processing</span>
                </div>
              </motion.div>

              {/* Step 2 - Vaani AI Style */}
              <motion.div
                whileHover={{ y: -6 }}
                className="bg-theme-card border border-theme-border dark:border-white/15 rounded-[2.2rem] p-7 sm:p-8 shadow-xl transition-all flex flex-col justify-between space-y-6 relative overflow-hidden group hover:border-white/40 dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.12)]"
              >
                <div className="space-y-4">
                  <div className="w-12 h-12 rounded-2xl bg-theme-main border border-theme-border dark:border-white/10 flex items-center justify-center text-emerald-500 shadow-sm dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.06)]">
                    <PhoneCall className="w-6 h-6" />
                  </div>
                  <span className="text-xs font-bold text-emerald-500 tracking-wider uppercase block font-mono">Step 02</span>
                  <h3 className="font-sans text-xl sm:text-2xl font-bold text-theme-text tracking-tight">Instant Viewing Booking</h3>
                  <p className="text-sm text-theme-sub leading-relaxed font-normal">
                    Book property viewing appointments instantly using automated voice scheduling and slot locking.
                  </p>
                </div>
                <div className="pt-4 border-t border-theme-border dark:border-white/10 flex items-center gap-2 text-xs font-semibold text-emerald-500 font-mono">
                  <CheckCircle2 className="w-4 h-4" />
                  <span>Real-Time Slot Reservation</span>
                </div>
              </motion.div>

              {/* Step 3 - Observe.ai Style */}
              <motion.div
                whileHover={{ y: -6 }}
                className="bg-theme-card border border-theme-border dark:border-white/15 rounded-[2.2rem] p-7 sm:p-8 shadow-xl transition-all flex flex-col justify-between space-y-6 relative overflow-hidden group hover:border-white/40 dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.12)]"
              >
                <div className="space-y-4">
                  <div className="w-12 h-12 rounded-2xl bg-theme-main border border-theme-border dark:border-white/10 flex items-center justify-center text-cyan-500 shadow-sm dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.06)]">
                    <FileCheck className="w-6 h-6" />
                  </div>
                  <span className="text-xs font-bold text-cyan-500 tracking-wider uppercase block font-mono">Step 03</span>
                  <h3 className="font-sans text-xl sm:text-2xl font-bold text-theme-text tracking-tight">Smart Contract Analysis</h3>
                  <p className="text-sm text-theme-sub leading-relaxed font-normal">
                    Upload tenancy agreements for instant clause auditing and automated contract verification.
                  </p>
                </div>
                <div className="pt-4 border-t border-theme-border dark:border-white/10 flex items-center gap-2 text-xs font-semibold text-cyan-500 font-mono">
                  <ShieldCheck className="w-4 h-4" />
                  <span>Automated Legal Safety</span>
                </div>
              </motion.div>

            </div>
          </section>

          {/* CALL TO ACTION */}
          <section className="rounded-3xl border border-theme-border dark:border-white/15 bg-theme-card p-6 sm:p-10 md:p-14 shadow-xl text-center space-y-4 sm:space-y-6 relative overflow-hidden dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.12)]">
            <div className="absolute top-0 right-0 w-96 h-96 bg-indigo-500/10 rounded-full blur-3xl pointer-events-none" />
            <h2 className="font-serif text-3xl sm:text-4xl md:text-6xl font-normal text-theme-text tracking-tight">
              Ready to Discover Properties?
            </h2>
            <p className="text-xs sm:text-sm md:text-base text-theme-sub max-w-xl mx-auto font-normal">
              Start searching properties with voice AI intelligence.
            </p>
            <div className="flex justify-center pt-2">
              <Link
                href="/auth"
                className="w-full sm:w-auto px-8 py-3.5 bg-theme-main hover:bg-black/10 dark:hover:bg-white/10 text-theme-text border border-theme-border dark:border-white/20 font-bold text-sm rounded-2xl shadow-md transition-all hover:scale-105 active:scale-95"
              >
                Get Started Free
              </Link>
            </div>
          </section>

        </main>

        {/* ELEVATED FOOTER WITH TOP SHADOW & BACKGROUND WATERMARK */}
        <footer className="relative bg-theme-card text-theme-sub pt-10 sm:pt-16 pb-8 sm:pb-12 px-4 sm:px-6 overflow-hidden shadow-2xl border-t border-theme-border transition-colors duration-300">
          {/* Top Subtle Indigo Glow Accent Line */}
          <div className="absolute top-0 left-0 right-0 h-[1px] bg-gradient-to-r from-transparent via-indigo-500/30 to-transparent" />

          {/* Background Watermark Text */}
          <div className="absolute bottom-[-10px] left-1/2 -translate-x-1/2 pointer-events-none select-none font-serif font-normal text-[6rem] sm:text-[12rem] md:text-[18rem] tracking-tighter text-slate-500/10 leading-none z-0">
            dwin
          </div>

          <div className="relative z-10 max-w-7xl mx-auto flex flex-col md:flex-row items-center justify-between gap-6 sm:gap-8 pb-8 sm:pb-12 border-b border-theme-border">
            <div className="flex items-center space-x-3">
              <div className="w-9 h-9 sm:w-10 sm:h-10 rounded-xl bg-theme-main border border-theme-border flex items-center justify-center font-black text-lg sm:text-xl shadow-md text-theme-text">
                dw
              </div>
              <div>
                <span className="font-serif font-normal text-xl sm:text-2xl text-theme-text tracking-tight">dwin</span>
                <p className="text-[11px] sm:text-xs text-theme-sub font-medium">Smart Voice AI Real Estate</p>
              </div>
            </div>

            <div className="flex items-center gap-6 text-xs font-semibold text-theme-sub">
              <Link href="/" className="hover:text-theme-accent transition-colors">Home</Link>
              <Link href="/dashboard" className="hover:text-theme-accent transition-colors">Dashboard</Link>
              <Link href="/auth" className="hover:text-theme-accent transition-colors">Sign In</Link>
            </div>
          </div>

          <div className="relative z-10 max-w-7xl mx-auto pt-6 sm:pt-8 flex flex-col sm:flex-row items-center justify-between text-xs text-theme-sub gap-3 text-center sm:text-left">
            <p>© 2026 dwin — Real Estate Voice AI Platform.</p>
            <div className="flex items-center gap-2">
              <span className="w-2 h-2 rounded-full bg-emerald-500 animate-pulse" />
              <span>dwin Neural Engine Active</span>
            </div>
          </div>
        </footer>

      </div>
    </div>
  );
}
