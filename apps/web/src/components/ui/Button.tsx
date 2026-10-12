import React, { JSX } from "react";
import { ButtonProps } from "../../../types/ui";
import { Loader2 } from "lucide-react";

export function Button({
  children,
  variant = "primary",
  size = "md",
  isLoading = false,
  leftIcon,
  rightIcon,
  className = "",
  disabled = false,
  ...props
}: ButtonProps): React.ReactElement {
  const baseStyles = "inline-flex items-center justify-center font-medium transition-colors focus:outline-none disabled:opacity-50 disabled:cursor-not-allowed";
  
  const variants = {
    primary: "bg-white text-black border border-slate-300 hover:bg-zinc-100 dark:bg-black dark:text-white dark:border-white/20 dark:hover:bg-zinc-900",
    secondary: "bg-white text-black border border-slate-300 hover:bg-zinc-100 dark:bg-black dark:text-white dark:border-white/20 dark:hover:bg-zinc-900",
    outline: "border-2 border-slate-300 bg-white text-black hover:bg-zinc-100 dark:border-white/20 dark:bg-black dark:text-white dark:hover:bg-zinc-900",
    ghost: "bg-white text-black hover:bg-zinc-100 dark:bg-black dark:text-white dark:hover:bg-zinc-900",
  };

  const sizes = {
    sm: "h-9 px-3 text-sm",
    md: "h-11 px-6 text-base",
    lg: "h-14 px-8 text-lg",
  };

  return (
    <button
      className={`${baseStyles} ${variants[variant]} ${sizes[size]} ${className}`}
      disabled={disabled || isLoading}
      {...props}
    >
      {isLoading && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
      {!isLoading && leftIcon && <span className="mr-2">{leftIcon}</span>}
      {children}
      {!isLoading && rightIcon && <span className="ml-2">{rightIcon}</span>}
    </button>
  );
}
