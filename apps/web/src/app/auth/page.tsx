import React from "react";
import { AuthFormContainer } from "../../components/auth/AuthFormContainer";

export default function AuthPage() {
  return (
    <main className="min-h-screen w-full bg-theme-main text-theme-text flex items-center justify-center px-4 py-10 sm:px-6">
      <div className="w-full max-w-md">
        <div className="mb-6 text-center">
          <p className="text-xs font-semibold uppercase tracking-[0.2em] text-theme-sub">
            DWIN · AI Voice for Real Estate
          </p>
          <h1 className="mt-3 text-2xl font-semibold tracking-tight text-theme-text">
            Find your next property by voice
          </h1>
          <p className="mt-2 text-sm leading-6 text-theme-sub">
            Sign in to explore UAE listings, compare details, and talk through your property search.
          </p>
        </div>
        <AuthFormContainer />
      </div>
    </main>
  );
}
