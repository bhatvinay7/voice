"use client";

import React from "react";
import { motion } from "framer-motion";
import { Mic, PhoneOff } from "lucide-react";

interface CenterVoiceCoreProps {
  isCallActive: boolean;
  aiState: "idle" | "listening" | "thinking" | "speaking";
  onToggleCall?: () => void;
}

export function CenterVoiceCore({
  isCallActive,
  aiState,
  onToggleCall,
}: CenterVoiceCoreProps): React.ReactElement {
  return (
    <motion.div
      whileHover={{ scale: 1.05 }}
      whileTap={{ scale: 0.95 }}
      onClick={onToggleCall}
      className="relative z-20 w-[128px] h-[128px] sm:w-[144px] sm:h-[144px] rounded-full bg-[#030306] cursor-pointer flex flex-col items-center justify-center p-1 group transition-all duration-300 border-2 border-cyan-400/90"
      style={{
        boxShadow: isCallActive
          ? "0 0 28px rgba(56, 189, 248, 0.8), inset 0 0 20px rgba(56, 189, 248, 0.5), inset 0 2px 4px rgba(255, 255, 255, 0.95)"
          : "0 0 16px rgba(56, 189, 248, 0.45), inset 0 0 14px rgba(56, 189, 248, 0.25), inset 0 1.5px 3.5px rgba(255, 255, 255, 0.85)",
      }}
    >
      {/* Concentric Inner Dashed HUD Ring */}
      <div className="absolute inset-2 rounded-full border border-dashed border-white/25 group-hover:border-white/40 spin-reverse-slow pointer-events-none" />

      {/* Glowing Center Badge Content */}
      <div className="relative flex flex-col items-center justify-center text-center space-y-1 z-10 px-2">
        {/* Neural Status Indicator Pill */}
        <div className="flex items-center gap-1.5 mb-0.5">
          <span
            className={`w-2 h-2 rounded-full ${
              isCallActive
                ? aiState === "speaking"
                  ? "bg-cyan-400 animate-ping"
                  : aiState === "listening"
                  ? "bg-emerald-400 animate-pulse"
                  : "bg-white animate-pulse"
                : "bg-zinc-500"
            }`}
          />
          <span className="text-[9px] font-mono font-bold tracking-widest uppercase text-cyan-300 drop-shadow-sm">
            {isCallActive ? aiState : "READY"}
          </span>
        </div>

        {/* Futuristic Typography: VOICE AI */}
        <h2 className="text-sm sm:text-base font-extrabold tracking-[0.24em] text-white uppercase font-sans drop-shadow-[0_2px_12px_rgba(56,189,248,0.85)] group-hover:tracking-[0.27em] transition-all">
          VOICE AI
        </h2>

        {/* Action Hint on Hover */}
        <div className="flex items-center gap-1 text-[9px] font-mono text-zinc-400 group-hover:text-white transition-colors">
          {isCallActive ? (
            <>
              <PhoneOff className="w-2.5 h-2.5 text-rose-400" />
              <span>End</span>
            </>
          ) : (
            <>
              <Mic className="w-2.5 h-2.5 text-cyan-400" />
              <span>Start</span>
            </>
          )}
        </div>
      </div>

      {/* Specular Curved Highlight on Inner Disc */}
      <div className="absolute top-2 left-4 w-12 h-6 rounded-full bg-gradient-to-b from-white/35 to-transparent blur-[1px] pointer-events-none" />
    </motion.div>
  );
}
