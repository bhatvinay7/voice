import React from "react";
import Link from "next/link";

export default function NotFound() {
  return (
    <div className="min-h-screen bg-[#050505] text-[#ffffff] flex flex-col items-center justify-center p-6 text-center font-sans selection:bg-white/20 selection:text-white">
      <div className="absolute inset-0 bg-[radial-gradient(ellipse_at_center,_var(--tw-gradient-stops))] from-cyan-500/10 via-transparent to-transparent blur-3xl pointer-events-none" />
      <div className="relative z-10 p-8 rounded-2xl bg-[#09090b] border border-white/15 max-w-md w-full shadow-2xl backdrop-blur-xl shadow-[inset_0_1px_0_0_rgba(255,255,255,0.15)]">
        <h1 className="font-mono text-7xl font-extrabold text-transparent bg-clip-text bg-gradient-to-r from-cyan-400 via-white to-cyan-200 mb-2">404</h1>
        <h2 className="text-xl font-bold text-[#ffffff] mb-3">Page Not Found</h2>
        <p className="text-sm text-zinc-400 mb-6 leading-relaxed">
          The voice interface or page you requested could not be located.
        </p>
        <Link
          href="/"
          className="inline-flex items-center justify-center px-6 py-3 bg-white hover:bg-zinc-200 text-black font-semibold text-xs tracking-wider uppercase rounded-xl transition-all duration-200 shadow-lg shadow-white/10 transform hover:-translate-y-0.5 border border-white/30"
        >
          Return Home
        </Link>
      </div>
    </div>
  );
}
