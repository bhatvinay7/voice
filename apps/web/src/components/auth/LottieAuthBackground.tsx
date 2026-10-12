"use client";

import React, { useState, useEffect } from "react";
import { motion } from "framer-motion";
import { DotLottieReact } from "@lottiefiles/dotlottie-react";
import { Sparkles, ShieldCheck, Cpu, Building2 } from "lucide-react";

export type LottiePresetId = "smart-city" | "security-lock" | "ai-voice" | "luxury-building";

export interface LottiePreset {
  id: LottiePresetId;
  name: string;
  category: string;
  icon: React.ElementType;
  lottieUrl: string;
  accentColor: string;
  glowGradient: string;
  description: string;
}

export const LOTTIE_PRESETS: LottiePreset[] = [
  {
    id: "smart-city",
    name: "Smart City Architecture",
    category: "Architecture & Urban",
    icon: Building2,
    lottieUrl: "https://assets9.lottiefiles.com/packages/lf20_w51pcehl.json",
    accentColor: "from-blue-600 to-indigo-600",
    glowGradient: "radial-gradient(circle at 30% 30%, rgba(59, 130, 246, 0.12), transparent 70%)",
    description: "Interactive 3D building construct & urban intelligence",
  },
  {
    id: "ai-voice",
    name: "AI Voice & Neural Mesh",
    category: "AI & Intelligence",
    icon: Cpu,
    lottieUrl: "https://assets3.lottiefiles.com/packages/lf20_1L47c5.json",
    accentColor: "from-indigo-600 to-purple-600",
    glowGradient: "radial-gradient(circle at 70% 30%, rgba(99, 102, 241, 0.12), transparent 70%)",
    description: "Real-time multilingual voice assistant waveform",
  },
  {
    id: "security-lock",
    name: "Vault & Key Pass",
    category: "Security & Auth",
    icon: ShieldCheck,
    lottieUrl: "https://assets2.lottiefiles.com/packages/lf20_kvw8u8p2.json",
    accentColor: "from-emerald-600 to-teal-600",
    glowGradient: "radial-gradient(circle at 20% 70%, rgba(16, 185, 129, 0.12), transparent 70%)",
    description: "Encrypted biometric token & vault key",
  },
  {
    id: "luxury-building",
    name: "Luxury Penthouse",
    category: "Real Estate",
    icon: Sparkles,
    lottieUrl: "https://assets10.lottiefiles.com/packages/lf20_5njp3vgg.json",
    accentColor: "from-cyan-500 to-blue-600",
    glowGradient: "radial-gradient(circle at 80% 80%, rgba(56, 189, 248, 0.12), transparent 70%)",
    description: "High-end property showcase & ambient floor plan",
  },
];

interface LottieAuthBackgroundProps {
  activePreset: LottiePresetId;
  onPresetChange?: (preset: LottiePresetId) => void;
  children?: React.ReactNode;
}

export function LottieAuthBackground({
  activePreset,
  onPresetChange,
  children,
}: LottieAuthBackgroundProps) {
  const currentPreset = LOTTIE_PRESETS.find((p) => p.id === activePreset) || LOTTIE_PRESETS[0];
  const [hasLottieError, setHasLottieError] = useState(false);
  const [mounted, setMounted] = useState(false);

  useEffect(() => {
    setMounted(true);
  }, []);

  useEffect(() => {
    setHasLottieError(false);
  }, [activePreset]);

  return (
    <div className="relative min-h-screen w-full overflow-hidden bg-theme-main text-theme-text transition-colors duration-300 selection:bg-cyan-500/20 selection:text-white">
      {/* Dynamic Radial Ambient Light Glow */}
      <motion.div
        key={currentPreset.id}
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: 1.2 }}
        className="pointer-events-none absolute inset-0 transition-all duration-1000"
        style={{ background: "radial-gradient(circle at 50% 30%, rgba(56, 189, 248, 0.05), transparent 70%)" }}
      />

      {/* Background Main Lottie Layer */}
      <div className="pointer-events-none absolute inset-0 flex items-center justify-center overflow-hidden opacity-30 md:opacity-40">
        {mounted && !hasLottieError ? (
          <div className="relative h-[850px] w-[850px] transform transition-transform duration-700 hover:scale-105">
            <DotLottieReact
              src={currentPreset.lottieUrl}
              loop
              autoplay
              onError={() => setHasLottieError(true)}
              style={{ width: "100%", height: "100%" }}
            />
          </div>
        ) : (
          /* Fallback Procedural Vector Waves */
          <div className="relative flex h-full w-full items-center justify-center">
            <motion.div
              animate={{ rotate: 360 }}
              transition={{ duration: 40, repeat: Infinity, ease: "linear" }}
              className="relative h-[600px] w-[600px] rounded-full border border-theme-border/50 bg-theme-card/40 backdrop-blur-3xl"
            >
              <div className="absolute inset-10 rounded-full border border-cyan-500/20 border-dashed" />
              <div className="absolute inset-24 rounded-full border border-cyan-500/10" />
            </motion.div>
          </div>
        )}
      </div>

      {/* Content Children */}
      <div className="relative z-10">{children}</div>
    </div>
  );
}
