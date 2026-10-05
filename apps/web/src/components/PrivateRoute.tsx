"use client";

import React, { JSX } from "react";
import { UserProfile } from "../../types/user";
import { motion } from "framer-motion";
import { Lock, ShieldAlert } from "lucide-react";
import { useRouter } from "next/navigation";

interface PrivateRouteProps {
  user: UserProfile | null;
  requiredRole?: "user" | "admin";
  children: React.ReactNode;
}

export function PrivateRoute({
  user,
  requiredRole = "user",
  children,
}: PrivateRouteProps): React.ReactElement | null {
  const router = useRouter();
  if (!user) {
    return (
      <motion.div 
        initial={{ opacity: 0, scale: 0.95 }}
        animate={{ opacity: 1, scale: 1 }}
        className="bg-white border border-slate-200 rounded-3xl p-10 text-center space-y-6 max-w-md mx-auto my-20 shadow-xl shadow-slate-200/50"
      >
        <div className="w-16 h-16 bg-blue-50 text-blue-600 rounded-full flex items-center justify-center mx-auto border border-blue-200 shadow-sm">
          <Lock className="w-8 h-8" />
        </div>
        <div className="space-y-2">
          <h3 className="text-2xl font-bold text-slate-800 tracking-tight">Authentication Required</h3>
          <p className="text-sm text-slate-500 leading-relaxed">
            Please sign in to access this private section and interact with our AI voice agents.
          </p>
        </div>
        <button
          onClick={() => router.push("/auth")}
          className="w-full bg-slate-900 text-white font-bold text-sm px-6 py-3 rounded-full hover:bg-slate-800 transition-all shadow-sm"
        >
          Sign In to Continue
        </button>
      </motion.div>
    );
  }

  if (requiredRole === "admin" && user.role !== "admin") {
    return (
      <motion.div 
        initial={{ opacity: 0, scale: 0.95 }}
        animate={{ opacity: 1, scale: 1 }}
        className="bg-white border border-slate-200 rounded-3xl p-10 text-center space-y-6 max-w-md mx-auto my-20 shadow-xl shadow-slate-200/50"
      >
        <div className="w-16 h-16 bg-red-50 text-red-600 rounded-full flex items-center justify-center mx-auto border border-red-200 shadow-sm">
          <ShieldAlert className="w-8 h-8" />
        </div>
        <div className="space-y-2">
          <h3 className="text-2xl font-bold text-slate-800 tracking-tight">Admin Access Required</h3>
          <p className="text-sm text-slate-500 leading-relaxed">
            Your account ({user.email}) does not have administrative permissions to view this section.
          </p>
        </div>
      </motion.div>
    );
  }

  return <>{children}</>;
};
