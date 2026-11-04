"use client";

import React, { useEffect, useState } from "react";
import Link from "next/link";
import { motion, AnimatePresence } from "framer-motion";
import { ArrowLeft, Database, FileCheck, Headphones, Layers, Menu, PanelLeftClose, ScrollText, Workflow, X } from "lucide-react";
import { AdminTab, AdminNavItem } from "../../../types/admin";
import { UserProfile } from "../../../types/user";
import { Avatar } from "../ui/Avatar";

const PLACEHOLDER_NAMES = new Set(["user", "admin", "administrator", "guest"]);
function displayName(user: UserProfile | null): string {
  const name = user?.name?.trim() ?? "";
  if (name && !PLACEHOLDER_NAMES.has(name.toLowerCase())) return name;
  return user?.email?.split("@")[0] ?? "Administrator";
}

interface AdminSidebarProps {
  activeTab: AdminTab;
  onSelectTab: (tab: AdminTab) => void;
  user: UserProfile | null;
}

interface NavGroup {
  title: string;
  items: AdminNavItem[];
}

const NAV_GROUPS: NavGroup[] = [
  {
    title: "General",
    items: [{ id: "all", label: "Overview", shortLabel: "Overview", icon: Layers, description: "Everything at a glance" }],
  },
  {
    title: "Customers",
    items: [
      { id: "calls", label: "Call logs", shortLabel: "Calls", icon: Headphones, description: "Recordings, summaries, conversations" },
      { id: "contracts", label: "Contracts", shortLabel: "Contracts", icon: FileCheck, description: "Review tenancy agreements" },
    ],
  },
  {
    title: "Content",
    items: [
      { id: "ingestion", label: "Listings & knowledge", shortLabel: "Listings", icon: Database, description: "Upload and manage inventory" },
      { id: "workflows", label: "Conversation flows", shortLabel: "Flows", icon: Workflow, description: "Assistant journey steps" },
    ],
  },
  {
    title: "System",
    items: [{ id: "logs", label: "Activity log", shortLabel: "Activity", icon: ScrollText, description: "Recent platform events" }],
  },
];

export const ADMIN_NAV_ITEMS: AdminNavItem[] = NAV_GROUPS.flatMap((group) => group.items);

export function AdminSidebar({ activeTab, onSelectTab, user }: AdminSidebarProps): React.ReactElement {
  const [mobileOpen, setMobileOpen] = useState(false);
  const [collapsed, setCollapsed] = useState(false);
  const activeItem = ADMIN_NAV_ITEMS.find((item) => item.id === activeTab);

  useEffect(() => {
    setCollapsed(localStorage.getItem("admin_sidebar_collapsed") === "1");
  }, []);

  useEffect(() => {
    if (!mobileOpen) return;
    const onKey = (event: KeyboardEvent) => event.key === "Escape" && setMobileOpen(false);
    document.addEventListener("keydown", onKey);
    document.body.style.overflow = "hidden";
    return () => {
      document.removeEventListener("keydown", onKey);
      document.body.style.overflow = "";
    };
  }, [mobileOpen]);

  const toggleCollapsed = () => {
    setCollapsed((value) => {
      localStorage.setItem("admin_sidebar_collapsed", value ? "0" : "1");
      return !value;
    });
  };

  const selectTab = (tab: AdminTab) => {
    onSelectTab(tab);
    setMobileOpen(false);
  };

  const renderContent = (compact: boolean, onClose?: () => void) => (
    <div className="flex h-full min-h-0 flex-col">
      <div className={`flex items-center gap-2 ${compact ? "flex-col" : ""}`}>
        <div className={`flex min-w-0 flex-1 items-center gap-3 rounded-2xl border border-theme-border bg-theme-card-hover ${compact ? "justify-center p-2" : "p-3"}`}>
          <Avatar name={user?.name} email={user?.email} />
          {!compact && (
            <div className="min-w-0 flex-1">
              <p className="truncate text-sm font-semibold text-theme-text">{displayName(user)}</p>
              <p className="truncate text-[11px] text-theme-sub">{user?.email || "Administrator"}</p>
            </div>
          )}
        </div>
        {onClose && (
          <button
            type="button"
            onClick={onClose}
            aria-label={compact ? "Expand sidebar" : "Close sidebar"}
            title={compact ? "Expand sidebar" : "Close sidebar"}
            className="page-btn h-9 w-9 shrink-0"
          >
            {compact ? <Menu className="h-4 w-4" /> : <PanelLeftClose className="h-4 w-4" />}
          </button>
        )}
      </div>

      <nav aria-label="Admin sections" className="mt-5 min-h-0 flex-1 space-y-5 overflow-y-auto overflow-x-hidden pr-1">
        {NAV_GROUPS.map((group) => (
          <div key={group.title}>
            {compact ? (
              <div className="mx-auto mb-2 h-px w-6 bg-theme-border" />
            ) : (
              <p className="mb-1.5 px-3 text-[10px] font-semibold uppercase tracking-[0.14em] text-theme-sub">{group.title}</p>
            )}
            <ul className="space-y-1">
              {group.items.map((item) => {
                const Icon = item.icon;
                const isActive = activeTab === item.id;
                return (
                  <li key={item.id}>
                    <button
                      id={`admin-nav-${item.id}${compact ? "-rail" : ""}`}
                      type="button"
                      onClick={() => selectTab(item.id)}
                      aria-current={isActive ? "page" : undefined}
                      title={compact ? item.label : undefined}
                      className={`relative flex w-full items-center rounded-xl text-left transition-colors ${
                        compact ? "justify-center p-3" : "gap-3 px-3 py-2.5"
                      } ${isActive ? "bg-theme-card-hover text-theme-text" : "text-theme-sub hover:bg-theme-card-hover/60 hover:text-theme-text"}`}
                    >
                      {isActive && (
                        <motion.span
                          layoutId={compact ? "admin-nav-indicator-rail" : "admin-nav-indicator"}
                          className="absolute left-0 top-1/2 h-5 w-1 -translate-y-1/2 rounded-r-full bg-cyan-400"
                        />
                      )}
                      <Icon className={`h-4 w-4 shrink-0 ${isActive ? "text-cyan-400" : ""}`} />
                      {!compact && (
                        <span className="min-w-0">
                          <span className="block truncate text-sm font-medium">{item.label}</span>
                          <span className="block truncate text-[11px] text-theme-sub">{item.description}</span>
                        </span>
                      )}
                    </button>
                  </li>
                );
              })}
            </ul>
          </div>
        ))}
      </nav>

      <Link
        href="/dashboard"
        title={compact ? "Back to my dashboard" : undefined}
        className="mt-4 inline-flex items-center justify-center gap-2 rounded-xl border border-theme-border px-3 py-2.5 text-xs font-semibold text-theme-text transition-colors hover:bg-theme-card-hover"
      >
        <ArrowLeft className="h-3.5 w-3.5" />
        {!compact && "Back to my dashboard"}
      </Link>
    </div>
  );

  return (
    <>
      {/* Mobile / tablet top bar */}
      <div className="sticky top-[76px] z-30 mb-4 flex w-full items-center gap-3 rounded-2xl border border-theme-border bg-theme-card/90 p-3 backdrop-blur-xl lg:hidden">
        <button
          id="admin-nav-toggle"
          type="button"
          onClick={() => setMobileOpen(true)}
          aria-expanded={mobileOpen}
          aria-label="Open menu"
          className="page-btn h-10 w-10"
        >
          <Menu className="h-4 w-4" />
        </button>
        <div className="min-w-0">
          <p className="text-[10px] uppercase tracking-[0.14em] text-theme-sub">Admin</p>
          <p className="truncate text-sm font-semibold text-theme-text">{activeItem?.label}</p>
        </div>
      </div>

      {/* Mobile / tablet drawer */}
      <AnimatePresence>
        {mobileOpen && (
          <>
            <motion.div
              key="admin-drawer-backdrop"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              onClick={() => setMobileOpen(false)}
              className="fixed inset-0 z-[70] bg-black/60 backdrop-blur-sm lg:hidden"
            />
            <motion.aside
              key="admin-drawer"
              initial={{ x: "-100%" }}
              animate={{ x: 0 }}
              exit={{ x: "-100%" }}
              transition={{ type: "spring", stiffness: 380, damping: 36 }}
              className="fixed inset-y-0 left-0 z-[80] w-[85vw] max-w-xs border-r border-theme-border bg-theme-card p-4 shadow-2xl lg:hidden"
            >
              <button
                type="button"
                onClick={() => setMobileOpen(false)}
                aria-label="Close menu"
                className="page-btn absolute right-3 top-3 h-9 w-9"
              >
                <X className="h-4 w-4" />
              </button>
              <div className="h-full pt-10">{renderContent(false)}</div>
            </motion.aside>
          </>
        )}
      </AnimatePresence>

      {/* Desktop sticky sidebar */}
      <aside
        className={`sticky top-[84px] z-20 hidden h-[calc(100vh-100px)] shrink-0 self-start transition-[width] duration-300 lg:block ${
          collapsed ? "w-[76px]" : "w-64 xl:w-72"
        }`}
      >
        <div className={`flex h-full flex-col rounded-2xl border border-theme-border bg-theme-card ${collapsed ? "p-2.5" : "p-4"}`}>
          {renderContent(collapsed, toggleCollapsed)}
        </div>
      </aside>
    </>
  );
}
