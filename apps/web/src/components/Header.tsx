"use client";

import React, { useState, useEffect } from "react";
import Link from "next/link";
import { usePathname, useRouter } from "next/navigation";
import { UserProfile } from "../../types/user";
import { motion, AnimatePresence } from "framer-motion";
import { Bot, Mic, MicOff } from "lucide-react";

interface HeaderProps {
  user: UserProfile | null;
  agentModeActive: boolean;
  onToggleAgentMode: (active: boolean) => void;
}

export function Header({
  user,
  agentModeActive,
  onToggleAgentMode,
}: HeaderProps): React.ReactElement {
  const pathname = usePathname();
  const router = useRouter();
  const [scrolled, setScrolled] = useState(false);

  useEffect(() => {
    const handleScroll = () => {
      setScrolled(window.scrollY > 20);
    };
    window.addEventListener("scroll", handleScroll);
    return () => window.removeEventListener("scroll", handleScroll);
  }, []);



  return (
    <motion.header 
      initial={{ y: -100 }}
      animate={{ y: 0 }}
      transition={{ duration: 0.5 }}
      className={`fixed top-0 left-0 right-0 z-50 transition-all duration-300 ${
        scrolled 
          ? "bg-white/80 backdrop-blur-xl border-b border-slate-200 shadow-sm" 
          : "bg-transparent border-b border-transparent"
      } text-slate-800 px-6 py-4 flex items-center justify-between`}
    >
      <Link href="/" className="flex items-center space-x-3 cursor-pointer group">
        <div className="w-10 h-10 rounded-sm bg-slate-900 flex items-center justify-center font-bold text-xl shadow-sm group-hover:scale-105 transition-transform duration-300">
          <span className="text-white">AE</span>
        </div>
        <div>
          <h1 className="font-extrabold text-lg tracking-wide text-slate-900 type-mono-label">
            Estate AI
          </h1>
          <p className="text-[10px] text-indigo-600 font-mono tracking-widest uppercase">Autonomous Platform</p>
        </div>
      </Link>

      <nav className="hidden md:flex items-center space-x-1 bg-white p-1 rounded-sm border border-slate-200 shadow-sm type-mono-label">
        <Link
          href="/"
          className={`px-5 py-2 text-sm font-bold transition-all duration-300 ${
            pathname === "/" ? "bg-indigo-50 text-indigo-700 shadow-sm border border-indigo-100" : "text-slate-500 hover:text-slate-900 hover:bg-slate-50"
          }`}
        >
          Home
        </Link>
        <Link
          href="/dashboard"
          className={`px-5 py-2 text-sm font-bold transition-all duration-300 ${
            pathname === "/dashboard" ? "bg-indigo-50 text-indigo-700 shadow-sm border border-indigo-100" : "text-slate-500 hover:text-slate-900 hover:bg-slate-50"
          }`}
        >
          Dashboard
        </Link>
      </nav>

      <div className="flex items-center space-x-4 md:space-x-6">


        {/* Auth User Info */}
        <AnimatePresence mode="wait">
          {user ? (
            <motion.div 
              initial={{ opacity: 0, scale: 0.9 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.9 }}
              className="flex items-center space-x-3 bg-slate-100/80 pl-2 pr-4 py-1.5 rounded-full border border-slate-200"
            >
              <div className="w-8 h-8 rounded-full bg-gradient-to-br from-emerald-400 to-emerald-600 flex items-center justify-center text-xs font-bold text-white shadow-inner">
                {user.name.charAt(0)}
              </div>
              <div className="text-left hidden sm:block">
                <p className="text-xs font-semibold text-slate-800 leading-tight">{user.name}</p>
                <p className="text-[9px] text-emerald-600 font-mono tracking-wider uppercase">
                  {user.is_verified ? "Verified" : "Unverified"}
                </p>
              </div>
            </motion.div>
          ) : (
            <motion.button
              initial={{ opacity: 0, scale: 0.9 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.9 }}
              onClick={() => router.push("/auth")}
              className="relative inline-flex h-9 items-center justify-center overflow-hidden rounded-full bg-slate-900 px-6 font-medium text-white transition-all hover:bg-slate-800 hover:scale-105 active:scale-95 focus:outline-none focus:ring-2 focus:ring-slate-900/50 shadow-sm"
            >
              <span className="text-xs font-semibold tracking-wide">Sign In</span>
            </motion.button>
          )}
        </AnimatePresence>
      </div>
    </motion.header>
  );
};
