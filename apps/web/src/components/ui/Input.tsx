import React, { JSX } from "react";
import { InputProps } from "../../../types/ui";

export function Input({
  label,
  error,
  className = "",
  ...props
}: InputProps): React.ReactElement {
  return (
    <div className="flex flex-col space-y-1">
      {label && (
        <label className="text-sm font-medium text-black dark:text-white">
          {label}
        </label>
      )}
      <input
        className={`flex h-11 w-full rounded-md border border-slate-300 bg-white text-black placeholder:text-slate-400 focus:outline-none focus:ring-2 focus:ring-indigo-500 disabled:cursor-not-allowed disabled:opacity-50 dark:bg-black dark:text-white dark:border-white/20 dark:placeholder:text-zinc-500 ${className}`}
        {...props}
      />
      {error && (
        <span className="text-xs text-red-500">{error}</span>
      )}
    </div>
  );
}
