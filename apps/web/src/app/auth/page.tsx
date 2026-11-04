import React from "react";
import type { Metadata } from "next";
import { AuthFormContainer } from "../../components/auth/AuthFormContainer";
import { AuthSplitLayout } from "../../components/auth/AuthSplitLayout";
import { ThemeToggle } from "../../components/ThemeToggle";
import { clientAuthShowcase } from "../../lib/auth-showcase";

export const metadata: Metadata = {
  title: "Sign in · DWIN",
  description: "Sign in to DWIN to search UAE properties by voice, save listings and book viewings.",
};

export default function AuthPage() {
  return (
    <AuthSplitLayout content={clientAuthShowcase} topSlot={<ThemeToggle />}>
      <div className="mb-6 lg:hidden">
        <span className="eyebrow-pill">{clientAuthShowcase.eyebrow}</span>
        <p className="mt-4 font-display text-2xl font-semibold tracking-tight">Find your next property by voice</p>
      </div>
      <h1 className="sr-only">Sign in to DWIN</h1>
      <AuthFormContainer />
    </AuthSplitLayout>
  );
}
