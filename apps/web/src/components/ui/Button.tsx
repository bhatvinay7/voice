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
    primary: "bg-theme-card text-theme-text border border-theme-border hover:bg-theme-card-hover",
    secondary: "bg-theme-card text-theme-text border border-theme-border hover:bg-theme-card-hover",
    outline: "border-2 border-theme-border bg-theme-card text-theme-text hover:bg-theme-card-hover",
    ghost: "bg-theme-card text-theme-text hover:bg-theme-card-hover",
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
