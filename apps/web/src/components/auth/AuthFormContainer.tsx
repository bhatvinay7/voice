"use client";

import React, { useCallback, useState } from "react";
import { useRouter } from "next/navigation";
import { AlertCircle, ArrowRight, Eye, EyeOff, Lock, Mail, User } from "lucide-react";
import { useEmailLogin, useGoogleLogin } from "../../lib/api";
import { AuthCardHeader } from "./AuthCardHeader";
import { AuthSocialButtons } from "./AuthSocialButtons";

export function AuthFormContainer(): React.ReactElement {
  const router = useRouter();
  const { mutate: loginWithGoogle, isPending: googleLoginPending } = useGoogleLogin();
  const emailLoginMutation = useEmailLogin();
  const [mode, setMode] = useState<"login" | "signup">("login");
  const [showPassword, setShowPassword] = useState(false);
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [name, setName] = useState("");
  const [errorMsg, setErrorMsg] = useState("");
  const isSubmitting = emailLoginMutation.isPending || googleLoginPending;

  const redirectForUser = (role?: string) => {
    router.push(role === "admin" ? "/admin" : "/dashboard");
  };

  const handleGoogleCredential = useCallback((idToken: string) => {
    setErrorMsg("");
    loginWithGoogle(
      { idToken },
      {
        onSuccess: (data) => redirectForUser(data.user?.role),
        onError: (error) => setErrorMsg(error.message || "Google sign-in failed. Please try again."),
      }
    );
  }, [loginWithGoogle, router]);

  const handleFormSubmit = (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setErrorMsg("");
    emailLoginMutation.mutate(
      { email, password, name: mode === "signup" ? name : undefined },
      {
        onSuccess: (data) => redirectForUser(data.user?.role),
        onError: (error) => setErrorMsg(error.message || "Authentication failed. Please check your details."),
      }
    );
  };

  return (
    <section className="w-full rounded-3xl border border-theme-border bg-theme-card p-6 text-theme-text shadow-xl dark:border-white/15 dark:bg-black sm:p-8">
      <AuthCardHeader mode={mode} />

      {errorMsg && (
        <div className="mb-4 flex items-center gap-2 rounded-xl border border-rose-500/30 bg-rose-500/10 p-3 text-sm text-rose-500" role="alert">
          <AlertCircle className="h-4 w-4 shrink-0" />
          <span>{errorMsg}</span>
        </div>
      )}

      <form onSubmit={handleFormSubmit} className="space-y-4">
        {mode === "signup" && (
          <label className="block space-y-1.5">
            <span className="text-sm font-medium">Full name</span>
            <span className="relative block">
              <User className="absolute left-3.5 top-1/2 h-4 w-4 -translate-y-1/2 text-theme-sub" />
              <input
                type="text"
                value={name}
                onChange={(event) => setName(event.target.value)}
                placeholder="Your name"
                required
                className="w-full rounded-xl border border-theme-border bg-theme-main py-3 pl-10 pr-4 text-sm text-theme-text outline-none focus:border-theme-text dark:border-white/20 dark:bg-black"
              />
            </span>
          </label>
        )}

        <label className="block space-y-1.5">
          <span className="text-sm font-medium">Email address</span>
          <span className="relative block">
            <Mail className="absolute left-3.5 top-1/2 h-4 w-4 -translate-y-1/2 text-theme-sub" />
            <input
              type="email"
              value={email}
              onChange={(event) => setEmail(event.target.value)}
              placeholder="you@example.com"
              autoComplete="email"
              required
              className="w-full rounded-xl border border-theme-border bg-theme-main py-3 pl-10 pr-4 text-sm text-theme-text outline-none focus:border-theme-text dark:border-white/20 dark:bg-black"
            />
          </span>
        </label>

        <label className="block space-y-1.5">
          <span className="text-sm font-medium">Password</span>
          <span className="relative block">
            <Lock className="absolute left-3.5 top-1/2 h-4 w-4 -translate-y-1/2 text-theme-sub" />
            <input
              type={showPassword ? "text" : "password"}
              value={password}
              onChange={(event) => setPassword(event.target.value)}
              placeholder="Enter your password"
              autoComplete={mode === "signup" ? "new-password" : "current-password"}
              required
              className="w-full rounded-xl border border-theme-border bg-theme-main py-3 pl-10 pr-11 text-sm text-theme-text outline-none focus:border-theme-text dark:border-white/20 dark:bg-black"
            />
            <button
              type="button"
              onClick={() => setShowPassword((shown) => !shown)}
              aria-label={showPassword ? "Hide password" : "Show password"}
              className="absolute right-3.5 top-1/2 -translate-y-1/2 text-theme-sub hover:text-theme-text"
            >
              {showPassword ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
            </button>
          </span>
        </label>

        <button
          type="submit"
          disabled={isSubmitting}
          className="flex w-full items-center justify-center gap-2 rounded-xl bg-theme-text px-4 py-3 text-sm font-semibold text-theme-main transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
        >
          <span>{isSubmitting ? "Signing in…" : mode === "login" ? "Sign in" : "Create account"}</span>
          {!isSubmitting && <ArrowRight className="h-4 w-4" />}
        </button>
      </form>

      {mode === "login" && (
        <AuthSocialButtons
          clientId={process.env.NEXT_PUBLIC_GOOGLE_CLIENT_ID ?? ""}
          onGoogleCredential={handleGoogleCredential}
          isSubmitting={isSubmitting}
        />
      )}

      <p className="mt-6 text-center text-sm text-theme-sub">
        {mode === "login" ? "New to DWIN?" : "Already have an account?"}{" "}
        <button
          type="button"
          onClick={() => {
            setErrorMsg("");
            setMode((current) => current === "login" ? "signup" : "login");
          }}
          className="font-semibold text-theme-text underline-offset-4 hover:underline"
        >
          {mode === "login" ? "Create an account" : "Sign in"}
        </button>
      </p>
    </section>
  );
}
