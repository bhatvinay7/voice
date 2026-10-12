"use client";

import React from "react";
import { BadgeCheck } from "lucide-react";
import { UserProfile } from "../../../types/user";

interface UserProfileCardProps {
  user: UserProfile | null;
  onSelectKyc?: () => void;
}

export function UserProfileCard({
  user,
  onSelectKyc,
}: UserProfileCardProps): React.ReactElement {
  return (
    <div className="relative overflow-hidden bg-theme-main p-4 rounded-2xl border border-theme-border dark:border-white/15 shadow-md dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.08)] group">
      <div className="flex items-center gap-3.5 relative z-10">
        <div className="relative">
          <div className="w-11 h-11 rounded-xl bg-gradient-to-tr from-white/30 via-white/15 to-transparent p-0.5 shadow-md">
            <div className="w-full h-full bg-theme-card rounded-[10px] flex items-center justify-center text-theme-text font-bold border border-theme-border dark:border-white/15 dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.15)]">
              {user?.name?.trim().charAt(0).toUpperCase() || "U"}
            </div>
          </div>
          <span
            className={`absolute -bottom-0.5 -right-0.5 w-3.5 h-3.5 rounded-full border-2 border-theme-main ${
              user?.is_verified ? "bg-emerald-500 animate-pulse" : "bg-zinc-600"
            }`}
          />
        </div>

        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-1.5">
            <h3 className="text-sm font-bold text-theme-text truncate tracking-tight">
              {user?.name || "Guest User"}
            </h3>
            {user?.is_verified && <BadgeCheck className="w-4 h-4 text-emerald-400 shrink-0" />}
          </div>
          <p className="text-[11px] text-theme-sub truncate font-mono mt-0.5">
            {user?.email || "user@estate.ae"}
          </p>
        </div>
      </div>

      <div className="mt-3.5 pt-3 border-t border-theme-border dark:border-white/10 flex items-center justify-between text-[11px] relative z-10">
        <span className="text-theme-sub font-mono">KYC Status:</span>
        {user?.is_verified ? (
          <span className="inline-flex items-center gap-1 text-emerald-400 font-bold bg-emerald-500/10 px-2.5 py-0.5 rounded-full border border-emerald-500/20 font-mono shadow-[inset_0_1px_0_0_rgba(255,255,255,0.08)]">
            <span className="w-1.5 h-1.5 rounded-full bg-emerald-400" />
            Verified
          </span>
        ) : (
          <button
            onClick={onSelectKyc}
            className="inline-flex items-center gap-1 text-theme-text font-bold bg-theme-card hover:bg-black/10 dark:hover:bg-white/10 px-2.5 py-0.5 rounded-full border border-theme-border dark:border-white/25 transition-colors font-mono shadow-[inset_0_1px_0_0_rgba(255,255,255,0.1)]"
          >
            Verify Now &rarr;
          </button>
        )}
      </div>
    </div>
  );
}
