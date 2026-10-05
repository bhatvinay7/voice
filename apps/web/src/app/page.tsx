"use client";

import React, { useState } from "react";
import Link from "next/link";
import { Header } from "../components/Header";
import { useUserProfile } from "../lib/api";
import { motion } from "framer-motion";
import { Mic, Shield, Zap, Search, ArrowRight, Play, CheckCircle2 } from "lucide-react";

export default function HomePage() {
  const [agentModeActive, setAgentModeActive] = useState(true);
  const { data: userData, isLoading: userLoading } = useUserProfile();

  const user = userData?.user || null;

  return (
    <div className="min-h-screen bg-slate-50 text-slate-900 font-sans antialiased overflow-hidden selection:bg-indigo-200 selection:text-indigo-950 neo-grid">
      {/* Dynamic Background */}
      <div className="fixed inset-0 z-0 pointer-events-none">
        <div className="absolute top-[0%] left-[50%] -translate-x-1/2 w-[80%] h-[50%] rounded-full bg-indigo-100/50 blur-[120px]" />
      </div>

      <div className="relative z-10 flex flex-col min-h-screen">
        <Header
          user={user}
          agentModeActive={agentModeActive}
          onToggleAgentMode={setAgentModeActive}
        />

        <main className="flex-1 max-w-7xl mx-auto px-6 py-12 md:py-20 w-full space-y-24">
          {/* Hero Section */}
          <section className="relative flex flex-col items-center text-center space-y-8 pt-10">
            <motion.div
              initial={{ opacity: 0, y: 20 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.5, ease: "easeOut" }}
              className="inline-flex items-center space-x-2 bg-white border border-slate-200 px-4 py-2 text-xs font-bold text-slate-600 shadow-sm type-mono-label crosshair-corners"
            >
              <span className="relative flex h-2 w-2">
                <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-indigo-400 opacity-75"></span>
                <span className="relative inline-flex rounded-full h-2 w-2 bg-indigo-500"></span>
              </span>
              <span>24/7 AI Real Estate Assistant Online</span>
            </motion.div>

            <motion.h1
              initial={{ opacity: 0, y: 20 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.5, delay: 0.1, ease: "easeOut" }}
              className="text-5xl md:text-7xl font-extrabold tracking-tighter leading-[1.1] max-w-4xl text-slate-900 drop-shadow-sm"
            >
              Find Your Dream Home <br className="hidden md:block" /> By Just {" "}
              <span className="text-transparent bg-clip-text bg-gradient-to-r from-indigo-600 to-violet-600">Talking</span>.
            </motion.h1>

            <motion.p
              initial={{ opacity: 0, y: 20 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.5, delay: 0.2, ease: "easeOut" }}
              className="text-lg md:text-xl text-slate-500 max-w-2xl leading-relaxed font-mono"
            >
              ~ uv add real-estate-ai <br/>
              Skip the endless scrolling. Talk to our AI to instantly find properties, schedule viewings, and negotiate contracts globally.
            </motion.p>

            <motion.div
              initial={{ opacity: 0, y: 20 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.5, delay: 0.3, ease: "easeOut" }}
              className="flex flex-col sm:flex-row items-center gap-4 pt-4"
            >
              <Link
                href="/dashboard"
                className="group relative inline-flex items-center justify-center gap-2 px-8 py-4 text-sm font-bold text-white transition-all duration-300 bg-slate-900 border border-slate-800 hover:bg-slate-800 hover:shadow-lg hover:shadow-indigo-500/20 type-mono-label crosshair-corners"
              >
                <span className="relative flex items-center gap-2">
                  Talk to AI Agent <Mic className="w-4 h-4" />
                </span>
              </Link>
            </motion.div>
          </section>

          {/* Core Features Grid */}
          <section>
            <motion.div 
              initial={{ opacity: 0, y: 20 }}
              whileInView={{ opacity: 1, y: 0 }}
              viewport={{ once: true }}
              transition={{ duration: 0.5 }}
              className="grid grid-cols-1 md:grid-cols-3 gap-6"
            >
              {/* Feature 1 */}
              <div className="group relative bg-white border border-slate-200 p-8 hover:bg-slate-50 transition-all duration-300 crosshair-corners">
                <div className="relative z-10 flex flex-col space-y-4">
                  <div className="w-12 h-12 bg-indigo-50 flex items-center justify-center border border-indigo-200 text-indigo-600 shadow-sm">
                    <Mic className="w-6 h-6" />
                  </div>
                  <h3 className="text-xl font-bold text-slate-900 tracking-tight type-mono-label">Natural Voice Search</h3>
                  <p className="text-sm text-slate-500 leading-relaxed font-mono">
                    Simply tell our AI what you are looking for—like "a 2-bedroom downtown under $2,000"—and get matching properties instantly.
                  </p>
                </div>
              </div>

              {/* Feature 2 */}
              <div className="group relative bg-white border border-slate-200 p-8 hover:bg-slate-50 transition-all duration-300 crosshair-corners">
                <div className="relative z-10 flex flex-col space-y-4">
                  <div className="w-12 h-12 bg-indigo-50 flex items-center justify-center border border-indigo-200 text-indigo-600 shadow-sm">
                    <CheckCircle2 className="w-6 h-6" />
                  </div>
                  <h3 className="text-xl font-bold text-slate-900 tracking-tight type-mono-label">Instant Bookings</h3>
                  <p className="text-sm text-slate-500 leading-relaxed font-mono">
                    No more waiting for callbacks. View available time slots and reserve your property tour with a single tap.
                  </p>
                </div>
              </div>

              {/* Feature 3 */}
              <div className="group relative bg-white border border-slate-200 p-8 hover:bg-slate-50 transition-all duration-300 crosshair-corners">
                <div className="relative z-10 flex flex-col space-y-4">
                  <div className="w-12 h-12 bg-indigo-50 flex items-center justify-center border border-indigo-200 text-indigo-600 shadow-sm">
                    <Shield className="w-6 h-6" />
                  </div>
                  <h3 className="text-xl font-bold text-slate-900 tracking-tight type-mono-label">Verified Listings</h3>
                  <p className="text-sm text-slate-500 leading-relaxed font-mono">
                    Every property on our platform is strictly verified, ensuring you only deal with authentic landlords and agencies.
                  </p>
                </div>
              </div>
            </motion.div>
          </section>



        </main>
        
        <footer className="border-t border-slate-200 bg-slate-50 mt-20">
           <div className="max-w-7xl mx-auto px-6 py-8 flex flex-col md:flex-row items-center justify-between gap-4">
              <p className="text-slate-500 text-xs font-mono">
                 © 2026 Estate AI Assistant. All rights reserved.
              </p>
              <div className="flex items-center space-x-4 text-xs font-bold text-slate-400 type-mono-label">
                 <Link href="#" className="hover:text-indigo-600 transition-colors">Privacy</Link>
                 <Link href="#" className="hover:text-indigo-600 transition-colors">Terms</Link>
              </div>
           </div>
        </footer>
      </div>
    </div>
  );
}
