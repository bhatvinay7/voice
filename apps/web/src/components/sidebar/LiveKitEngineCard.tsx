"use client";

import React from "react";
import { Mic, Radio } from "lucide-react";

interface LiveKitEngineCardProps {
  isCallActive?: boolean;
  onToggleCall?: () => void;
}

export function LiveKitEngineCard({
  isCallActive = false,
  onToggleCall,
}: LiveKitEngineCardProps): React.ReactElement {
  return (
    <div className="mt-auto pt-4">
      <div className="relative p-4 rounded-2xl bg-theme-main border border-theme-border dark:border-white/15 shadow-md dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.08)] overflow-hidden group">
        <div className="flex items-center justify-between mb-3 relative z-10">
          <div className="flex items-center gap-2">
            <span className="relative flex h-2.5 w-2.5">
              <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75"></span>
              <span className="relative inline-flex rounded-full h-2.5 w-2.5 bg-emerald-500"></span>
            </span>
            <span className="text-[11px] font-mono font-bold text-theme-text uppercase tracking-wider">
              LiveKit RTC Engine
            </span>
          </div>
          <span className="text-[9px] font-mono text-emerald-400 bg-emerald-500/10 px-2 py-0.5 rounded-md border border-emerald-500/30 font-bold shadow-[inset_0_1px_0_0_rgba(255,255,255,0.08)]">
            165ms TTFT
          </span>
        </div>

        <p className="text-[11px] text-theme-sub font-normal leading-relaxed mb-4 relative z-10">
          {isCallActive
            ? "Voice pipeline connected and actively streaming."
            : "Bilingual AI Voice assistant ready for instant property query."}
        </p>

        {onToggleCall && (
          <button
            onClick={onToggleCall}
            className={`w-full py-2.5 px-4 rounded-xl font-bold text-xs tracking-wide transition-all duration-200 flex items-center justify-center gap-2 relative z-10 shadow-sm ${
              isCallActive
                ? "bg-rose-500/20 text-rose-400 border border-rose-500/40 hover:bg-rose-500/30 dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.1)]"
                : "bg-theme-card hover:bg-black/10 dark:hover:bg-white/10 text-theme-text border border-theme-border dark:border-white/20 shadow-md"
            }`}
          >
            {isCallActive ? (
              <>
                <Radio className="w-3.5 h-3.5 animate-pulse" />
                <span>End Voice Session</span>
              </>
            ) : (
              <>
                <Mic className="w-3.5 h-3.5" />
                <span>Connect AI Voice Call</span>
              </>
            )}
          </button>
        )}
      </div>
    </div>
  );
}
