"use client";

import React, { useState, useEffect } from "react";
import Link from "next/link";
import { usePathname, useRouter } from "next/navigation";
import { UserProfile } from "../../types/user";
import { useLogout } from "../lib/api";
import { motion, AnimatePresence } from "framer-motion";
import { ThemeToggle } from "./ThemeToggle";

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
  const logoutMutation = useLogout();

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
          ? "bg-theme-card/90 backdrop-blur-xl border-b border-theme-border dark:border-white/15 shadow-lg dark:shadow-[inset_0_-1px_0_0_rgba(255,255,255,0.1)]" 
          : "bg-transparent border-b border-transparent"
      } text-theme-text px-6 py-4 flex items-center justify-between`}
    >
      <Link href="/" className="flex items-center space-x-3 cursor-pointer group">
        <div className="w-10 h-10 rounded-xl bg-theme-text flex items-center justify-center font-black text-xl shadow-md group-hover:scale-105 transition-transform duration-300 dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.2)]">
          <span className="text-theme-main">dw</span>
        </div>
        <div>
          <h1 className="font-extrabold text-xl tracking-tight text-theme-text">
            dwin
          </h1>
          <p className="text-[10px] text-theme-sub font-mono tracking-widest uppercase font-semibold">Real Estate AI</p>
        </div>
      </Link>

      <nav className="flex items-center gap-4 sm:gap-6">
        <Link
          href="/"
          className={`text-xs sm:text-sm transition-all duration-200 ${
            pathname === "/" ? "text-theme-accent font-extrabold" : "text-theme-sub font-medium hover:text-theme-text"
          }`}
        >
          Home
        </Link>
        <Link
          href="/dashboard"
          className={`text-xs sm:text-sm transition-all duration-200 ${
            pathname === "/dashboard" ? "text-theme-accent font-extrabold" : "text-theme-sub font-medium hover:text-theme-text"
          }`}
        >
          Dashboard
        </Link>
      </nav>

      <div className="flex items-center space-x-3 sm:space-x-4">
        {/* Dark / Light Mode Switcher */}
        <ThemeToggle showLabel={false} />

        {/* Auth User Info */}
        <AnimatePresence mode="wait">
          {user ? (
            <motion.div 
              initial={{ opacity: 0, scale: 0.9 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.9 }}
              className="flex items-center space-x-3 bg-theme-card pl-2 pr-4 py-1.5 rounded-full border border-theme-border dark:border-white/15 shadow-sm dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.1)]"
            >
              <div className="w-8 h-8 rounded-full bg-gradient-to-tr from-emerald-500 to-teal-400 flex items-center justify-center text-xs font-bold text-white shadow-inner dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.2)]">
                {user.name?.trim().charAt(0).toUpperCase() || "U"}
              </div>
              <div className="text-left hidden sm:block">
                <p className="text-xs font-bold text-theme-text leading-tight">{user.name}</p>
                <p className="text-[9px] text-emerald-500 font-mono tracking-wider uppercase font-semibold">
                  {user.role === "admin" ? "Admin" : (user.is_verified ? "Verified" : "Unverified")}
                </p>
              </div>
              {user.role === "admin" && (
                <button
                  onClick={() => router.push("/admin")}
                  className="ml-2 text-[10px] bg-theme-main text-theme-text border border-theme-border dark:border-white/15 px-2.5 py-1 rounded-full shadow-sm hover:bg-theme-card-hover transition font-bold dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.08)]"
                >
                  Admin
                </button>
              )}
              <button
                onClick={() => {
                  logoutMutation.mutate();
                }}
                className="ml-2 text-[10px] text-rose-500 hover:text-rose-600 font-bold uppercase tracking-wider"
              >
                Logout
              </button>
            </motion.div>
          ) : (
            <motion.button
              initial={{ opacity: 0, scale: 0.9 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.9 }}
              onClick={() => router.push("/auth")}
              className="relative inline-flex h-9 items-center justify-center overflow-hidden rounded-full bg-theme-text px-6 font-bold text-theme-main transition-all hover:opacity-90 hover:scale-105 active:scale-95 focus:outline-none shadow-sm dark:shadow-[inset_0_1px_0_0_rgba(255,255,255,0.2)]"
            >
              <span className="text-xs font-bold tracking-wide">Sign In</span>
            </motion.button>
          )}
        </AnimatePresence>
      </div>
    </motion.header>
  );
}
