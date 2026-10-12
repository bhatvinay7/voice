import { LucideIcon } from "lucide-react";

export type AdminTab =
  | "all"
  | "ingestion"
  | "workflows"
  | "contracts"
  | "calls"
  | "logs";

export interface AdminNavItem {
  id: AdminTab;
  label: string;
  shortLabel: string;
  icon: LucideIcon;
  badge?: string;
  badgeColor?: string;
  description: string;
  count?: number | string;
}

export interface AdminSystemStats {
  servicesOnline: number;
  dbLatencyMs: number;
  activeCallsCount: number;
  vectorIndexStatus: "Ready" | "Syncing" | "Degraded";
}

import { StreamEventRecord, CallSummaryRecord } from "./log";

export interface AdminSessionDetailsResponse {
  success: boolean;
  user?: {
    name: string;
    phone: string;
    email: string;
  };
  session?: {
    id: string;
    user_id: string;
    created_at: string;
  };
  summary?: CallSummaryRecord;
  endpoints?: {
    session_id: string;
    audio_url: string;
    text_url: string;
    created_at: string;
  };
  conversation?: StreamEventRecord[];
  audit?: StreamEventRecord[];
  logs?: StreamEventRecord[];
  raw_events?: StreamEventRecord[];
}


