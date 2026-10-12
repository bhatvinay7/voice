"use client";

import React, { Component, ErrorInfo, ReactNode } from "react";
import { AlertTriangle, RefreshCw } from "lucide-react";

interface AdminErrorBoundaryProps {
  children: ReactNode;
  title?: string;
}

interface AdminErrorBoundaryState {
  hasError: boolean;
  errorMessage: string;
}

export class AdminErrorBoundary extends Component<
  AdminErrorBoundaryProps,
  AdminErrorBoundaryState
> {
  public override state: AdminErrorBoundaryState = {
    hasError: false,
    errorMessage: "",
  };

  public static getDerivedStateFromError(error: Error): AdminErrorBoundaryState {
    return {
      hasError: true,
      errorMessage: error.message || "An unexpected rendering error occurred.",
    };
  }

  public override componentDidCatch(error: Error, errorInfo: ErrorInfo): void {
    console.error("AdminErrorBoundary caught an error:", error, errorInfo);
  }

  private handleReset = (): void => {
    this.setState({ hasError: false, errorMessage: "" });
  };

  public override render(): ReactNode {
    if (this.state.hasError) {
      return (
        <div className="bg-theme-card border border-rose-500/30 rounded-2xl p-6 sm:p-8 space-y-4 shadow-xl">
          <div className="flex items-center gap-3 text-rose-400">
            <div className="p-2 rounded-xl bg-rose-500/10 border border-rose-500/20">
              <AlertTriangle className="w-5 h-5" />
            </div>
            <div>
              <h4 className="text-base font-bold text-theme-text font-serif">
                {this.props.title || "Admin Module"} Render Error Handled
              </h4>
              <p className="text-xs text-theme-sub mt-0.5">
                The widget encountered an issue but the rest of the portal remains active.
              </p>
            </div>
          </div>

          <div className="bg-theme-main p-3 rounded-xl border border-theme-border font-mono text-xs text-rose-300 break-words">
            {this.state.errorMessage}
          </div>

          <button
            type="button"
            onClick={this.handleReset}
            className="inline-flex items-center gap-2 px-4 py-2 rounded-xl bg-theme-main border border-theme-border hover:bg-theme-card text-theme-text text-xs font-semibold transition"
          >
            <RefreshCw className="w-3.5 h-3.5" />
            Retry Component
          </button>
        </div>
      );
    }

    return this.props.children;
  }
}
