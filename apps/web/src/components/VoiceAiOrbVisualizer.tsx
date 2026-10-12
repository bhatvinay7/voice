"use client";

import React, { useRef, useState } from "react";
import { CircuitBoardSvg } from "./visualizer/CircuitBoardSvg";
import { CenterVoiceCore } from "./visualizer/CenterVoiceCore";
import { useOrbCanvas } from "./visualizer/useOrbCanvas";

interface VoiceAiOrbVisualizerProps {
  isCallActive: boolean;
  aiState: "idle" | "listening" | "thinking" | "speaking";
  onToggleCall?: () => void;
  className?: string;
}

export function VoiceAiOrbVisualizer({
  isCallActive,
  aiState,
  onToggleCall,
  className = "",
}: VoiceAiOrbVisualizerProps): React.ReactElement {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [, setIsHovered] = useState(false);

  // Initialize and run high-performance Canvas rendering (constellation + voice spikes + laser beam)
  useOrbCanvas(canvasRef, isCallActive, aiState);

  return (
    <div
      ref={containerRef}
      className={`relative flex items-center justify-center select-none ${className}`}
      onMouseEnter={() => setIsHovered(true)}
      onMouseLeave={() => setIsHovered(false)}
    >
      {/* Visualizer Stage Container */}
      <div className="relative w-[340px] h-[340px] sm:w-[420px] sm:h-[420px] md:w-[450px] md:h-[450px] flex items-center justify-center">
        {/* 1. Sci-Fi Holographic Circuit Board Traces (SVG Layer) */}
        <CircuitBoardSvg />

        {/* 2. Concentric Cybernetic HUD Rings */}
        <div className="absolute w-[310px] h-[310px] sm:w-[370px] sm:h-[370px] md:w-[390px] md:h-[390px] rounded-full border border-cyan-500/30 spin-slow pointer-events-none" />
        <div className="absolute w-[295px] h-[295px] sm:w-[350px] sm:h-[350px] md:w-[370px] md:h-[370px] rounded-full border border-dashed border-white/20 spin-reverse-slow pointer-events-none" />

        {/* 3. Luminous Cyan Outer Halo Ring with Pure White Specular Edge Shine */}
        <div
          className="absolute w-[280px] h-[280px] sm:w-[335px] sm:h-[335px] md:w-[350px] md:h-[350px] rounded-full border-2 border-cyan-400/90 pointer-events-none transition-all duration-500"
          style={{
            boxShadow: isCallActive
              ? "0 0 35px rgba(56, 189, 248, 0.75), inset 0 0 25px rgba(56, 189, 248, 0.5), inset 0 2.5px 5px rgba(255, 255, 255, 0.95)"
              : "0 0 22px rgba(56, 189, 248, 0.45), inset 0 0 16px rgba(56, 189, 248, 0.3), inset 0 1.5px 3.5px rgba(255, 255, 255, 0.8)",
          }}
        />

        {/* 4. Deep Obsidian / Pitch Black Core Sphere Body with White Rim Shine */}
        <div
          className="relative w-[255px] h-[255px] sm:w-[305px] sm:h-[305px] md:w-[320px] md:h-[320px] rounded-full bg-[#030407] overflow-hidden flex items-center justify-center border border-white/25 transition-all duration-300"
          style={{
            boxShadow:
              "inset 0 2px 7px rgba(255, 255, 255, 0.7), inset 0 -3px 8px rgba(0, 0, 0, 0.95), inset 0 0 35px rgba(56, 189, 248, 0.25), 0 20px 50px rgba(0, 0, 0, 0.95)",
          }}
        >
          {/* Subtle 3D Spherical Radial Light */}
          <div className="absolute inset-0 bg-[radial-gradient(circle_at_50%_32%,rgba(56,189,248,0.14)_0%,rgba(3,4,7,0.98)_72%)] pointer-events-none" />

          {/* Micro Constellation Grid Pattern */}
          <div
            className="absolute inset-0 opacity-30 pointer-events-none"
            style={{
              backgroundImage: "radial-gradient(rgba(255, 255, 255, 0.3) 1px, transparent 1px)",
              backgroundSize: "20px 20px",
            }}
          />

          {/* Top Specular Curved Glass Lens Flare */}
          <div className="absolute -top-1 left-1/4 right-1/4 h-7 bg-gradient-to-b from-white/40 via-white/10 to-transparent rounded-full blur-[2px] pointer-events-none" />

          {/* 5. Dynamic Voice Spikes Waveform & Constellation HTML5 Canvas */}
          <canvas
            ref={canvasRef}
            className="absolute inset-0 w-full h-full pointer-events-none z-10"
          />

          {/* 6. Central Round-Shaped Structure */}
          <CenterVoiceCore
            isCallActive={isCallActive}
            aiState={aiState}
            onToggleCall={onToggleCall}
          />
        </div>
      </div>
    </div>
  );
}
