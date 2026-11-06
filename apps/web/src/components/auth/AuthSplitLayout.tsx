import React from "react";
import type { AuthShowcaseContent } from "../../../types/auth-layout";
import { AuthBrandPanel } from "./AuthBrandPanel";

interface AuthSplitLayoutProps {
  content: AuthShowcaseContent;
  topSlot?: React.ReactNode;
  children: React.ReactNode;
}

export function AuthSplitLayout({ content, topSlot, children }: AuthSplitLayoutProps): React.ReactElement {
  return (
    <main className="theme-page grid min-h-screen w-full lg:grid-cols-[1.1fr_1fr]">
      <AuthBrandPanel content={content} />
      <section className="relative flex flex-col px-4 py-8 sm:px-8 lg:px-12">
        <div className="flex min-h-[2.5rem] items-center justify-end">{topSlot}</div>
        <div className="flex flex-1 items-center justify-center py-8">
          <div className="w-full max-w-md">{children}</div>
        </div>
      </section>
    </main>
  );
}
