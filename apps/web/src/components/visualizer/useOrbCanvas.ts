"use client";

import { useEffect, RefObject } from "react";

interface ConstellationParticle {
  x: number;
  y: number;
  vx: number;
  vy: number;
  radius: number;
  baseAlpha: number;
}

export function useOrbCanvas(
  canvasRef: RefObject<HTMLCanvasElement | null>,
  isCallActive: boolean,
  aiState: "idle" | "listening" | "thinking" | "speaking"
) {
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    let animationFrameId: number;
    let phase = 0;

    const particleCount = 38;
    const particles: ConstellationParticle[] = [];
    const orbRadius = 150;

    for (let i = 0; i < particleCount; i++) {
      const angle = Math.random() * Math.PI * 2;
      const dist = Math.sqrt(Math.random()) * (orbRadius - 15);
      particles.push({
        x: dist * Math.cos(angle),
        y: dist * Math.sin(angle),
        vx: (Math.random() - 0.5) * 0.35,
        vy: (Math.random() - 0.5) * 0.35,
        radius: Math.random() * 1.5 + 0.8,
        baseAlpha: Math.random() * 0.45 + 0.25,
      });
    }

    const render = () => {
      const rect = canvas.getBoundingClientRect();
      const dpr = typeof window !== "undefined" ? Math.min(window.devicePixelRatio || 1, 2) : 1;
      const targetWidth = Math.round(rect.width * dpr);
      const targetHeight = Math.round(rect.height * dpr);

      if (canvas.width !== targetWidth || canvas.height !== targetHeight) {
        canvas.width = targetWidth;
        canvas.height = targetHeight;
      }

      ctx.save();
      ctx.scale(dpr, dpr);

      const width = rect.width;
      const height = rect.height;
      const centerX = width / 2;
      const centerY = height / 2;
      const centerHoleRadius = 72;

      ctx.clearRect(0, 0, width, height);
      phase += 0.065;

      // 1. Constellation Network
      for (let i = 0; i < particles.length; i++) {
        const p = particles[i];
        p.x += p.vx;
        p.y += p.vy;

        const dist = Math.hypot(p.x, p.y);
        if (dist > orbRadius - 12) {
          p.vx *= -1;
          p.vy *= -1;
        }

        const px = centerX + p.x;
        const py = centerY + p.y;

        if (dist > centerHoleRadius - 5) {
          ctx.beginPath();
          ctx.fillStyle = `rgba(56, 189, 248, ${p.baseAlpha * (0.8 + 0.2 * Math.sin(phase + i))})`;
          ctx.arc(px, py, p.radius, 0, Math.PI * 2);
          ctx.fill();

          for (let j = i + 1; j < particles.length; j++) {
            const p2 = particles[j];
            const d2 = Math.hypot(p.x - p2.x, p.y - p2.y);
            if (d2 < 42) {
              const p2Dist = Math.hypot(p2.x, p2.y);
              if (p2Dist > centerHoleRadius - 5) {
                const lineAlpha = (1 - d2 / 42) * 0.22;
                ctx.beginPath();
                ctx.strokeStyle = `rgba(56, 189, 248, ${lineAlpha})`;
                ctx.lineWidth = 0.8;
                ctx.moveTo(px, py);
                ctx.lineTo(centerX + p2.x, centerY + p2.y);
                ctx.stroke();
              }
            }
          }
        }
      }

      // 2. Horizontal Glowing Carrier Beam
      const carrierGradient = ctx.createLinearGradient(0, centerY, width, centerY);
      carrierGradient.addColorStop(0, "rgba(56, 189, 248, 0)");
      carrierGradient.addColorStop(0.12, "rgba(56, 189, 248, 0.45)");
      carrierGradient.addColorStop(0.25, "rgba(56, 189, 248, 0.8)");
      carrierGradient.addColorStop(0.5, "rgba(255, 255, 255, 0.95)");
      carrierGradient.addColorStop(0.75, "rgba(56, 189, 248, 0.8)");
      carrierGradient.addColorStop(0.88, "rgba(56, 189, 248, 0.45)");
      carrierGradient.addColorStop(1, "rgba(56, 189, 248, 0)");

      ctx.beginPath();
      ctx.strokeStyle = "rgba(56, 189, 248, 0.25)";
      ctx.lineWidth = 4;
      ctx.moveTo(4, centerY);
      ctx.lineTo(width - 4, centerY);
      ctx.stroke();

      ctx.beginPath();
      ctx.strokeStyle = carrierGradient;
      ctx.lineWidth = 1.5;
      ctx.moveTo(6, centerY);
      ctx.lineTo(width - 6, centerY);
      ctx.stroke();

      // 3. Dynamic Voice Spikes (Left & Right Clusters)
      const spikesPerCluster = 38;

      const drawSpike = (x: number, amplitude: number, index: number) => {
        const topY = centerY - amplitude;
        const bottomY = centerY + amplitude;

        const spikeGradient = ctx.createLinearGradient(x, topY, x, bottomY);
        if (isCallActive && aiState === "speaking") {
          spikeGradient.addColorStop(0, "rgba(255, 255, 255, 0.98)");
          spikeGradient.addColorStop(0.2, "rgba(56, 189, 248, 0.9)");
          spikeGradient.addColorStop(0.5, "rgba(255, 255, 255, 1.0)");
          spikeGradient.addColorStop(0.8, "rgba(56, 189, 248, 0.9)");
          spikeGradient.addColorStop(1, "rgba(255, 255, 255, 0.98)");
        } else if (isCallActive && aiState === "listening") {
          spikeGradient.addColorStop(0, "rgba(255, 255, 255, 0.95)");
          spikeGradient.addColorStop(0.25, "rgba(52, 211, 153, 0.9)");
          spikeGradient.addColorStop(0.5, "rgba(255, 255, 255, 1.0)");
          spikeGradient.addColorStop(0.75, "rgba(56, 189, 248, 0.85)");
          spikeGradient.addColorStop(1, "rgba(52, 211, 153, 0.9)");
        } else if (isCallActive && aiState === "thinking") {
          spikeGradient.addColorStop(0, "rgba(167, 139, 250, 0.95)");
          spikeGradient.addColorStop(0.5, "rgba(255, 255, 255, 1.0)");
          spikeGradient.addColorStop(1, "rgba(56, 189, 248, 0.95)");
        } else {
          spikeGradient.addColorStop(0, "rgba(255, 255, 255, 0.75)");
          spikeGradient.addColorStop(0.5, "rgba(148, 163, 184, 0.85)");
          spikeGradient.addColorStop(1, "rgba(255, 255, 255, 0.75)");
        }

        ctx.beginPath();
        ctx.strokeStyle = spikeGradient;
        ctx.lineWidth = index % 3 === 0 ? 2.2 : 1.4;
        ctx.lineCap = "round";
        ctx.moveTo(x, topY);
        ctx.lineTo(x, bottomY);
        ctx.stroke();

        if (amplitude > 32) {
          ctx.beginPath();
          ctx.fillStyle = "#ffffff";
          ctx.arc(x, topY, 1.4, 0, Math.PI * 2);
          ctx.arc(x, bottomY, 1.4, 0, Math.PI * 2);
          ctx.fill();
        }
      };

      // LEFT CLUSTER
      const leftStartX = 14;
      const leftEndX = centerX - centerHoleRadius - 2;
      const leftWidth = leftEndX - leftStartX;

      for (let i = 0; i < spikesPerCluster; i++) {
        const norm = i / (spikesPerCluster - 1);
        const x = leftStartX + norm * leftWidth;
        const envelope = 0.14 + 0.86 * Math.sin(norm * Math.PI);

        let amplitude = 4;
        if (isCallActive) {
          if (aiState === "speaking") {
            const h1 = Math.sin(phase * 2.1 + i * 0.42);
            const h2 = Math.cos(phase * 3.3 - i * 0.28);
            const h3 = Math.sin(phase * 1.1 + i * 0.75);
            const voiceFlutter = Math.sin(phase * 5.2 + i * 1.2) * 6;
            amplitude = (Math.abs(h1) * 36 + Math.abs(h2) * 26 + Math.abs(h3) * 16 + voiceFlutter) * envelope;
          } else if (aiState === "listening") {
            const h = Math.sin(phase * 1.4 + i * 0.38);
            amplitude = (Math.abs(h) * 26 + 6) * envelope;
          } else if (aiState === "thinking") {
            const wave = Math.sin(phase * 2.4 - i * 0.3);
            amplitude = (Math.abs(wave) * 18 + 5) * envelope;
          }
        } else {
          const idleWave = Math.sin(phase * 0.75 + i * 0.22);
          amplitude = (Math.abs(idleWave) * 12 + 4) * envelope;
        }

        amplitude = Math.max(3, Math.min(height * 0.42, amplitude));
        drawSpike(x, amplitude, i);
      }

      // RIGHT CLUSTER
      const rightStartX = centerX + centerHoleRadius + 2;
      const rightEndX = width - 14;
      const rightWidth = rightEndX - rightStartX;

      for (let i = 0; i < spikesPerCluster; i++) {
        const norm = i / (spikesPerCluster - 1);
        const x = rightStartX + norm * rightWidth;
        const envelope = 0.14 + 0.86 * Math.sin(norm * Math.PI);

        let amplitude = 4;
        if (isCallActive) {
          if (aiState === "speaking") {
            const h1 = Math.sin(phase * 2.1 - i * 0.42 + 1.2);
            const h2 = Math.cos(phase * 3.3 + i * 0.28 - 0.8);
            const h3 = Math.sin(phase * 1.1 - i * 0.75 + 0.4);
            const voiceFlutter = Math.sin(phase * 5.2 - i * 1.2) * 6;
            amplitude = (Math.abs(h1) * 36 + Math.abs(h2) * 26 + Math.abs(h3) * 16 + voiceFlutter) * envelope;
          } else if (aiState === "listening") {
            const h = Math.sin(phase * 1.4 - i * 0.38 + 0.5);
            amplitude = (Math.abs(h) * 26 + 6) * envelope;
          } else if (aiState === "thinking") {
            const wave = Math.sin(phase * 2.4 + i * 0.3);
            amplitude = (Math.abs(wave) * 18 + 5) * envelope;
          }
        } else {
          const idleWave = Math.sin(phase * 0.75 - i * 0.22 + 1.0);
          amplitude = (Math.abs(idleWave) * 12 + 4) * envelope;
        }

        amplitude = Math.max(3, Math.min(height * 0.42, amplitude));
        drawSpike(x, amplitude, i);
      }

      ctx.restore();
      animationFrameId = requestAnimationFrame(render);
    };

    render();

    return () => {
      cancelAnimationFrame(animationFrameId);
    };
  }, [canvasRef, isCallActive, aiState]);
}
