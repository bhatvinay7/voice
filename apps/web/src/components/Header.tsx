"use client";

import React, { useState, useEffect, useRef } from "react";
import Link from "next/link";
import { usePathname, useRouter } from "next/navigation";
import { UserProfile } from "../../types/user";
import { useLogout } from "../lib/api";
import { motion, AnimatePresence } from "framer-motion";
import { ThemeToggle } from "./ThemeToggle";
import { getInitials } from "./ui/Avatar";
import { Menu, X, ChevronDown, LayoutDashboard, ShieldCheck, LogOut, BadgeCheck } from "lucide-react";

interface HeaderProps {
  user: UserProfile | null;
}

const PLACEHOLDER_NAMES = new Set(["user", "admin", "administrator"]);

function displayName(user: UserProfile): string {
  const name = user.name?.trim() ?? "";
  if (name && !PLACEHOLDER_NAMES.has(name.toLowerCase())) return name;
  return user.email?.split("@")[0] ?? "";
}

function firstName(user: UserProfile): string {
  return displayName(user).split(/\s+/)[0];
}

export function Header({
  user,
}: HeaderProps): React.ReactElement {
  const pathname = usePathname();
  const router = useRouter();
  const [scrolled, setScrolled] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const logoutMutation = useLogout();
  const [accountOpen, setAccountOpen] = useState(false);
  const accountRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!accountOpen) return;
    const onDown = (e: MouseEvent) => {
      if (accountRef.current && !accountRef.current.contains(e.target as Node)) setAccountOpen(false);
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setAccountOpen(false);
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [accountOpen]);

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
      className={`fixed top-0 left-0 right-0 z-50 min-h-[72px] transition-all duration-300 ${
        scrolled 
          ? "bg-theme-card/90 backdrop-blur-xl border-b border-theme-border dark:border-white/15 shadow-lg dark:shadow-[inset_0_-1px_0_0_rgba(255,255,255,0.1)]" 
          : "bg-transparent border-b border-transparent"
      } text-theme-text px-4 sm:px-6 py-3 sm:py-4 flex items-center justify-between gap-3 md:grid md:grid-cols-[1fr_auto_1fr] md:gap-8`}
    >
      <Link href="/" className="flex min-w-0 items-center gap-2.5 sm:gap-3 cursor-pointer group">
        <div className="brand-mark w-9 h-9 sm:w-10 sm:h-10 rounded-xl flex items-center justify-center font-black text-sm shadow-md group-hover:scale-105 transition-transform duration-300">
          <span>dw</span>
        </div>
        <div>
          <h1 className="font-extrabold text-lg sm:text-xl tracking-tight text-theme-text">
            dwin
          </h1>
          <p className="text-[10px] text-theme-sub font-mono tracking-widest uppercase font-semibold">Real Estate AI</p>
        </div>
      </Link>

      <nav className="hidden md:flex items-center gap-6 justify-self-center">
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

      <div className="flex items-center gap-2 sm:gap-3 md:justify-self-end">
        {/* Dark / Light Mode Switcher */}
        <ThemeToggle showLabel={false} />

        {/* Auth User Info */}
        <AnimatePresence mode="wait">
          {user ? (
            <motion.div
              key="account"
              ref={accountRef}
              initial={{ opacity: 0, scale: 0.9 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.9 }}
              className="relative"
            >
              <button
                id="header-account-button"
                type="button"
                aria-haspopup="menu"
                aria-expanded={accountOpen}
                onClick={() => setAccountOpen((o) => !o)}
                className="group flex items-center gap-2.5 rounded-full border border-theme-border bg-theme-card/80 backdrop-blur py-1 pl-1 pr-2.5 shadow-sm transition-all hover:shadow-md hover:border-sky-400/50"
              >
                <span className="relative">
                  <span className="flex h-8 w-8 items-center justify-center rounded-full bg-gradient-to-br from-sky-500 to-indigo-500 text-xs font-bold text-white ring-2 ring-white/70 dark:ring-white/10">
                    {getInitials(user.name, user.email)}
                  </span>
                  <span className={`absolute -bottom-0.5 -right-0.5 h-2.5 w-2.5 rounded-full ring-2 ring-theme-card ${user.is_verified || user.role === "admin" ? "bg-emerald-500" : "bg-amber-400"}`} />
                </span>
                <span className="hidden lg:block max-w-[120px] text-left">
                  <span className="block truncate text-xs font-semibold text-theme-text leading-tight">{firstName(user)}</span>
                  <span className="block text-[10px] text-theme-sub leading-tight">
                    {user.role === "admin" ? "Administrator" : user.is_verified ? "Verified" : "Unverified"}
                  </span>
                </span>
                <ChevronDown className={`h-3.5 w-3.5 text-theme-sub transition-transform duration-200 ${accountOpen ? "rotate-180" : ""}`} />
              </button>

              <AnimatePresence>
                {accountOpen && (
                  <motion.div
                    role="menu"
                    initial={{ opacity: 0, y: -6, scale: 0.97 }}
                    animate={{ opacity: 1, y: 0, scale: 1 }}
                    exit={{ opacity: 0, y: -6, scale: 0.97 }}
                    transition={{ duration: 0.16 }}
                    className="absolute right-0 mt-2 w-64 origin-top-right overflow-hidden rounded-2xl border border-theme-border bg-theme-card/95 backdrop-blur-xl shadow-2xl"
                  >
                    <div className="flex items-center gap-3 p-4 border-b border-theme-border">
                      <span className="flex h-10 w-10 shrink-0 items-center justify-center rounded-full bg-gradient-to-br from-sky-500 to-indigo-500 text-sm font-bold text-white">
                        {getInitials(user.name, user.email)}
                      </span>
                      <div className="min-w-0">
                        <p className="truncate text-sm font-semibold text-theme-text">{displayName(user)}</p>
                        <p className="truncate text-xs text-theme-sub">{user.email}</p>
                        <span className={`mt-1 inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-semibold ${user.role === "admin" ? "bg-indigo-500/10 text-indigo-500" : user.is_verified ? "bg-emerald-500/10 text-emerald-600" : "bg-amber-500/10 text-amber-600"}`}>
                          <BadgeCheck className="h-3 w-3" />
                          {user.role === "admin" ? "Admin" : user.is_verified ? "Verified" : "Unverified"}
                        </span>
                      </div>
                    </div>
                    <div className="p-1.5">
                      <button role="menuitem" onClick={() => { setAccountOpen(false); router.push("/dashboard"); }} className="flex w-full items-center gap-3 rounded-xl px-3 py-2 text-sm text-theme-text hover:bg-theme-card-hover transition">
                        <LayoutDashboard className="h-4 w-4 text-theme-sub" /> Dashboard
                      </button>
                      {user.role === "admin" && (
                        <button role="menuitem" onClick={() => { setAccountOpen(false); router.push("/admin"); }} className="flex w-full items-center gap-3 rounded-xl px-3 py-2 text-sm text-theme-text hover:bg-theme-card-hover transition">
                          <ShieldCheck className="h-4 w-4 text-theme-sub" /> Admin Console
                        </button>
                      )}
                    </div>
                    <div className="p-1.5 border-t border-theme-border">
                      <button id="header-logout-button" role="menuitem" onClick={() => { setAccountOpen(false); logoutMutation.mutate(); }} className="flex w-full items-center gap-3 rounded-xl px-3 py-2 text-sm font-medium text-rose-500 hover:bg-rose-500/10 transition">
                        <LogOut className="h-4 w-4" /> Sign out
                      </button>
                    </div>
                  </motion.div>
                )}
              </AnimatePresence>
            </motion.div>
          ) : (
            <motion.button
              initial={{ opacity: 0, scale: 0.9 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.9 }}
              onClick={() => router.push("/auth")}
              className="btn-blue relative inline-flex h-9 items-center justify-center overflow-hidden rounded-full hover:scale-105 active:scale-95 focus:outline-none"
            >
              <span className="text-xs font-bold tracking-wide">Sign In</span>
            </motion.button>
          )}
        </AnimatePresence>
        <button
          type="button"
          className="md:hidden inline-flex h-9 w-9 items-center justify-center rounded-full border border-theme-border bg-theme-card text-theme-text"
          aria-label="Toggle navigation"
          onClick={() => setMenuOpen((open) => !open)}
        >
          {menuOpen ? <X className="w-4 h-4" /> : <Menu className="w-4 h-4" />}
        </button>
      </div>
      {menuOpen && (
        <div className="absolute top-full left-3 right-3 mt-2 rounded-2xl border border-theme-border bg-theme-card p-3 shadow-xl md:hidden">
          <div className="grid grid-cols-2 gap-2 text-sm font-semibold">
            <Link onClick={() => setMenuOpen(false)} className="rounded-xl px-3 py-2 hover:bg-theme-card-hover" href="/">Home</Link>
            <Link onClick={() => setMenuOpen(false)} className="rounded-xl px-3 py-2 hover:bg-theme-card-hover" href="/dashboard">Dashboard</Link>
            {user?.role === "admin" && <Link onClick={() => setMenuOpen(false)} className="rounded-xl px-3 py-2 hover:bg-theme-card-hover" href="/admin">Admin</Link>}
            {user && <button onClick={() => logoutMutation.mutate()} className="rounded-xl px-3 py-2 text-left text-rose-500 hover:bg-rose-500/10">Logout</button>}
          </div>
        </div>
      )}
    </motion.header>
  );
}
