"use client";

import React, { useEffect, useRef, useState } from "react";

interface AuthSocialButtonsProps {
  clientId: string;
  onGoogleCredential: (credential: string) => void;
  isSubmitting: boolean;
}

const GOOGLE_SCRIPT_ID = "google-identity-services";
const GOOGLE_SCRIPT_URL = "https://accounts.google.com/gsi/client";

export function AuthSocialButtons({
  clientId,
  onGoogleCredential,
  isSubmitting,
}: AuthSocialButtonsProps): React.ReactElement {
  const buttonContainer = useRef<HTMLDivElement>(null);
  const [scriptReady, setScriptReady] = useState(false);
  const [scriptFailed, setScriptFailed] = useState(false);

  useEffect(() => {
    if (!clientId) return;

    const initializeGoogleButton = () => {
      if (!buttonContainer.current || !window.google?.accounts?.id) return;
      window.google.accounts.id.initialize({
        client_id: clientId,
        callback: ({ credential }) => {
          if (credential) onGoogleCredential(credential);
        },
      });
      window.google.accounts.id.renderButton(buttonContainer.current, {
        theme: "outline",
        size: "large",
        shape: "pill",
        text: "continue_with",
        width: Math.min(buttonContainer.current.clientWidth, 400),
      });
      setScriptReady(true);
    };

    const existingScript = document.getElementById(GOOGLE_SCRIPT_ID) as HTMLScriptElement | null;
    if (window.google?.accounts?.id) {
      initializeGoogleButton();
      return;
    }
    if (existingScript) {
      existingScript.addEventListener("load", initializeGoogleButton, { once: true });
      existingScript.addEventListener("error", () => setScriptFailed(true), { once: true });
      return;
    }

    const script = document.createElement("script");
    script.id = GOOGLE_SCRIPT_ID;
    script.src = GOOGLE_SCRIPT_URL;
    script.async = true;
    script.defer = true;
    script.onload = initializeGoogleButton;
    script.onerror = () => setScriptFailed(true);
    document.head.appendChild(script);
  }, [clientId, onGoogleCredential]);

  return (
    <>
      <div className="relative my-6 text-center" aria-hidden="true">
        <div className="absolute inset-0 flex items-center">
          <div className="w-full border-t border-theme-border dark:border-white/10" />
        </div>
        <div className="relative flex justify-center text-xs">
          <span className="bg-theme-card px-3 text-theme-sub">or continue with</span>
        </div>
      </div>

      {clientId ? (
        <div className="relative min-h-11">
          {!scriptReady && !scriptFailed && (
            <div className="absolute inset-0 flex items-center justify-center rounded-full border border-theme-border text-sm text-theme-sub">
              Loading Google sign-in…
            </div>
          )}
          <div
            ref={buttonContainer}
            className={`flex min-h-11 justify-center ${isSubmitting ? "pointer-events-none opacity-60" : ""}`}
            aria-label="Continue with Google"
          />
          {scriptFailed && (
            <p className="mt-2 text-center text-xs text-rose-500" role="status">
              Google sign-in could not load. Please try again later.
            </p>
          )}
        </div>
      ) : (
        <p className="text-center text-xs text-theme-sub">
          Google sign-in is not configured yet.
        </p>
      )}
    </>
  );
}
