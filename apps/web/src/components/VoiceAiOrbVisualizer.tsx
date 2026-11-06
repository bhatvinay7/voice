"use client";

import React from "react";
import { motion } from "framer-motion";
import { Mic, AudioLines } from "lucide-react";

interface VoiceAiOrbVisualizerProps {
  isCallActive: boolean;
  aiState: "idle" | "listening" | "thinking" | "speaking";
  onToggleCall?: () => void;
  className?: string;
}

// Organic "vein" paths inside a 200x200 viewBox, centred on (100,100)
const VEINS = [
  "M100 100 C 80 70, 60 60, 30 55",
  "M100 100 C 125 75, 145 60, 172 58",
  "M100 100 C 70 115, 55 135, 40 160",
  "M100 100 C 130 120, 150 140, 165 162",
  "M100 100 C 95 70, 105 45, 100 18",
  "M100 100 C 105 130, 95 155, 100 182",
];

export function VoiceAiOrbVisualizer({
  isCallActive,
  aiState,
  onToggleCall,
  className = "",
}: VoiceAiOrbVisualizerProps): React.ReactElement {
  const speaking = aiState === "speaking";
  const thinking = aiState === "thinking";
  const listening = aiState === "listening";
  const pulseDuration = speaking ? 0.9 : listening ? 1.6 : thinking ? 1.2 : 3.2;
  const veinSpeed = speaking ? 1.4 : thinking ? 2 : isCallActive ? 2.8 : 4.5;

  return (
    <div className={`relative flex items-center justify-center select-none ${className}`}>
      <div className="relative w-[280px] h-[280px] sm:w-[340px] sm:h-[340px] flex items-center justify-center">
        {/* Soft ambient glow */}
        <motion.div
          className="absolute inset-6 rounded-full bg-sky-400/30 blur-3xl"
          animate={{ scale: [1, speaking ? 1.18 : 1.08, 1], opacity: [0.5, 0.85, 0.5] }}
          transition={{ duration: pulseDuration, repeat: Infinity, ease: "easeInOut" }}
        />

        {/* Ripple rings */}
        {[0, 1, 2].map((i) => (
          <motion.span
            key={i}
            className="absolute inset-10 rounded-full border border-sky-400/40"
            animate={{ scale: [1, 1.45], opacity: [0.6, 0] }}
            transition={{
              duration: pulseDuration * 2,
              repeat: Infinity,
              delay: (i * pulseDuration * 2) / 3,
              ease: "easeOut",
            }}
          />
        ))}

        {/* Orb */}
        <motion.button
          id="voice-ai-orb"
          type="button"
          onClick={onToggleCall}
          aria-label={isCallActive ? "End voice call" : "Start voice call"}
          whileHover={{ scale: 1.03 }}
          whileTap={{ scale: 0.97 }}
          animate={{ scale: speaking ? [1, 1.04, 1] : [1, 1.015, 1] }}
          transition={{ duration: pulseDuration, repeat: Infinity, ease: "easeInOut" }}
          className="relative w-[200px] h-[200px] sm:w-[240px] sm:h-[240px] rounded-full overflow-hidden cursor-pointer focus:outline-none focus-visible:ring-4 focus-visible:ring-sky-400/50"
          style={{
            background:
              "radial-gradient(circle at 35% 30%, #e0f2fe 0%, #7dd3fc 22%, #0ea5e9 52%, #1e3a8a 100%)",
            boxShadow:
              "0 20px 60px -10px rgba(14,165,233,0.55), inset 0 -12px 30px rgba(30,58,138,0.55), inset 0 8px 20px rgba(255,255,255,0.55)",
          }}
        >
          {/* Animated veins */}
          <svg viewBox="0 0 200 200" className="absolute inset-0 w-full h-full">
            <defs>
              <linearGradient id="veinGrad" x1="0" y1="0" x2="1" y2="1">
                <stop offset="0%" stopColor="#ffffff" stopOpacity="0.95" />
                <stop offset="100%" stopColor="#bae6fd" stopOpacity="0.4" />
              </linearGradient>
            </defs>
            {VEINS.map((d, i) => (
              <g key={i}>
                <path d={d} stroke="rgba(255,255,255,0.18)" strokeWidth="1.4" fill="none" strokeLinecap="round" />
                <motion.path
                  d={d}
                  stroke="url(#veinGrad)"
                  strokeWidth="2"
                  fill="none"
                  strokeLinecap="round"
                  strokeDasharray="14 120"
                  animate={{ strokeDashoffset: [0, -134] }}
                  transition={{ duration: veinSpeed, repeat: Infinity, ease: "linear", delay: i * 0.25 }}
                />
              </g>
            ))}
          </svg>

          {/* Gloss highlight */}
          <span className="absolute top-3 left-1/2 -translate-x-1/2 w-2/3 h-1/3 rounded-full bg-gradient-to-b from-white/60 to-transparent blur-[2px]" />

          {/* Centre logo */}
          <span className="absolute inset-0 flex items-center justify-center">
            <motion.span
              className="flex h-20 w-20 sm:h-24 sm:w-24 items-center justify-center rounded-full bg-white/90 backdrop-blur shadow-lg shadow-sky-900/30"
              animate={thinking ? { rotate: 360 } : { rotate: 0 }}
              transition={thinking ? { duration: 3, repeat: Infinity, ease: "linear" } : { duration: 0.3 }}
            >
              {isCallActive ? (
                <AudioLines className="h-9 w-9 sm:h-10 sm:w-10 text-sky-600" />
              ) : (
                <Mic className="h-9 w-9 sm:h-10 sm:w-10 text-sky-600" />
              )}
            </motion.span>
          </span>
        </motion.button>

        {/* State label */}
        <span className="absolute bottom-0 left-1/2 -translate-x-1/2 inline-flex items-center gap-1.5 rounded-full bg-white/80 dark:bg-black/60 backdrop-blur px-3 py-1 text-[10px] font-semibold uppercase tracking-widest text-sky-600 dark:text-sky-300 border border-sky-200 dark:border-sky-500/30">
          <span className={`h-1.5 w-1.5 rounded-full ${isCallActive ? "bg-emerald-500 animate-pulse" : "bg-sky-400"}`} />
          Voice AI · {isCallActive ? aiState : "ready"}
        </span>
      </div>
    </div>
  );
}
