"use client";

import React from "react";

interface AuthPasswordStrengthProps {
  score: number;
}

export function AuthPasswordStrength({
  score,
}: AuthPasswordStrengthProps): React.ReactElement {
  return (
    <div className="pt-2 space-y-1">
      <div className="flex items-center justify-between text-[11px]">
        <span className="text-theme-sub">Password Strength</span>
        <span className="font-bold text-cyan-400">
          {score <= 25 && "Weak"}
          {score === 50 && "Fair"}
          {score === 75 && "Good"}
          {score === 100 && "Strong 🔒"}
        </span>
      </div>
      <div className="h-1.5 w-full bg-theme-main border border-theme-border rounded-full overflow-hidden">
        <div
          className={`h-full transition-all duration-300 rounded-full ${
            score <= 25
              ? "bg-rose-500 w-1/4"
              : score === 50
              ? "bg-sky-400 w-2/4"
              : score === 75
              ? "bg-cyan-400 w-3/4"
              : "bg-emerald-400 w-full"
          }`}
        />
      </div>
    </div>
  );
}
