"use client";

import React, { useEffect, useRef } from "react";

interface AudioSpectrumWaveProps {
  isCallActive: boolean;
  aiState: "idle" | "listening" | "thinking" | "speaking";
}

export function AudioSpectrumWave({
  isCallActive,
  aiState,
}: AudioSpectrumWaveProps): React.ReactElement {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    let animationFrameId: number;
    let phase = 0;

    const render = () => {
      ctx.clearRect(0, 0, canvas.width, canvas.height);
      const width = canvas.width;
      const height = canvas.height;
      const barCount = 32;
      const barWidth = (width / barCount) * 0.6;
      const gap = (width - barCount * barWidth) / (barCount + 1);

      phase += 0.08;

      for (let i = 0; i < barCount; i++) {
        let barHeight = 6;

        if (isCallActive) {
          if (aiState === "speaking") {
            barHeight = 12 + Math.sin(phase + i * 0.4) * 28 + Math.cos(phase * 0.7 + i * 0.2) * 15;
          } else if (aiState === "listening") {
            barHeight = 10 + Math.abs(Math.sin(phase * 0.5 + i * 0.3)) * 22;
          } else if (aiState === "thinking") {
            barHeight = 8 + Math.sin(phase + i * 0.6) * 12;
          }
        } else {
          barHeight = 4 + Math.sin(phase + i * 0.2) * 3;
        }

        barHeight = Math.max(4, Math.min(height * 0.85, Math.abs(barHeight)));

        const x = gap + i * (barWidth + gap);
        const y = (height - barHeight) / 2;

        const gradient = ctx.createLinearGradient(0, y, 0, y + barHeight);
        if (aiState === "speaking") {
          gradient.addColorStop(0, "#ffffff");
          gradient.addColorStop(0.5, "#38bdf8");
          gradient.addColorStop(1, "#0284c7");
        } else if (aiState === "listening") {
          gradient.addColorStop(0, "#10b981");
          gradient.addColorStop(0.5, "#06b6d4");
          gradient.addColorStop(1, "#38bdf8");
        } else if (aiState === "thinking") {
          gradient.addColorStop(0, "#818cf8");
          gradient.addColorStop(1, "#38bdf8");
        } else {
          gradient.addColorStop(0, "#475569");
          gradient.addColorStop(1, "#94a3b8");
        }

        ctx.fillStyle = gradient;
        ctx.beginPath();
        ctx.roundRect(x, y, barWidth, barHeight, 4);
        ctx.fill();
      }

      animationFrameId = requestAnimationFrame(render);
    };

    render();

    return () => {
      cancelAnimationFrame(animationFrameId);
    };
  }, [isCallActive, aiState]);

  return (
    <div className="flex items-center justify-center p-3 bg-theme-main/60 rounded-2xl border border-white/10 dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.06)]">
      <canvas ref={canvasRef} width={280} height={40} className="w-full max-w-[280px] h-10" />
    </div>
  );
}
