import React from "react";
import { cn } from "../../lib/utils";

interface AvatarProps {
  name?: string | null;
  email?: string | null;
  size?: "sm" | "md" | "lg";
  className?: string;
}

const SIZE_CLASSES: Record<NonNullable<AvatarProps["size"]>, string> = {
  sm: "h-8 w-8 text-xs rounded-lg",
  md: "h-10 w-10 text-sm rounded-xl",
  lg: "h-12 w-12 text-base rounded-xl",
};

const PLACEHOLDER_NAMES = new Set(["user", "admin", "administrator", "guest"]);

/** Up to two initials from a name, falling back to the email prefix.
 *  Placeholder names like "User" / "Admin" are treated as absent. */
export function getInitials(name?: string | null, email?: string | null): string {
  const trimmed = (name ?? "").trim();
  const isPlaceholder = !trimmed || PLACEHOLDER_NAMES.has(trimmed.toLowerCase());
  if (!isPlaceholder) {
    const parts = trimmed.split(/\s+/).filter(Boolean);
    const initials = parts.slice(0, 2).map((p) => p.charAt(0).toUpperCase()).join("");
    if (initials) return initials;
  }
  // Fall back to the part of the email before @
  const emailLocal = (email ?? "").split("@")[0].trim();
  const fromEmail = emailLocal.charAt(0).toUpperCase();
  return fromEmail || "?";
}

export function Avatar({ name, email, size = "md", className }: AvatarProps): React.ReactElement {
  return (
    <span
      aria-hidden
      className={cn(
        "edge-highlight inline-flex shrink-0 select-none items-center justify-center bg-theme-card-hover font-display font-semibold text-theme-text",
        SIZE_CLASSES[size],
        className,
      )}
    >
      {getInitials(name, email)}
    </span>
  );
}
