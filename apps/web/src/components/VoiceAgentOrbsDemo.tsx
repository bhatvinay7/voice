"use client";

import React, { useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { Play, Pause, Sparkles, Mic, MessageSquare, Bot, ShieldCheck } from "lucide-react";

interface AgentOrb {
  id: string;
  name: string;
  role: string;
  voiceName: string;
  color: string;
  gradient: string;
  shadow: string;
  size: number;
  transcript: string;
}

const AGENT_ORBS: AgentOrb[] = [
  {
    id: "agent-1",
    name: "Viewing Coordinator",
    role: "Property Viewing Assistant",
    voiceName: "Eve",
    color: "from-indigo-500 via-blue-500 to-cyan-400",
    gradient: "radial-gradient(circle at 35% 35%, rgba(99, 102, 241, 0.9), rgba(59, 130, 246, 0.7), rgba(6, 182, 212, 0.8))",
    shadow: "shadow-[0_0_50px_rgba(99,102,241,0.35)]",
    size: 160,
    transcript: "I've checked the schedule for Marina Heights. We have a viewing slot open tomorrow at 3:00 PM. Would that work for you?",
  },
  {
    id: "agent-2",
    name: "Property Matcher",
    role: "Voice Search Agent",
    voiceName: "Aria",
    color: "from-blue-500 via-cyan-400 to-emerald-400",
    gradient: "radial-gradient(circle at 35% 35%, rgba(59, 130, 246, 0.9), rgba(6, 182, 212, 0.7), rgba(16, 185, 129, 0.8))",
    shadow: "shadow-[0_0_45px_rgba(59,130,246,0.35)]",
    size: 135,
    transcript: "Found 3 pet-friendly 2-bedroom luxury apartments near Skyline District matching your $120,000 budget.",
  },
  {
    id: "agent-3",
    name: "Contract Advisor",
    role: "Tenancy Audit Agent",
    voiceName: "Zenith",
    color: "from-purple-500 via-indigo-500 to-blue-500",
    gradient: "radial-gradient(circle at 35% 35%, rgba(168, 85, 247, 0.9), rgba(99, 102, 241, 0.7), rgba(59, 130, 246, 0.8))",
    shadow: "shadow-[0_0_40px_rgba(168,85,247,0.35)]",
    size: 120,
    transcript: "Your tenancy agreement clauses meet standard legal compliance with zero hidden maintenance penalties.",
  },
];

export function VoiceAgentOrbsDemo() {
  const [activeOrbId, setActiveOrbId] = useState<string | null>("agent-1");
  const [isPlaying, setIsPlaying] = useState<boolean>(true);

  const activeOrb = AGENT_ORBS.find((o) => o.id === activeOrbId) || AGENT_ORBS[0];

  const toggleOrbPlay = (id: string) => {
    if (activeOrbId === id) {
      setIsPlaying(!isPlaying);
    } else {
      setActiveOrbId(id);
      setIsPlaying(true);
    }
  };

  return (
    <div className="w-full max-w-6xl mx-auto my-6 sm:my-12 space-y-8">
      
      {/* Interactive Orbs Stage Card */}
      <div className="p-6 sm:p-10 rounded-[2.2rem] bg-theme-card border border-theme-border dark:border-white/15 shadow-xl backdrop-blur-xl relative overflow-hidden transition-colors duration-300 dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.12)]">
        {/* Top Header Badge */}
        <div className="flex flex-col md:flex-row items-start md:items-center justify-between gap-4 border-b border-theme-border dark:border-white/10 pb-6 mb-8">
          <div>
            <div className="inline-flex items-center gap-2 px-3.5 py-1.5 rounded-full bg-cyan-500/10 border border-cyan-500/20 text-cyan-400 text-xs font-bold mb-3 shadow-[inset_0_1px_0_0_rgba(255,255,255,0.08)]">
              <Sparkles className="w-3.5 h-3.5 text-cyan-400 animate-pulse" />
              <span>Interactive Voice Agent Orbs</span>
            </div>
            <h2 className="font-serif text-3xl sm:text-4xl md:text-5xl font-normal text-theme-text tracking-tight">
              Human-like Voice AI
            </h2>
            <p className="text-theme-sub text-sm mt-2 max-w-xl font-normal leading-relaxed">
              Natural voice timing, sub-180ms neural latency, and intelligent turn-taking across Arabic and English.
            </p>
          </div>

          <div className="flex items-center gap-3">
            <div className="flex items-center gap-2 px-4 py-2 rounded-2xl bg-theme-main border border-theme-border dark:border-white/15 text-xs font-mono font-bold text-theme-text shadow-sm dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.08)]">
              <span className="w-2 h-2 rounded-full bg-emerald-400 animate-pulse" />
              <span>Latency: &lt;180ms TTFT</span>
            </div>
          </div>
        </div>

        {/* Main Orbs Stage Grid */}
        <div className="grid grid-cols-1 lg:grid-cols-12 gap-8 items-center">
          
          {/* Left Side: Orbs Display */}
          <div className="lg:col-span-7 flex flex-wrap sm:flex-nowrap items-center justify-center gap-6 relative py-6">
            {AGENT_ORBS.map((orb, index) => {
              const isActive = activeOrbId === orb.id;
              return (
                <div key={orb.id} className="relative flex flex-col items-center">
                  {/* Active Speech Bubble */}
                  <AnimatePresence>
                    {isActive && (
                      <motion.div
                        initial={{ opacity: 0, y: 10, scale: 0.9 }}
                        animate={{ opacity: 1, y: 0, scale: 1 }}
                        exit={{ opacity: 0, y: 10, scale: 0.9 }}
                        className="absolute -top-24 z-20 w-64 bg-theme-main text-theme-text p-4 rounded-2xl shadow-2xl text-xs leading-relaxed border border-theme-border dark:border-white/15 pointer-events-none dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.1)]"
                      >
                        <div className="flex items-center gap-1.5 mb-1.5 text-[10px] text-cyan-400 font-mono uppercase tracking-wider font-bold">
                          <MessageSquare className="w-3 h-3 text-cyan-400" />
                          <span>Voice Model: {orb.voiceName}</span>
                        </div>
                        <p className="font-normal text-theme-text">{orb.transcript}</p>
                        <div className="absolute left-1/2 -bottom-2 -translate-x-1/2 w-3.5 h-3.5 bg-theme-main rotate-45 border-r border-b border-theme-border dark:border-white/15" />
                      </motion.div>
                    )}
                  </AnimatePresence>

                  {/* Glowing Animated Orb */}
                  <motion.div
                    animate={{
                      y: isPlaying && isActive ? [0, -8, 0] : [0, -4, 0],
                      scale: isActive ? 1.08 : 0.95,
                    }}
                    transition={{
                      duration: 4 + index * 0.8,
                      repeat: Infinity,
                      ease: "easeInOut",
                    }}
                    onClick={() => toggleOrbPlay(orb.id)}
                    className={`relative cursor-pointer rounded-full p-1 transition-all duration-300 ${orb.shadow}`}
                    style={{
                      width: `${orb.size}px`,
                      height: `${orb.size}px`,
                    }}
                  >
                    {isActive && isPlaying && (
                      <motion.div
                        animate={{ scale: [1, 1.25, 1], opacity: [0.6, 0.1, 0.6] }}
                        transition={{ duration: 2, repeat: Infinity, ease: "easeInOut" }}
                        className="absolute inset-0 rounded-full border border-white/40 pointer-events-none"
                      />
                    )}

                    <div
                      className="w-full h-full rounded-full relative flex items-center justify-center overflow-hidden border border-white/30 shadow-inner"
                      style={{ background: orb.gradient }}
                    >
                      <div className="absolute top-2 left-4 w-1/2 h-1/3 rounded-full bg-gradient-to-b from-white/60 to-transparent blur-[2px]" />

                      <button
                        type="button"
                        aria-label={`Play ${orb.name}`}
                        className="w-11 h-11 rounded-full bg-white/95 backdrop-blur-md text-slate-900 flex items-center justify-center shadow-lg transition-transform hover:scale-110 active:scale-95 z-10"
                      >
                        {isActive && isPlaying ? (
                          <Pause className="w-4 h-4 text-slate-900" />
                        ) : (
                          <Play className="w-4 h-4 text-slate-900 ml-0.5" />
                        )}
                      </button>
                    </div>
                  </motion.div>

                  <div className="mt-3.5 text-center">
                    <h4 className="text-xs font-bold text-theme-text">{orb.name}</h4>
                    <p className="text-[10px] text-theme-sub font-mono mt-0.5">{orb.role}</p>
                  </div>
                </div>
              );
            })}
          </div>

          {/* Right Side: Observe.ai Style Testimonial Card */}
          <div className="lg:col-span-5 bg-theme-main border border-theme-border dark:border-white/15 rounded-[1.8rem] p-6 space-y-5 shadow-lg dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.08)]">
            <div className="flex items-center justify-between border-b border-theme-border dark:border-white/10 pb-4">
              <div className="flex items-center gap-3">
                <div className="w-10 h-10 rounded-2xl bg-theme-card border border-theme-border dark:border-white/15 flex items-center justify-center text-theme-text shadow-sm dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.08)]">
                  <Bot className="w-5 h-5 text-cyan-400" />
                </div>
                <div>
                  <h3 className="text-sm font-bold text-theme-text">{activeOrb.name}</h3>
                  <p className="text-xs text-theme-sub">Voice Model: {activeOrb.voiceName}</p>
                </div>
              </div>

              <div className="flex items-center gap-1.5 px-3 py-1 rounded-full bg-emerald-500/10 border border-emerald-500/20 text-emerald-400 text-[11px] font-bold font-mono shadow-[inset_0_1px_0_0_rgba(255,255,255,0.08)]">
                <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" />
                <span>ACTIVE</span>
              </div>
            </div>

            {/* Observe.ai Styled Quote Box */}
            <div className="space-y-2">
              <span className="text-[10px] font-bold text-theme-sub uppercase tracking-wider block font-mono">
                Live Speech Output
              </span>
              <div className="bg-theme-card border border-theme-border dark:border-white/15 rounded-2xl p-4 text-xs sm:text-sm text-theme-text leading-relaxed font-normal shadow-sm dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.08)]">
                &ldquo;{activeOrb.transcript}&rdquo;
              </div>
            </div>

            {/* Author / Agent Profile Footer (Observe.ai style) */}
            <div className="flex items-center gap-3 pt-2 border-t border-theme-border dark:border-white/10">
              <div className="w-10 h-10 rounded-full bg-white/10 border border-white/20 text-theme-text font-bold flex items-center justify-center text-xs shrink-0 dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.1)]">
                {activeOrb.voiceName.charAt(0)}
              </div>
              <div>
                <p className="text-xs font-bold text-theme-text">{activeOrb.name}</p>
                <p className="text-[11px] text-theme-sub font-mono">{activeOrb.role}</p>
              </div>
            </div>
          </div>

        </div>
      </div>

      {/* Vaani AI & Observe.ai Style Feature Cards Grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        
        {/* Vaani AI Style Card 1 */}
        <div className="bg-theme-card border border-theme-border dark:border-white/15 rounded-[2.2rem] p-7 sm:p-8 space-y-4 shadow-xl transition-all hover:border-white/40 dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.12)]">
          <div className="w-12 h-12 rounded-2xl bg-theme-main border border-theme-border dark:border-white/15 flex items-center justify-center text-2xl shadow-sm dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.08)]">
            🧱
          </div>
          <h3 className="text-xl sm:text-2xl font-bold font-sans text-theme-text tracking-tight">
            A different foundation
          </h3>
          <p className="text-sm sm:text-base text-theme-sub font-normal leading-relaxed">
            Our Latent Speech Model learns meaning in a continuous latent space &mdash; the shape of speech itself, not just its transcript.
          </p>
        </div>

        {/* Vaani AI Style Card 2 */}
        <div className="bg-theme-card border border-theme-border dark:border-white/15 rounded-[2.2rem] p-7 sm:p-8 space-y-4 shadow-xl transition-all hover:border-white/40 dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.12)]">
          <div className="w-12 h-12 rounded-2xl bg-theme-main border border-theme-border dark:border-white/15 flex items-center justify-center text-2xl shadow-sm dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.08)]">
            🎛️
          </div>
          <h3 className="text-xl sm:text-2xl font-bold font-sans text-theme-text tracking-tight">
            Built to expand
          </h3>
          <p className="text-sm sm:text-base text-theme-sub font-normal leading-relaxed">
            One foundation for any downstream speech tasks like Text-to-Speech, Speech-to-Text, Speech Translation, and Speech-to-Speech. Each ability is a decoder off the same latent core.
          </p>
        </div>

      </div>

    </div>
  );
}
