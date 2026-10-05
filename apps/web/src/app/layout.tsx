import React from "react";
import type { Metadata } from "next";
import "./globals.css";
import { Providers } from "./providers";

export const metadata: Metadata = {
  title: "Estate AI - Autonomous Voice Agent & RAG Platform",
  description: "Pure Rust powered AI Voice Agent & Real Estate Orchestration Platform for premium listings",
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en" className="light">
      <body className="bg-[#F8F9FA] text-slate-900 min-h-screen font-sans antialiased selection:bg-indigo-500/30 selection:text-indigo-900">
        <Providers>{children}</Providers>
      </body>
    </html>
  );
}
