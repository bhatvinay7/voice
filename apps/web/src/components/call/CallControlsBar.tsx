"use client";

import React from "react";
import { Mic, MicOff, Volume2, VolumeX, Pause, Play } from "lucide-react";

interface CallControlsBarProps {
  isMicMuted: boolean;
  onToggleMic: () => void;
  isSpeakerMuted: boolean;
  onToggleSpeaker: () => void;
  interviewerSpeaking: boolean;
  onTogglePauseAi: () => void;
}

export function CallControlsBar({
  isMicMuted,
  onToggleMic,
  isSpeakerMuted,
  onToggleSpeaker,
  interviewerSpeaking,
  onTogglePauseAi,
}: CallControlsBarProps): React.ReactElement {
  return (
    <div className="mt-6 flex flex-wrap items-center justify-center gap-3 bg-theme-card border border-theme-border p-3 rounded-2xl shadow-lg relative z-10 dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.1)]">
      {/* Mic Mute Button */}
      <button
        onClick={onToggleMic}
        className={`flex items-center gap-2 px-4 py-2 rounded-xl text-xs font-bold transition-all ${
          isMicMuted
            ? "bg-rose-500/20 text-rose-400 border border-rose-500/40"
            : "bg-theme-main text-theme-text hover:bg-theme-card-hover border border-theme-border"
        }`}
      >
        {isMicMuted ? <MicOff className="w-4 h-4 text-rose-400" /> : <Mic className="w-4 h-4 text-emerald-400" />}
        <span>{isMicMuted ? "Mic Muted" : "Mute Mic"}</span>
      </button>

      {/* Speaker Mute Button */}
      <button
        onClick={onToggleSpeaker}
        className={`flex items-center gap-2 px-4 py-2 rounded-xl text-xs font-bold transition-all ${
          isSpeakerMuted
            ? "bg-rose-500/20 text-rose-400 border border-rose-500/40"
            : "bg-theme-main text-theme-text hover:bg-theme-card-hover border border-theme-border"
        }`}
      >
        {isSpeakerMuted ? (
          <VolumeX className="w-4 h-4 text-rose-400" />
        ) : (
          <Volume2 className="w-4 h-4 text-cyan-400" />
        )}
        <span>{isSpeakerMuted ? "Unmute Speaker" : "Mute Speaker"}</span>
      </button>

      {/* Pause/Hold AI Button */}
      <button
        onClick={onTogglePauseAi}
        className={`flex items-center gap-2 px-4 py-2 rounded-xl text-xs font-bold transition-all ${
          interviewerSpeaking
            ? "bg-indigo-500/20 text-indigo-300 border border-indigo-500/40 shadow-inner"
            : "bg-theme-main text-theme-sub hover:text-theme-text hover:bg-theme-card-hover border border-theme-border"
        }`}
      >
        {interviewerSpeaking ? (
          <>
            <Play className="w-4 h-4 text-emerald-400 fill-emerald-400" />
            <span>Resume AI Voice</span>
          </>
        ) : (
          <>
            <Pause className="w-4 h-4 text-zinc-400" />
            <span>Pause AI (Hold)</span>
          </>
        )}
      </button>
    </div>
  );
}
