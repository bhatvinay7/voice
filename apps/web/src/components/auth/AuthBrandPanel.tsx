"use client";

import React from "react";
import { motion } from "framer-motion";
import { CheckCircle2, Quote } from "lucide-react";
import type { AuthShowcaseContent } from "../../../types/auth-layout";

interface AuthBrandPanelProps {
  content: AuthShowcaseContent;
}

function renderHeadline(headline: string, accentWord: string): React.ReactNode {
  const index = headline.indexOf(accentWord);
  if (!accentWord || index < 0) return headline;
  return (
    <>
      {headline.slice(0, index)}
      <span className="font-serif italic font-normal text-gradient-accent">{accentWord}</span>
      {headline.slice(index + accentWord.length)}
    </>
  );
}

export function AuthBrandPanel({ content }: AuthBrandPanelProps): React.ReactElement {
  return (
    <aside className="auth-brand-panel relative hidden overflow-hidden lg:flex lg:flex-col lg:justify-between p-12 xl:p-16">
      <div className="surface-grid pointer-events-none absolute inset-0" aria-hidden />
      <div className="pointer-events-none absolute -left-24 top-1/3 h-96 w-96 rounded-full bg-cyan-500/10 blur-[120px]" aria-hidden />

      <div className="relative z-10 flex items-center gap-2.5">
        <span className="brand-mark flex h-9 w-9 items-center justify-center rounded-xl text-sm font-bold">D</span>
        <span className="font-display text-lg font-semibold tracking-tight">DWIN</span>
      </div>

      <motion.div
        initial={{ opacity: 0, y: 16 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.5 }}
        className="relative z-10 max-w-xl"
      >
        <span className="eyebrow-pill">{content.eyebrow}</span>
        <h2 className="mt-6 font-display text-4xl font-semibold leading-[1.1] tracking-tight xl:text-5xl">
          {renderHeadline(content.headline, content.accentWord)}
        </h2>
        <p className="mt-5 text-base leading-7 text-theme-sub">{content.description}</p>

        <ul className="mt-8 space-y-4">
          {content.highlights.map((item) => (
            <li key={item.title} className="flex gap-3">
              <CheckCircle2 className="mt-0.5 h-5 w-5 shrink-0 text-cyan-400" />
              <div>
                <p className="text-sm font-semibold">{item.title}</p>
                <p className="text-sm text-theme-sub">{item.description}</p>
              </div>
            </li>
          ))}
        </ul>

        <dl className="mt-10 grid grid-cols-3 gap-3">
          {content.stats.map((stat) => (
            <div key={stat.label} className="stat-tile">
              <dt className="text-xs text-theme-sub">{stat.label}</dt>
              <dd className="mt-1 font-display text-2xl font-semibold">{stat.value}</dd>
            </div>
          ))}
        </dl>
      </motion.div>

      {content.quote ? (
        <figure className="relative z-10 max-w-xl rounded-2xl border border-theme-border bg-theme-card-hover/60 p-5">
          <Quote className="h-5 w-5 text-cyan-400" />
          <blockquote className="mt-3 text-sm leading-6">{content.quote.text}</blockquote>
          <figcaption className="mt-3 text-xs text-theme-sub">
            <span className="font-semibold text-theme-text">{content.quote.author}</span> · {content.quote.role}
          </figcaption>
        </figure>
      ) : (
        <span />
      )}
    </aside>
  );
}
