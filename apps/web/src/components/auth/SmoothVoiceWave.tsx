"use client";

import React, { useMemo } from "react";
import { motion, Variants } from "framer-motion";

interface SmoothVoiceWaveProps {
  isPlaying: boolean;
}

// SVG Spiked Wave Variants for background ribbon morphing
const spikedWaveVariants: Variants = {
  active: {
    d: [
      "M 0 45 Q 35 10, 70 45 T 140 15 T 210 55 T 280 10 T 350 50 T 420 20 T 490 55 T 560 45 V 90 H 0 Z",
      "M 0 45 Q 35 60, 70 20 T 140 55 T 210 10 T 280 50 T 350 15 T 420 55 T 490 20 T 560 45 V 90 H 0 Z",
      "M 0 45 Q 35 25, 70 50 T 140 10 T 210 40 T 280 60 T 350 20 T 420 40 T 490 15 T 560 45 V 90 H 0 Z",
      "M 0 45 Q 35 10, 70 45 T 140 15 T 210 55 T 280 10 T 350 50 T 420 20 T 490 55 T 560 45 V 90 H 0 Z",
    ],
    transition: {
      duration: 2.2,
      repeat: Infinity,
      ease: "easeInOut",
    },
  },
  idle: {
    d: "M 0 45 Q 35 45, 70 45 T 140 45 T 210 45 T 280 45 T 350 45 T 420 45 T 490 45 T 560 45 V 90 H 0 Z",
    transition: { duration: 0.8, ease: "easeOut" },
  },
};

const secondarySpikeVariants: Variants = {
  active: {
    d: [
      "M 0 45 Q 40 55, 80 20 T 160 60 T 240 15 T 320 50 T 400 20 T 480 55 T 560 45 V 90 H 0 Z",
      "M 0 45 Q 40 15, 80 55 T 160 20 T 240 60 T 320 15 T 400 55 T 480 25 T 560 45 V 90 H 0 Z",
      "M 0 45 Q 40 40, 80 15 T 160 50 T 240 25 T 320 60 T 400 15 T 480 40 T 560 45 V 90 H 0 Z",
      "M 0 45 Q 40 55, 80 20 T 160 60 T 240 15 T 320 50 T 400 20 T 480 55 T 560 45 V 90 H 0 Z",
    ],
    transition: {
      duration: 3,
      repeat: Infinity,
      ease: "easeInOut",
      delay: 0.15,
    },
  },
  idle: {
    d: "M 0 45 Q 40 45, 80 45 T 160 45 T 240 45 T 320 45 T 400 45 T 480 45 T 560 45 V 90 H 0 Z",
    transition: { duration: 0.8, ease: "easeOut" },
  },
};

export function SmoothVoiceWave({ isPlaying }: SmoothVoiceWaveProps) {
  // Generate 32 spike visualizer bars with natural sound envelope amplitudes
  const spikeBars = useMemo(() => {
    const count = 32;
    return Array.from({ length: count }).map((_, i) => {
      // Gaussian center distribution for realistic audio spectrum profile
      const normalizedPos = i / (count - 1); // 0 to 1
      const centerFactor = Math.sin(normalizedPos * Math.PI); // 0 at edges, 1 in middle

      // Create realistic heights (in pixels)
      const minActiveHeight = 8 + Math.floor(centerFactor * 10);
      const maxActiveHeight = Math.floor(25 + centerFactor * 55);
      const midActiveHeight = Math.floor(15 + centerFactor * 35);
      const peakActiveHeight = Math.floor(30 + centerFactor * 65);

      const idleHeight = 6 + Math.floor(centerFactor * 8);

      // Duration & delay jitter for fluid organic spike motion
      const duration = 0.5 + (i % 5) * 0.15 + (i % 3) * 0.1;
      const delay = (i * 0.04) % 0.4;

      return {
        id: i,
        idleHeight,
        activeHeights: [minActiveHeight, maxActiveHeight, midActiveHeight, peakActiveHeight, minActiveHeight],
        duration,
        delay,
        isAccent: i % 4 === 0,
      };
    });
  }, []);

  return (
    <div className="relative w-full h-28 sm:h-32 bg-theme-main rounded-2xl border border-theme-border overflow-hidden shadow-2xl flex items-center justify-center p-2 sm:p-4">
      {/* Dynamic Background Ambient Glow */}
      <motion.div
        animate={{
          scale: isPlaying ? [1, 1.25, 1] : 1,
          opacity: isPlaying ? [0.35, 0.65, 0.35] : 0.15,
        }}
        transition={{ duration: 2.5, repeat: Infinity, ease: "easeInOut" }}
        className="absolute inset-0 bg-[radial-gradient(ellipse_at_center,_var(--tw-gradient-stops))] from-cyan-500/20 via-white/10 to-transparent blur-xl pointer-events-none"
      />

      {/* Center Dynamic Baseline Axis */}
      <div className="absolute inset-x-4 top-1/2 -translate-y-1/2 h-[1px] bg-gradient-to-r from-transparent via-cyan-400/50 to-transparent pointer-events-none" />

      {/* Layer 1: SVG Spiked Morphing Fluid Ribbon (Backdrop) */}
      <svg
        className="absolute inset-0 w-full h-full opacity-40 pointer-events-none z-0"
        viewBox="0 0 560 90"
        preserveAspectRatio="none"
        fill="none"
      >
        <defs>
          <linearGradient id="spikedGradient1" x1="0%" y1="0%" x2="100%" y2="0%">
            <stop offset="0%" stopColor="#38bdf8" stopOpacity="0.8" />
            <stop offset="50%" stopColor="#ffffff" stopOpacity="0.95" />
            <stop offset="100%" stopColor="#38bdf8" stopOpacity="0.8" />
          </linearGradient>
          <linearGradient id="spikedGradient2" x1="0%" y1="0%" x2="100%" y2="0%">
            <stop offset="0%" stopColor="#0284c7" stopOpacity="0.5" />
            <stop offset="50%" stopColor="#38bdf8" stopOpacity="0.7" />
            <stop offset="100%" stopColor="#0284c7" stopOpacity="0.5" />
          </linearGradient>
        </defs>

        <motion.path
          variants={secondarySpikeVariants}
          animate={isPlaying ? "active" : "idle"}
          fill="url(#spikedGradient2)"
        />

        <motion.path
          variants={spikedWaveVariants}
          animate={isPlaying ? "active" : "idle"}
          fill="url(#spikedGradient1)"
        />
      </svg>

      {/* Layer 2: Smooth Equalizer Spike Bars (Foreground Visualizer) */}
      <div className="relative z-10 w-full h-full flex items-center justify-between gap-1 sm:gap-1.5 px-2 sm:px-6">
        {spikeBars.map((bar) => (
          <div key={bar.id} className="relative flex-1 flex flex-col items-center justify-center h-full">
            {/* Upper Spike (Expands Upward from Center Axis) */}
            <motion.div
              animate={{
                height: isPlaying ? bar.activeHeights.map((h) => `${h / 2}px`) : `${bar.idleHeight / 2}px`,
              }}
              transition={{
                duration: bar.duration,
                repeat: Infinity,
                repeatType: "mirror",
                ease: "easeInOut",
                delay: bar.delay,
              }}
              className={`w-full max-w-[5px] rounded-t-full transition-colors ${
                bar.isAccent
                  ? "bg-gradient-to-t from-cyan-500 via-cyan-300 to-white shadow-[0_0_8px_rgba(56,189,248,0.8)]"
                  : "bg-gradient-to-t from-zinc-600 via-cyan-500 to-white shadow-[0_0_6px_rgba(255,255,255,0.5)]"
              }`}
            />

            {/* Micro Center Node */}
            <div
              className={`w-1 h-1 rounded-full my-[1px] ${
                bar.isAccent ? "bg-white shadow-[0_0_6px_#ffffff]" : "bg-cyan-400/80"
              }`}
            />

            {/* Lower Spike (Expands Downward from Center Axis - Mirrored) */}
            <motion.div
              animate={{
                height: isPlaying ? bar.activeHeights.map((h) => `${h / 2}px`) : `${bar.idleHeight / 2}px`,
              }}
              transition={{
                duration: bar.duration,
                repeat: Infinity,
                repeatType: "mirror",
                ease: "easeInOut",
                delay: bar.delay,
              }}
              className={`w-full max-w-[5px] rounded-b-full opacity-90 transition-colors ${
                bar.isAccent
                  ? "bg-gradient-to-b from-cyan-500 via-cyan-300 to-white shadow-[0_0_8px_rgba(56,189,248,0.8)]"
                  : "bg-gradient-to-b from-zinc-600 via-cyan-500 to-white shadow-[0_0_6px_rgba(255,255,255,0.5)]"
              }`}
            />
          </div>
        ))}
      </div>

      {/* Floating Audio Signal Particle Sparks */}
      {isPlaying && (
        <div className="absolute inset-0 flex items-center justify-around pointer-events-none z-20 px-8">
          {[0, 1, 2, 3, 4, 5].map((i) => (
            <motion.div
              key={i}
              animate={{
                y: [-12, 12, -12],
                x: [-4, 4, -4],
                opacity: [0.2, 1, 0.2],
                scale: [0.7, 1.3, 0.7],
              }}
              transition={{
                duration: 1.8 + i * 0.3,
                repeat: Infinity,
                ease: "easeInOut",
                delay: i * 0.2,
              }}
              className="w-1.5 h-1.5 rounded-full bg-cyan-300 shadow-[0_0_10px_#06b6d4]"
            />
          ))}
        </div>
      )}
    </div>
  );
}
