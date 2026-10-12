"use client";

import React from "react";
import { Sparkles, BedDouble, Bath } from "lucide-react";
import { formatCurrency } from "../../lib/formatters";

export interface PropertySuggestion {
  id: string;
  title: string;
  description: string;
  price_aed: number;
  bedrooms: number;
  bathrooms: number;
  city?: string;
  images?: string[];
}

interface PropertySuggestionsGridProps {
  suggestions: PropertySuggestion[];
}

export function PropertySuggestionsGrid({
  suggestions,
}: PropertySuggestionsGridProps): React.ReactElement {
  if (suggestions.length === 0) return <></>;

  return (
    <div className="bg-theme-card border border-theme-border dark:border-white/15 rounded-3xl p-6 sm:p-8 space-y-5 shadow-xl transition-colors duration-300 dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.1)]">
      <div className="flex items-center justify-between border-b border-theme-border dark:border-white/10 pb-4">
        <div className="flex items-center gap-2.5">
          <div className="p-2 rounded-xl bg-cyan-500/10 text-cyan-400 border border-cyan-500/20">
            <Sparkles className="w-4 h-4" />
          </div>
          <div>
            <h3 className="text-base font-bold text-theme-text font-serif">
              Live AI Property Recommendations
            </h3>
            <p className="text-xs text-theme-sub font-mono">
              Dynamically pinned during your voice call session
            </p>
          </div>
        </div>
        <span className="text-[10px] font-mono text-cyan-400 bg-cyan-500/10 px-2.5 py-1 rounded-full border border-cyan-500/30 font-bold">
          {suggestions.length} Pinned
        </span>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
        {suggestions.map((prop) => (
          <div
            key={prop.id}
            className="p-5 rounded-2xl bg-theme-main border border-theme-border dark:border-white/10 hover:border-cyan-500/40 transition-all group space-y-3 shadow-md dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.06)]"
          >
            <div>
              <span className="text-[10px] font-mono text-cyan-400 font-bold uppercase tracking-wider">
                {prop.city || "Dubai"}
              </span>
              <h4 className="text-sm font-bold text-theme-text truncate group-hover:text-cyan-300 transition-colors mt-0.5">
                {prop.title}
              </h4>
            </div>

            <p className="text-xs text-theme-sub line-clamp-2 leading-relaxed">
              {prop.description}
            </p>

            <div className="flex items-center justify-between pt-2 border-t border-theme-border dark:border-white/10 text-xs">
              <span className="font-bold text-theme-text font-mono">
                {formatCurrency(prop.price_aed, { currency: "AED" })}
                <span className="text-[10px] text-theme-sub font-normal"> / yr</span>
              </span>
              <div className="flex items-center gap-3 text-theme-sub font-mono text-[11px]">
                <span className="flex items-center gap-1">
                  <BedDouble className="w-3.5 h-3.5 text-zinc-500" />
                  {prop.bedrooms}
                </span>
                <span className="flex items-center gap-1">
                  <Bath className="w-3.5 h-3.5 text-zinc-500" />
                  {prop.bathrooms}
                </span>
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
