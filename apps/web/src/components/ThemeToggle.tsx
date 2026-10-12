"use client";

import React from "react";
import { useTheme } from "../context/ThemeContext";
import { Moon, Sun } from "lucide-react";

interface ThemeToggleProps {
  className?: string;
  showLabel?: boolean;
}

export function ThemeToggle({ className = "", showLabel = false }: ThemeToggleProps) {
  const { theme, toggleTheme } = useTheme();
  const isDark = theme === "dark";

  return (
    <button
      onClick={toggleTheme}
      aria-label="Toggle Dark / Light Theme"
      title={isDark ? "Switch to Light Mode" : "Switch to Dark Mode (x.ai Voice)"}
      className={`inline-flex items-center gap-2 px-3 py-1.5 rounded-full border border-theme-border dark:border-white/15 bg-theme-card text-theme-text hover:bg-theme-main transition-all shadow-sm dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.12)] active:scale-95 ${className}`}
    >
      {isDark ? (
        <>
          <Moon className="w-4 h-4 text-cyan-400" />
          {showLabel && <span className="text-xs font-semibold">Dark</span>}
        </>
      ) : (
        <>
          <Sun className="w-4 h-4 text-zinc-400" />
          {showLabel && <span className="text-xs font-semibold">Light</span>}
        </>
      )}
    </button>
  );
}
