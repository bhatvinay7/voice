import React, { JSX } from "react";
import { CardProps } from "../../../types/ui";

export function Card({
  children,
  className = "",
}: CardProps): React.ReactElement {
  return (
    <div className={`glass-panel p-6 shadow-sm ${className}`}>
      {children}
    </div>
  );
}
