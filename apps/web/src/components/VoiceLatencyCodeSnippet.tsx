"use client";

import React, { useState } from "react";
import { Copy, Check, Terminal, Cpu, Zap, Activity } from "lucide-react";

type CodeLang = "python" | "javascript" | "curl";

const CODE_EXAMPLES: Record<CodeLang, string> = {
  python: `from dwin import VoiceAgent, LiveKitSession

agent = VoiceAgent(
    model="dwin-voice-v2",
    voice="eve",
    system_prompt="You are a real estate assistant for property search & viewings."
)

session = agent.connect(webrtc_url="wss://voice.dwin.ai")
session.start_stream()`,

  javascript: `import { DwinVoiceAgent } from "@dwin/voice-sdk";

const agent = new DwinVoiceAgent({
  apiKey: process.env.DWIN_API_KEY,
  voice: "eve",
  onSpeechRecognized: (transcript) => console.log(transcript),
});

await agent.startCall({ callId: "session-101" });`,

  curl: `curl -X POST https://api.dwin.ai/v1/voice/calls \\
  -H "Authorization: Bearer $DWIN_API_KEY" \\
  -H "Content-Type: application/json" \\
  -d '{
    "voice": "eve",
    "prompt": "Find 2-bedroom listings under $120k",
    "turn_detection": "eou_server"
  }'`,
};

export function VoiceLatencyCodeSnippet() {
  const [activeLang, setActiveLang] = useState<CodeLang>("python");
  const [copied, setCopied] = useState(false);

  const handleCopy = () => {
    navigator.clipboard.writeText(CODE_EXAMPLES[activeLang]);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="w-full max-w-6xl mx-auto my-12 grid grid-cols-1 lg:grid-cols-12 gap-8 items-stretch">
      
      {/* Left Column: Latency & Speed Benchmarks */}
      <div className="lg:col-span-5 bg-slate-900 text-white rounded-3xl p-8 space-y-6 flex flex-col justify-between relative overflow-hidden shadow-xl">
        <div className="absolute top-0 right-0 w-64 h-64 bg-indigo-500/10 rounded-full blur-3xl pointer-events-none" />

        <div className="space-y-4 relative z-10">
          <div className="inline-flex items-center gap-2 px-3 py-1 rounded-full bg-indigo-500/20 border border-indigo-400/30 text-indigo-300 text-xs font-semibold">
            <Zap className="w-3.5 h-3.5 text-cyan-400 animate-pulse" />
            <span>Low Latency Voice Engine</span>
          </div>

          <h3 className="font-serif text-3xl font-normal text-white tracking-tight">
            Sub-180ms Real-Time Response
          </h3>

          <p className="text-slate-400 text-xs leading-relaxed font-normal">
            dwin Voice AI delivers instant speech-to-speech interaction. Continuous streaming minimizes latency pauses so conversation flows naturally.
          </p>
        </div>

        {/* Latency Stats Cards */}
        <div className="grid grid-cols-2 gap-3 relative z-10 pt-4">
          <div className="bg-slate-800/80 border border-slate-700/80 rounded-2xl p-4">
            <div className="flex items-center gap-2 text-cyan-400 text-xs font-mono mb-1">
              <Activity className="w-4 h-4" />
              <span>Time To First Token</span>
            </div>
            <div className="text-3xl font-bold text-white">&lt;180ms</div>
          </div>

          <div className="bg-slate-800/80 border border-slate-700/80 rounded-2xl p-4">
            <div className="flex items-center gap-2 text-emerald-400 text-xs font-mono mb-1">
              <Cpu className="w-4 h-4" />
              <span>Turn Taking</span>
            </div>
            <div className="text-3xl font-bold text-white">Instant</div>
          </div>
        </div>
      </div>

      {/* Right Column: Code Snippet Switcher */}
      <div className="lg:col-span-7 bg-slate-950 text-slate-200 rounded-3xl border border-slate-800 p-6 flex flex-col justify-between shadow-xl">
        {/* Header Tabs & Copy Button */}
        <div className="flex items-center justify-between border-b border-slate-800 pb-4 mb-4">
          <div className="flex items-center gap-2">
            <Terminal className="w-4 h-4 text-indigo-400" />
            <span className="text-xs font-bold text-slate-400 uppercase tracking-wider">Quick Start SDK</span>
          </div>

          <div className="flex items-center gap-2">
            {(["python", "javascript", "curl"] as CodeLang[]).map((lang) => (
              <button
                key={lang}
                onClick={() => setActiveLang(lang)}
                className={`px-3 py-1 rounded-xl text-xs font-mono transition-all ${
                  activeLang === lang
                    ? "bg-indigo-600 text-white font-bold"
                    : "bg-slate-900 text-slate-400 hover:text-white"
                }`}
              >
                {lang === "javascript" ? "JS / TS" : lang.toUpperCase()}
              </button>
            ))}

            <button
              onClick={handleCopy}
              className="p-1.5 rounded-xl bg-slate-900 hover:bg-slate-800 border border-slate-800 text-slate-400 hover:text-white transition-all ml-2"
              title="Copy Code"
            >
              {copied ? <Check className="w-4 h-4 text-emerald-400" /> : <Copy className="w-4 h-4" />}
            </button>
          </div>
        </div>

        {/* Code Content Box */}
        <pre className="p-4 bg-slate-900/90 rounded-2xl border border-slate-800/80 text-xs font-mono text-slate-300 leading-relaxed overflow-x-auto min-h-[160px]">
          <code>{CODE_EXAMPLES[activeLang]}</code>
        </pre>

        {/* Footer info */}
        <div className="flex items-center justify-between pt-4 text-[11px] text-slate-500 font-mono">
          <span>WebRTC & WebSocket streaming enabled</span>
          <span className="text-indigo-400">docs.dwin.ai/voice</span>
        </div>
      </div>

    </div>
  );
}
