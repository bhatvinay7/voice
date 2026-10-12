"use client";

import React, { useState, useEffect, useRef, useCallback } from "react";
import { motion } from "framer-motion";
import { BrainCircuit, Mic, PhoneOff } from "lucide-react";
import { LiveKitRoom } from "@livekit/components-react";
import { VoiceAiOrbVisualizer } from "./VoiceAiOrbVisualizer";
import { LiveKitAudioController } from "./call/LiveKitAudioController";
import { AudioSpectrumWave } from "./call/AudioSpectrumWave";
import { CallControlsBar } from "./call/CallControlsBar";
import { PropertySuggestionsGrid, PropertySuggestion } from "./call/PropertySuggestionsGrid";
import { CallTranscriptsView, TranscriptItem } from "./call/CallTranscriptsView";

interface WebRtcCallWidgetProps {
  callId: string;
  userId: string;
  agentModeActive?: boolean;
  onToggleCallState?: (active: boolean) => void;
}

export function WebRtcCallWidget({
  callId,
  userId,
  agentModeActive = true,
  onToggleCallState,
}: WebRtcCallWidgetProps): React.ReactElement {
  const [isCallActive, setIsCallActive] = useState<boolean>(false);
  const [callError, setCallError] = useState<string | null>(null);
  const [aiState, setAiState] = useState<"idle" | "listening" | "thinking" | "speaking">("idle");
  const [interviewerSpeaking, setInterviewerSpeaking] = useState<boolean>(false);
  const [isMicMuted, setIsMicMuted] = useState<boolean>(false);
  const [isSpeakerMuted, setIsSpeakerMuted] = useState<boolean>(false);
  const [sseSuggestions, setSseSuggestions] = useState<PropertySuggestion[]>([]);
  const [liveKitToken, setLiveKitToken] = useState<string>("");
  const [liveKitUrl, setLiveKitUrl] = useState<string>("");
  const [customInputText, setCustomInputText] = useState<string>("");
  const wsRef = useRef<WebSocket | null>(null);

  const [transcripts, setTranscripts] = useState<TranscriptItem[]>([]);

  // --- WebSocket helpers (session-id-aware JSON protocol) ---
  const sendWsMessage = useCallback((payload: Record<string, unknown>) => {
    if (wsRef.current && wsRef.current.readyState === WebSocket.OPEN) {
      wsRef.current.send(JSON.stringify({ session_id: callId, ...payload }));
    }
  }, [callId]);

  const openWs = useCallback(() => {
    if (wsRef.current) return; // already open
    const protocol = window.location.protocol === "https:" ? "wss" : "ws";
    const ws = new WebSocket(`${protocol}://${window.location.host}/ws`);
    ws.onopen = () => console.log(`[WS] Connected for session ${callId}`);
    ws.onerror = (e) => console.warn("[WS] Error", e);
    ws.onclose = () => { wsRef.current = null; };
    wsRef.current = ws;
  }, [callId]);

  const closeWs = useCallback(() => {
    wsRef.current?.close();
    wsRef.current = null;
  }, []);

  const togglePauseAi = async (): Promise<void> => {
    const nextPaused = !interviewerSpeaking;
    setInterviewerSpeaking(nextPaused);
    if (nextPaused) {
      setAiState("idle");
      sendWsMessage({ type: "INTERVIEWER_SPEAKING_STOP" });
    } else {
      setAiState("listening");
      sendWsMessage({ type: "INTERVIEWER_SPEAKING_START" });
    }

    try {
      await fetch("/api/calls/pause", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ session_id: callId, paused: nextPaused }),
      });
    } catch (err: unknown) {
      console.warn("Failed to notify backend about pause state:", err);
    }
  };

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
      .catch((err: unknown) => console.warn("Initial suggestions fetch warning:", err));

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

  // Dynamic AI State Simulation when call is active
  useEffect(() => {
    if (isCallActive) {
      if (interviewerSpeaking) {
        setAiState("listening");
      } else {
        const states: ("listening" | "thinking" | "speaking")[] = ["listening", "thinking", "speaking"];
        const interval = setInterval(() => {
          if (!interviewerSpeaking) {
            setAiState(states[Math.floor(Math.random() * states.length)]);
          }
        }, 4000);
        return () => clearInterval(interval);
      }
    } else {
      setAiState("idle");
    }
  }, [isCallActive, interviewerSpeaking]);

  const toggleCall = async (): Promise<void> => {
    if (!isCallActive) {
      setCallError(null);
      try {
        const res = await fetch("/api/calls/session", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ call_id: callId, user_id: userId }),
        });
        const data = (await res.json()) as {
          livekit_token?: string;
          livekit_url?: string;
          success?: boolean;
          error?: string;
        };

        if (!data.livekit_token) {
          // Surface the error — do NOT silently activate without a token.
          // Without a token, LiveKitRoom never mounts, so the mic never opens.
          const reason = data.error || "Backend did not return a LiveKit token. Microphone cannot be enabled.";
          setCallError(reason);
          console.error("[Call] Cannot start — no livekit_token:", reason);
          return;
        }

        setLiveKitToken(data.livekit_token);
        if (data.livekit_url) {
          setLiveKitUrl(data.livekit_url);
        }
        setIsCallActive(true);
        onToggleCallState?.(true);
        openWs();
      } catch (err: unknown) {
        const msg = err instanceof Error ? err.message : "Network error reaching backend.";
        setCallError(`Failed to initialise call session: ${msg}`);
        console.error("[Call] Session init failed:", err);
      }
    } else {
      // End call
      closeWs();
      fetch("/api/calls/end", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ call_id: callId, user_id: userId }),
      }).catch((err) => console.warn("[Call] End-call notice:", err));
      setIsCallActive(false);
      setLiveKitToken("");
      setLiveKitUrl("");
      setCallError(null);
      onToggleCallState?.(false);
    }
  };

  const handleSimulateUserSpeech = (text: string): void => {
    if (!text.trim()) return;
    const now = new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    setTranscripts((prev) => [...prev, { role: "user", text, time: now }]);

    // Send to backend via WS using the JSON protocol (session_id included by sendWsMessage)
    sendWsMessage({ type: "STT_TRANSCRIPT", text, language: "en" });

    setAiState("thinking");
    setTimeout(() => {
      setAiState("speaking");
      setTranscripts((prev) => [
        ...prev,
        {
          role: "assistant",
          text: `Understood! Searching our verified database for luxury properties matching "${text}". I have pinned the top matching listings for you below.`,
          time: new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }),
        },
      ]);
    }, 1800);
  };

  const innerContent = (
    <div className="space-y-6">
      {/* Call Error Banner — shown when session init fails (e.g. no LiveKit token = no mic) */}
      {callError && (
        <div className="flex items-start gap-3 p-4 rounded-2xl bg-rose-500/10 border border-rose-500/30 text-rose-400 text-sm font-medium">
          <span className="text-rose-400 text-lg leading-none mt-0.5">⚠</span>
          <div>
            <p className="font-bold mb-0.5">Voice call failed to start</p>
            <p className="text-xs text-rose-300/80">{callError}</p>
          </div>
          <button onClick={() => setCallError(null)} className="ml-auto text-rose-400 hover:text-rose-300 text-xs font-bold">✕</button>
        </div>
      )}
      {/* Central AI Voice Representation Stage */}
      <div className="relative bg-theme-card border border-theme-border dark:border-white/15 rounded-[2.2rem] p-7 md:p-10 shadow-2xl overflow-hidden transition-colors duration-300 dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.12)]">
        <div className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-96 h-96 bg-cyan-500/10 rounded-full blur-[110px] pointer-events-none" />
        <div className="absolute -top-24 -right-24 w-72 h-72 bg-white/5 rounded-full blur-[90px] pointer-events-none" />

        {/* Top Voice Header & Connection Badge */}
        <div className="flex flex-col sm:flex-row items-start sm:items-center justify-between gap-4 relative z-10 border-b border-theme-border dark:border-white/10 pb-6 mb-8">
          <div className="flex items-center gap-4">
            <div className="w-12 h-12 rounded-2xl bg-white/10 border border-white/20 p-0.5 shadow-lg shadow-black/50">
              <div className="w-full h-full bg-[#050507] rounded-[14px] flex items-center justify-center text-cyan-400 dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.2)]">
                <BrainCircuit className="w-6 h-6 drop-shadow-md text-white" />
              </div>
            </div>
            <div>
              <div className="flex items-center gap-2">
                <h2 className="text-xl md:text-2xl font-bold text-white tracking-tight font-serif">
                  UAE Real Estate Voice Agent
                </h2>
                <span className="text-[10px] font-mono text-cyan-400 bg-cyan-500/10 px-2.5 py-0.5 rounded-full border border-cyan-500/30 font-bold shadow-[inset_0_1px_0_0_rgba(255,255,255,0.08)]">
                  Neural v2.4
                </span>
              </div>
              <p className="text-xs text-zinc-400 font-mono mt-0.5">
                Bilingual WebRTC Speech-to-Speech Architecture &bull; Sub-180ms Latency
              </p>
            </div>
          </div>

          <div className="flex items-center gap-3 self-end sm:self-center">
            {isCallActive ? (
              <span className="inline-flex items-center gap-2 px-3.5 py-1.5 rounded-full bg-emerald-500/15 border border-emerald-500/40 text-emerald-400 text-xs font-bold font-mono tracking-wider shadow-inner dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.1)]">
                <span className="w-2 h-2 rounded-full bg-emerald-400 animate-ping" />
                SESSION ACTIVE
              </span>
            ) : (
              <span className="inline-flex items-center gap-2 px-3.5 py-1.5 rounded-full bg-[#09090b] border border-white/15 text-zinc-400 text-xs font-semibold font-mono dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.08)]">
                <span className="w-2 h-2 rounded-full bg-zinc-500" />
                STANDBY
              </span>
            )}

            <button
              onClick={toggleCall}
              className={`px-6 py-2.5 rounded-2xl font-extrabold text-xs tracking-wider uppercase transition-all duration-300 shadow-lg flex items-center gap-2 ${
                isCallActive
                  ? "bg-rose-500 hover:bg-rose-600 text-white shadow-rose-500/25 border border-rose-400/40"
                  : "bg-white hover:bg-zinc-100 text-black border border-slate-300 dark:bg-black dark:text-white dark:border-white/20 dark:hover:bg-zinc-900"
              }`}
            >
              {isCallActive ? (
                <>
                  <PhoneOff className="w-4 h-4" />
                  <span>End Call</span>
                </>
              ) : (
                <>
                  <Mic className="w-4 h-4" />
                  <span>Start Voice Call</span>
                </>
              )}
            </button>
          </div>
        </div>

        {/* Central Voice AI Stage */}
        <div className="relative z-10 flex flex-col items-center justify-center py-4 sm:py-6">
          <VoiceAiOrbVisualizer
            isCallActive={isCallActive}
            aiState={aiState}
            onToggleCall={toggleCall}
          />

          <div className="mt-4 text-center space-y-1 relative z-10">
            <h3 className="text-base sm:text-lg font-bold text-white uppercase tracking-widest font-mono">
              {interviewerSpeaking
                ? "AI Paused (On Hold)"
                : aiState === "speaking"
                ? "AI Speaking..."
                : aiState === "listening"
                ? "Listening to Voice..."
                : aiState === "thinking"
                ? "Processing Neural Context..."
                : "AI Agent Standby"}
            </h3>
            <p className="text-xs text-zinc-400 font-mono">
              {isCallActive
                ? "Speak naturally into your microphone or pick a prompt"
                : "Click the Voice AI orb or 'Start Voice Call' to launch live WebRTC audio stream"}
            </p>
          </div>

          {/* Interactive Voice Controls Bar */}
          {isCallActive && (
            <motion.div initial={{ opacity: 0, y: 15 }} animate={{ opacity: 1, y: 0 }}>
              <CallControlsBar
                isMicMuted={isMicMuted}
                onToggleMic={() => setIsMicMuted(!isMicMuted)}
                isSpeakerMuted={isSpeakerMuted}
                onToggleSpeaker={() => setIsSpeakerMuted(!isSpeakerMuted)}
                interviewerSpeaking={interviewerSpeaking}
                onTogglePauseAi={togglePauseAi}
              />
            </motion.div>
          )}

          {/* Audio Wave Spectrum Canvas Bar */}
          <div className="mt-6 w-full max-w-xs">
            <AudioSpectrumWave isCallActive={isCallActive} aiState={aiState} />
          </div>
        </div>
      </div>

      {/* Property Recommendations Grid */}
      <PropertySuggestionsGrid suggestions={sseSuggestions} />

      {/* Real-time Conversation Transcript Panel */}
      <CallTranscriptsView
        transcripts={transcripts}
        customInputText={customInputText}
        onChangeInputText={setCustomInputText}
        onSubmitPrompt={handleSimulateUserSpeech}
      />
    </div>
  );

  if (isCallActive && liveKitToken) {
    return (
      <LiveKitRoom
        token={liveKitToken}
        serverUrl={liveKitUrl || process.env.NEXT_PUBLIC_LIVEKIT_URL || "wss://estate-12gbyba5.livekit.cloud"}
        connect={true}
        audio={!isMicMuted}
        video={false}
        options={{
          audioCaptureDefaults: {
            echoCancellation: true,
            noiseSuppression: true,
            autoGainControl: true,
            channelCount: 1,
          },
        }}
      >
        <LiveKitAudioController isMicMuted={isMicMuted} isSpeakerMuted={isSpeakerMuted} />
        {innerContent}
      </LiveKitRoom>
    );
  }

  return innerContent;
}
