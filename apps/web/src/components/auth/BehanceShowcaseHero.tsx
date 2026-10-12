"use client";

import React, { useState } from "react";
import { motion } from "framer-motion";
import { LOTTIE_PRESETS, LottiePresetId } from "./LottieAuthBackground";
import { SmoothVoiceWave } from "./SmoothVoiceWave";
import {
  Mic,
  Sparkles,
  Building,
  ShieldCheck,
  Globe2,
  CheckCircle2,
  TrendingUp,
  Bot,
} from "lucide-react";

interface BehanceShowcaseHeroProps {
  activePreset: LottiePresetId;
  onSelectPreset: (preset: LottiePresetId) => void;
}

export function BehanceShowcaseHero({
  activePreset,
  onSelectPreset,
}: BehanceShowcaseHeroProps) {
  const [activeTab, setActiveTab] = useState<"voice" | "search" | "contracts">("voice");

  return (
    <div className="flex flex-col justify-between p-6 lg:p-12 xl:p-16 h-full min-h-[600px] text-theme-text transition-colors duration-300">
      {/* Top Branding & Badge */}
      <div>
        <div className="flex items-center gap-3 mb-6">
          <div className="flex items-center gap-2 rounded-full border border-theme-border dark:border-white/15 bg-theme-card px-3.5 py-1.5 backdrop-blur-md shadow-sm dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.1)]">
            <Sparkles className="h-4 w-4 text-theme-accent animate-pulse" />
            <span className="text-xs font-bold tracking-wide text-theme-text uppercase">
              dwin AI Platform
            </span>
          </div>
          <div className="hidden sm:flex items-center gap-2 rounded-full border border-theme-border dark:border-white/15 bg-theme-card px-3.5 py-1.5 backdrop-blur-md shadow-sm dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.1)]">
            <Globe2 className="h-4 w-4 text-theme-sub" />
            <span className="text-xs font-semibold text-theme-sub">English Voice AI</span>
          </div>
        </div>

        <h1 className="font-serif text-4xl lg:text-5xl xl:text-6xl font-normal tracking-tight text-theme-text leading-[1.1]">
          Smart Voice AI <br />
          <span className="font-serif italic text-theme-text">
            Real Estate Discovery
          </span>
        </h1>

        <p className="mt-4 text-base lg:text-lg text-theme-sub max-w-xl font-normal leading-relaxed">
          Explore property listings with voice AI search, viewing scheduling, and instant contract analysis.
        </p>
      </div>

      {/* Feature Preview Cards & Audio Visualizer */}
      <div className="my-8 space-y-6">
        <div className="relative group rounded-3xl border border-theme-border dark:border-white/15 bg-theme-card backdrop-blur-xl p-6 overflow-hidden transition-all duration-500 hover:border-zinc-700 dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.12)]">
          <div className="absolute top-0 right-0 w-48 h-48 bg-cyan-500/10 rounded-full blur-3xl pointer-events-none" />

          {/* Top Card Controls */}
          <div className="flex items-center justify-between mb-4 relative z-10">
            <div className="flex items-center gap-3">
              <div className="w-10 h-10 rounded-2xl bg-white/10 border border-white/20 flex items-center justify-center">
                <Bot className="w-5 h-5 text-cyan-400" />
              </div>
              <div>
                <h3 className="text-sm font-bold text-theme-text">dwin Voice Agent</h3>
                <p className="text-xs font-medium text-theme-sub">Interactive Assistant</p>
              </div>
            </div>

            <div className="flex items-center gap-2 rounded-full bg-emerald-500/10 border border-emerald-500/30 px-3 py-1 text-xs font-semibold text-emerald-400">
              <span className="w-2 h-2 rounded-full bg-emerald-400 animate-pulse" />
              <span>Active</span>
            </div>
          </div>

          {/* Smooth Liquid Voice Wave Animation */}
          <div className="my-4 relative z-10">
            <SmoothVoiceWave isPlaying={true} />
          </div>

          {/* Feature Tabs inside Hero Showcase */}
          <div className="grid grid-cols-3 gap-2 mt-4 relative z-10">
            <button
              onClick={() => setActiveTab("voice")}
              className={`p-3 rounded-2xl text-left border transition-all ${activeTab === "voice"
                  ? "bg-white/10 border-white/30 text-theme-text font-bold shadow-[inset_0_1px_0_0_rgba(255,255,255,0.15)]"
                  : "bg-theme-main border-theme-border text-theme-sub hover:text-theme-text hover:bg-theme-card-hover"
                }`}
            >
              <Mic className="w-4 h-4 text-cyan-400 mb-1" />
              <div className="text-xs font-bold">Voice Booking</div>
              <div className="text-[10px] text-theme-sub">Instant Slots</div>
            </button>
            <button
              onClick={() => setActiveTab("search")}
              className={`p-3 rounded-2xl text-left border transition-all ${activeTab === "search"
                  ? "bg-white/10 border-white/30 text-theme-text font-bold shadow-[inset_0_1px_0_0_rgba(255,255,255,0.15)]"
                  : "bg-theme-main border-theme-border text-theme-sub hover:text-theme-text hover:bg-theme-card-hover"
                }`}
            >
              <Building className="w-4 h-4 text-cyan-400 mb-1" />
              <div className="text-xs font-bold">Smart Search</div>
              <div className="text-[10px] text-theme-sub">Natural Queries</div>
            </button>
            <button
              onClick={() => setActiveTab("contracts")}
              className={`p-3 rounded-2xl text-left border transition-all ${activeTab === "contracts"
                  ? "bg-white/10 border-white/30 text-theme-text font-bold shadow-[inset_0_1px_0_0_rgba(255,255,255,0.15)]"
                  : "bg-theme-main border-theme-border text-theme-sub hover:text-theme-text hover:bg-theme-card-hover"
                }`}
            >
              <ShieldCheck className="w-4 h-4 text-emerald-400 mb-1" />
              <div className="text-xs font-bold">Tenancy Audit</div>
              <div className="text-[10px] text-theme-sub">Contract Analysis</div>
            </button>
          </div>

          {/* Dynamic Tab Context Micro Banner */}
          <div className="mt-3 p-3 rounded-2xl bg-theme-main text-theme-text border border-theme-border text-xs flex items-center justify-between relative z-10 transition-all">
            {activeTab === "voice" && (
              <div className="flex items-center gap-2.5 w-full">
                <span className="w-2 h-2 rounded-full bg-cyan-400 animate-ping shrink-0" />
                <div className="truncate">
                  <span className="text-cyan-400 font-bold">Voice AI: </span>
                  <span className="text-theme-sub">&ldquo;Book viewing slot for Downtown Villa tomorrow at 4 PM&rdquo;</span>
                </div>
              </div>
            )}
            {activeTab === "search" && (
              <div className="flex items-center gap-2.5 w-full">
                <span className="w-2 h-2 rounded-full bg-cyan-400 animate-ping shrink-0" />
                <div className="truncate">
                  <span className="text-cyan-400 font-bold">Smart Search: </span>
                  <span className="text-theme-sub">&ldquo;Sea-view 3-bed apartments under 120k AED/yr with balcony&rdquo;</span>
                </div>
              </div>
            )}
            {activeTab === "contracts" && (
              <div className="flex items-center gap-2.5 w-full">
                <span className="w-2 h-2 rounded-full bg-emerald-400 animate-ping shrink-0" />
                <div className="truncate">
                  <span className="text-emerald-400 font-bold">Tenancy Audit: </span>
                  <span className="text-theme-sub">Clause Compliance Score: 98% (Low Risk)</span>
                </div>
              </div>
            )}
          </div>
        </div>

        {/* Lottie Animation Selector Controls */}
        <div className="rounded-3xl border border-theme-border bg-theme-card backdrop-blur-xl p-4">
          <div className="flex items-center justify-between mb-3 px-1">
            <span className="text-xs font-bold text-theme-text uppercase tracking-wider flex items-center gap-2">
              <Sparkles className="w-3.5 h-3.5 text-cyan-400" />
              Interactive Visual Themes
            </span>
            <span className="text-[10px] text-cyan-400 font-mono font-bold">dwin Engine</span>
          </div>

          <div className="grid grid-cols-2 gap-2">
            {LOTTIE_PRESETS.map((preset) => {
              const Icon = preset.icon;
              const isSelected = activePreset === preset.id;
              return (
                <button
                  key={preset.id}
                  onClick={() => onSelectPreset(preset.id)}
                  className={`flex items-center gap-2.5 p-2.5 rounded-2xl border text-left transition-all ${isSelected
                      ? `bg-white text-black border-white/40 font-bold shadow-md`
                      : "bg-theme-main border-theme-border text-theme-sub hover:text-theme-text hover:bg-theme-card-hover"
                    }`}
                >
                  <div className={`p-1.5 rounded-xl ${isSelected ? "bg-black/10" : "bg-theme-card"}`}>
                    <Icon className={`w-4 h-4 ${isSelected ? "text-black" : "text-cyan-400"}`} />
                  </div>
                  <div className="overflow-hidden">
                    <div className="text-xs font-bold truncate">{preset.name}</div>
                    <div className="text-[10px] opacity-80 truncate">{preset.category}</div>
                  </div>
                </button>
              );
            })}
          </div>
        </div>
      </div>

      {/* Hero Footer Metrics */}
      <div className="flex items-center justify-between text-xs text-theme-sub pt-4 border-t border-theme-border font-medium">
        <div className="flex items-center gap-2">
          <CheckCircle2 className="w-4 h-4 text-emerald-500" />
          <span>Verified Property Records</span>
        </div>
        <div className="flex items-center gap-2">
          <TrendingUp className="w-4 h-4 text-cyan-400" />
          <span>Real-time AI Match</span>
        </div>
      </div>
    </div>
  );
}
