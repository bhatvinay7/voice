import React from "react";

export type AuthMode = "login" | "signup";

interface AuthCardHeaderProps {
  mode: AuthMode;
}

export function AuthCardHeader({ mode }: AuthCardHeaderProps): React.ReactElement {
  return (
    <div className="mb-6">
      <h2 className="text-xl font-semibold tracking-tight text-theme-text">
        {mode === "login" ? "Welcome back" : "Create your account"}
      </h2>
      <p className="mt-1 text-sm text-theme-sub">
        {mode === "login"
          ? "Sign in to continue your property search."
          : "Save listings and speak with your DWIN AI property assistant."}
      </p>
    </div>
  );
}
