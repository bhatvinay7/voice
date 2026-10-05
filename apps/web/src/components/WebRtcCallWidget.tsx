"use client";

import React, { useState, useEffect } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { Mic, Radio, Loader2, Sparkles, MessageSquare } from "lucide-react";
import { LiveKitRoom, RoomAudioRenderer } from "@livekit/components-react";

interface WebRtcCallWidgetProps {
  callId: string;
  userId: string;
  agentModeActive: boolean;
}

interface PropertySuggestion {
  id: string;
  title: string;
  description: string;
  price_aed: number;
  bedrooms: number;
  bathrooms: number;
  city?: string;
  images?: string[];
}

export function WebRtcCallWidget({
  callId,
  userId,
  agentModeActive,
}: WebRtcCallWidgetProps): React.ReactElement {
  const [isCallActive, setIsCallActive] = useState<boolean>(false);
  const [aiState, setAiState] = useState<"idle" | "listening" | "thinking" | "speaking">("idle");
  const [interviewerSpeaking, setInterviewerSpeaking] = useState<boolean>(false);
  const [sseSuggestions, setSseSuggestions] = useState<PropertySuggestion[]>([]);
  const [liveKitToken, setLiveKitToken] = useState<string>("");
  const [transcripts, setTranscripts] = useState<Array<{ role: string; text: string }>>([
    { role: "assistant", text: "Hello! I am your Real Estate AI Voice Assistant. How can I help you today?" },
  ]);

  // Fetch initial cached suggestions & subscribe to SSE
  useEffect(() => {
    if (!callId) return;

    fetch(`/api/calls/suggestions/${callId}`)
      .then((res: Response) => res.json())
      .then((data: { suggestions?: PropertySuggestion[] }) => {
        if (data.suggestions && Array.isArray(data.suggestions)) {
          setSseSuggestions(data.suggestions);
        }
      })
      .catch((err: unknown) => console.error("Failed to fetch initial suggestions:", err));

    if (!isCallActive) return;

    const sse = new EventSource(`/api/calls/suggestions/sse?call_id=${callId}`);
    sse.onmessage = (event: MessageEvent) => {
      try {
        const data = JSON.parse(event.data) as { suggestions?: PropertySuggestion[] };
        if (data.suggestions && Array.isArray(data.suggestions)) {
          setSseSuggestions(data.suggestions);
        }
      } catch (err: unknown) {
        console.warn("SSE parse warning:", err);
      }
    };

    return () => {
      sse.close();
    };
  }, [isCallActive, callId]);

  // Simulate AI States based on call status
  useEffect(() => {
    if (isCallActive) {
      if (interviewerSpeaking) {
        setAiState("listening");
      } else {
        // Randomly flip between thinking and speaking for demo
        const states: ("listening" | "thinking" | "speaking")[] = ["listening", "thinking", "speaking"];
        const interval = setInterval(() => {
          if (!interviewerSpeaking) {
            setAiState(states[Math.floor(Math.random() * states.length)]);
          }
        }, 3000);
        return () => clearInterval(interval);
      }
    } else {
      setAiState("idle");
    }
  }, [isCallActive, interviewerSpeaking]);

  const toggleCall = async (): Promise<void> => {
    if (!isCallActive) {
      try {
        const res = await fetch("/api/calls/session", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ call_id: callId, user_id: userId })
        });
        const data = (await res.json()) as { livekit_token?: string };
        if (data.livekit_token) {
          setLiveKitToken(data.livekit_token);
          setIsCallActive(true);
        } else {
          console.error("Failed to get LiveKit token");
          alert("Could not connect to voice server.");
        }
      } catch (err: unknown) {
        console.error("Call error:", err);
      }
    } else {
      setIsCallActive(false);
      setLiveKitToken("");
    }
  };

  const handleSimulateUserSpeech = (text: string): void => {
    setTranscripts((prev: Array<{ role: string; text: string }>) => [...prev, { role: "user", text }]);
  };

  const toggleInterviewerSpeaking = (): void => {
    setInterviewerSpeaking(!interviewerSpeaking);
  };

  const innerContent = (
    <div className="relative">
      {/* Side AI State Animation (Large Screens) */}
      <AnimatePresence>
        {isCallActive && (
          <motion.div
            initial={{ opacity: 0, x: -50 }}
            animate={{ opacity: 1, x: 0 }}
            exit={{ opacity: 0, x: -50 }}
            className="hidden lg:flex absolute -left-64 top-10 flex-col items-center justify-center space-y-4 w-48 glass-panel p-6 shadow-sm crosshair-corners"
          >
            <div className="relative w-24 h-24 flex items-center justify-center">
              {/* Outer Ripple */}
              <motion.div
                animate={{ scale: aiState === "listening" || aiState === "speaking" ? [1, 1.5, 1] : 1, opacity: [0.5, 0, 0.5] }}
                transition={{ repeat: Infinity, duration: 2, ease: "easeInOut" }}
                className="absolute inset-0 bg-indigo-500/10 rounded-full"
              />
              <motion.div
                animate={{ scale: aiState === "thinking" ? [1, 1.2, 1] : 1, opacity: [0.8, 0.4, 0.8] }}
                transition={{ repeat: Infinity, duration: 1.5, ease: "easeInOut" }}
                className="absolute inset-4 bg-violet-500/20 rounded-full blur-md"
              />
              
              <div className="relative z-10 bg-white border border-indigo-200 rounded-full p-4 shadow-sm">
                {aiState === "listening" && <Mic className="w-8 h-8 text-emerald-500" />}
                {aiState === "thinking" && <Loader2 className="w-8 h-8 text-indigo-500 animate-spin" />}
                {aiState === "speaking" && <Radio className="w-8 h-8 text-violet-500 animate-pulse" />}
                {aiState === "idle" && <Sparkles className="w-8 h-8 text-slate-400" />}
              </div>
            </div>

            <div className="text-center space-y-1">
              <h4 className="text-slate-900 font-bold tracking-widest uppercase text-xs type-mono-label">
                {aiState === "listening" && "Listening..."}
                {aiState === "thinking" && "Processing..."}
                {aiState === "speaking" && "Speaking..."}
              </h4>
              <p className="text-[10px] text-slate-500 font-mono">Neural Engine Active</p>
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      <div className="glass-panel p-6 md:p-8 shadow-sm space-y-6 relative z-10 crosshair-corners neo-grid-sm">
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 border-b border-slate-200 pb-6 bg-white/80 p-4 rounded-xl shadow-sm border">
          <div className="flex items-center gap-4">
            <div className="p-3 bg-indigo-50 rounded-xl border border-indigo-100 text-indigo-600">
              <Mic className="w-6 h-6" />
            </div>
            <div>
              <h3 className="text-2xl font-bold text-slate-900 tracking-tight flex items-center gap-2">
                Live AI Assistant
              </h3>
              <p className="text-sm text-slate-500 font-mono">Real-time voice search and intelligent property matching</p>
            </div>
          </div>
          <div className="flex items-center gap-3">
            {isCallActive && (
              <span className="flex items-center space-x-2 text-xs text-emerald-600 bg-emerald-50 px-4 py-2 border border-emerald-200 font-bold shadow-sm type-mono-label">
                <span className="w-2 h-2 rounded-full bg-emerald-500 animate-pulse"></span>
                <span className="tracking-widest uppercase">Live Session</span>
              </span>
            )}
            <button
              onClick={toggleCall}
              className={`px-6 py-3 font-bold text-sm tracking-wide transition-all shadow-sm overflow-hidden relative type-mono-label border ${
                isCallActive
                  ? "bg-white border-rose-200 text-rose-600 hover:bg-rose-50"
                  : "bg-indigo-600 border-indigo-700 hover:bg-indigo-700 text-white"
              }`}
            >
              {isCallActive ? "End Session" : "Start Voice Call"}
            </button>
          </div>
        </div>

        {/* Human Override Switch */}
        {isCallActive && (
          <div className="p-5 bg-white border border-slate-200 shadow-sm flex flex-col sm:flex-row items-start sm:items-center justify-between gap-4 crosshair-corners">
            <div>
              <p className="text-sm font-bold text-slate-900 font-mono">Manual Speech Override</p>
              <p className="text-xs text-slate-500 mt-1">
                Pause the AI Assistant to allow human intervention or clear background noise.
              </p>
            </div>
            <button
              onClick={toggleInterviewerSpeaking}
              className={`px-5 py-2.5 text-xs font-bold transition-all shadow-sm whitespace-nowrap type-mono-label border ${
                interviewerSpeaking
                  ? "bg-amber-100 text-amber-900 border-amber-300"
                  : "bg-white text-slate-700 hover:bg-slate-50 border-slate-300"
              }`}
            >
              {interviewerSpeaking ? "Hold to Pause AI" : "Pause AI Assistant"}
            </button>
          </div>
        )}

        {/* Real-time Recommendations Stream */}
        <AnimatePresence>
          {isCallActive && sseSuggestions.length > 0 && (
            <motion.div 
              initial={{ opacity: 0, height: 0 }}
              animate={{ opacity: 1, height: "auto" }}
              exit={{ opacity: 0, height: 0 }}
              className="p-5 rounded-2xl bg-cyan-950/20 border border-cyan-500/20 space-y-3"
            >
              <p className="text-xs font-bold text-cyan-400 uppercase tracking-widest flex items-center gap-2">
                <Sparkles className="w-4 h-4" /> Smart Recommendations
              </p>
              <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 mt-2">
                {sseSuggestions.map((prop: PropertySuggestion, idx: number) => (
                  <motion.div 
                    initial={{ opacity: 0, x: -10 }}
                    animate={{ opacity: 1, x: 0 }}
                    key={idx} 
                    className="bg-white border border-slate-200 shadow-sm flex flex-col crosshair-corners group"
                  >
                    <div className="h-32 w-full relative overflow-hidden bg-slate-100 border-b border-slate-200">
                       <img src={`https://images.unsplash.com/photo-1512917774080-9991f1c4c750?ixlib=rb-4.0.3&auto=format&fit=crop&w=800&q=80`} alt="Property" className="object-cover w-full h-full group-hover:scale-105 transition-transform duration-500" />
                       <div className="absolute top-2 right-2 bg-white/90 backdrop-blur text-slate-900 text-xs px-2 py-1 font-bold border border-slate-200 type-mono-label">
                          {prop.price_aed?.toLocaleString()} AED
                       </div>
                    </div>
                    <div className="p-4 flex-1 flex flex-col">
                      <h4 className="font-bold text-slate-900 text-sm line-clamp-1">{prop.title}</h4>
                      <p className="text-xs text-slate-500 mt-1 line-clamp-2 mb-3">{prop.description}</p>
                      <div className="mt-auto flex justify-between items-center pt-3 border-t border-slate-100">
                        <div className="flex gap-2 text-xs text-slate-600 font-mono">
                           <span>🛏️ {prop.bedrooms}</span>
                           <span>🛁 {prop.bathrooms}</span>
                        </div>
                        <a href={`/property/${prop.id}`} target="_blank" className="text-xs bg-indigo-50 hover:bg-indigo-100 text-indigo-700 border border-indigo-200 font-bold px-3 py-1.5 transition-colors type-mono-label">
                           View Details
                        </a>
                      </div>
                    </div>
                  </motion.div>
                ))}
              </div>
            </motion.div>
          )}
        </AnimatePresence>

        {/* Live Transcript Stream */}
        <div className="space-y-4 bg-slate-50 p-5 border border-slate-200 max-h-72 overflow-y-auto shadow-inner crosshair-corners relative">
          <p className="text-xs font-bold text-slate-500 uppercase tracking-widest flex items-center gap-2 sticky top-0 bg-slate-50/90 p-2 backdrop-blur-md type-mono-label border-b border-slate-200 z-10 -mt-2 -mx-2 mb-4">
            <MessageSquare className="w-4 h-4" /> Live Conversation
          </p>
          {transcripts.map((t, idx:number) => (
            <motion.div
              initial={{ opacity: 0, y: 10 }}
              animate={{ opacity: 1, y: 0 }}
              key={idx}
              className={`p-4 text-sm leading-relaxed max-w-[85%] border shadow-sm ${
                t.role === "assistant"
                  ? "bg-white text-slate-900 border-indigo-200 mr-auto"
                  : "bg-slate-900 text-slate-100 border-slate-800 ml-auto"
              }`}
            >
              <span className={`font-bold block text-[10px] uppercase mb-1.5 tracking-wider type-mono-label ${t.role === "assistant" ? "text-indigo-600" : "text-slate-400"}`}>
                {t.role === "assistant" ? "AI Assistant" : "You"}
              </span>
              {t.text}
            </motion.div>
          ))}
        </div>

        {/* Quick Prompt Inputs */}
        <AnimatePresence>
          {isCallActive && (
            <motion.div 
              initial={{ opacity: 0, y: 10 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: 10 }}
              className="flex flex-wrap gap-3 pt-2"
            >
              <button
                onClick={() => handleSimulateUserSpeech("Show me 2-bedroom listings in Dubai Marina under 120k")}
                className="text-xs bg-white hover:bg-slate-50 text-slate-700 px-4 py-2.5 border border-slate-200 transition-colors shadow-sm font-mono"
              >
                "Search Marina 2BR under 120k"
              </button>
              <button
                onClick={() => handleSimulateUserSpeech("I would like to book a viewing slot for prop-101")}
                className="text-xs bg-white hover:bg-slate-50 text-slate-700 px-4 py-2.5 border border-slate-200 transition-colors shadow-sm font-mono"
              >
                "Book viewing slot for prop-101"
              </button>
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    </div>
  );

  return isCallActive && liveKitToken ? (
    <LiveKitRoom
      serverUrl={process.env.NEXT_PUBLIC_LIVEKIT_URL || "ws://localhost:7880"}
      token={liveKitToken}
      connect={true}
      audio={true}
      video={false}
    >
      <RoomAudioRenderer />
      {innerContent}
    </LiveKitRoom>
  ) : (
    innerContent
  );
};
