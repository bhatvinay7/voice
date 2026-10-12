"use client";

import React from "react";
import { MessageSquare, Bot, ChevronRight, Send } from "lucide-react";

export interface TranscriptItem {
  role: "assistant" | "user";
  text: string;
  time: string;
}

interface CallTranscriptsViewProps {
  transcripts: TranscriptItem[];
  customInputText: string;
  onChangeInputText: (text: string) => void;
  onSubmitPrompt: (text: string) => void;
}

export function CallTranscriptsView({
  transcripts,
  customInputText,
  onChangeInputText,
  onSubmitPrompt,
}: CallTranscriptsViewProps): React.ReactElement {
  const quickPrompts = [
    "Find luxury 2BR apartments in Dubai Marina",
    "Show me high-floor studios near Burj Khalifa",
    "What are the viewing slots for Dubai Marina?",
    "Check tenancy contract Ejari compliance rules",
  ];

  const handleFormSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (customInputText.trim()) {
      onSubmitPrompt(customInputText);
      onChangeInputText("");
    }
  };

  return (
    <div className="bg-theme-card border border-theme-border dark:border-white/15 rounded-3xl p-6 sm:p-8 space-y-6 shadow-xl transition-colors duration-300 dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.1)]">
      <div className="flex items-center justify-between border-b border-theme-border dark:border-white/10 pb-4">
        <div className="flex items-center gap-2.5">
          <div className="p-2 rounded-xl bg-white/10 text-white border border-white/20">
            <MessageSquare className="w-4 h-4" />
          </div>
          <div>
            <h3 className="text-base font-bold text-theme-text font-serif">
              Live Voice Conversation Transcript
            </h3>
            <p className="text-xs text-theme-sub font-mono">
              Bi-directional speech recognition & neural synthesis stream
            </p>
          </div>
        </div>
      </div>

      {/* Transcript Bubbles */}
      <div className="space-y-4 max-h-72 overflow-y-auto pr-2 custom-scrollbar">
        {transcripts.map((t, idx) => (
          <div
            key={idx}
            className={`flex items-start gap-3.5 ${
              t.role === "user" ? "flex-row-reverse" : ""
            }`}
          >
            <div
              className={`w-8 h-8 rounded-xl flex items-center justify-center shrink-0 text-xs font-bold ${
                t.role === "assistant"
                  ? "bg-white/10 border border-white/20 text-white"
                  : "bg-emerald-500/20 border border-emerald-500/30 text-emerald-400"
              }`}
            >
              {t.role === "assistant" ? <Bot className="w-4 h-4 text-white" /> : "You"}
            </div>
            <div
              className={`max-w-[80%] rounded-2xl p-4 text-xs sm:text-sm ${
                t.role === "assistant"
                  ? "bg-theme-main border border-theme-border dark:border-white/10 text-theme-text"
                  : "bg-emerald-500/10 border border-emerald-500/20 text-emerald-200"
              }`}
            >
              <div className="flex items-center justify-between gap-4 mb-1">
                <span className="font-bold text-[11px] text-theme-sub font-mono uppercase">
                  {t.role === "assistant" ? "UAE Voice AI Agent" : "You (Caller)"}
                </span>
                <span className="text-[10px] text-zinc-500 font-mono">{t.time}</span>
              </div>
              <p className="leading-relaxed">{t.text}</p>
            </div>
          </div>
        ))}
      </div>

      {/* Quick Suggestion Chips */}
      <div className="pt-2 border-t border-theme-border dark:border-white/10 space-y-2">
        <span className="text-[11px] text-theme-sub font-mono block">
          Quick Simulated Voice Inquiries:
        </span>
        <div className="flex flex-wrap gap-2">
          {quickPrompts.map((q) => (
            <button
              key={q}
              onClick={() => onSubmitPrompt(q)}
              className="text-xs px-3 py-1.5 rounded-xl bg-theme-main border border-theme-border dark:border-white/10 hover:border-cyan-500/50 hover:text-theme-text text-theme-sub transition-colors flex items-center gap-1.5"
            >
              <span>{q}</span>
              <ChevronRight className="w-3 h-3 text-zinc-500" />
            </button>
          ))}
        </div>
      </div>

      {/* Text Simulation Input Bar */}
      <form onSubmit={handleFormSubmit} className="flex gap-2">
        <input
          type="text"
          value={customInputText}
          onChange={(e) => onChangeInputText(e.target.value)}
          placeholder="Type or speak a property inquiry..."
          className="flex-1 bg-theme-main border border-theme-border dark:border-white/15 text-theme-text rounded-xl px-4 py-2.5 text-xs focus:outline-none focus:border-cyan-500"
        />
        <button
          type="submit"
          className="px-4 py-2.5 rounded-xl bg-theme-card border border-theme-border text-theme-text hover:bg-black/5 dark:hover:bg-white/10 font-bold text-xs flex items-center gap-1.5 transition-colors shadow-sm"
        >
          <span>Send</span>
          <Send className="w-3 h-3" />
        </button>
      </form>
    </div>
  );
}
