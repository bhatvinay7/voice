export interface StreamEventRecord {
  stream: string;
  session_id: string;
  trace_id: string;
  event_type: string;
  payload_json: string;
  timestamp: string;
}

export interface CallSummaryRecord {
  call_id: string;
  user_id?: string;
  user_name?: string;
  user_email?: string;
  duration_seconds?: number;
  r2_transcript_path?: string;
  r2_audio_path?: string;
  text_presigned_url?: string;
  audio_presigned_url?: string;
  summary?: string;
  qualified_budget?: number;
  qualified_budget_aed?: number;
  preferred_location?: string;
  created_at: string;
}

export interface PaginationMeta {
  page: number;
  page_size: number;
  total: number;
  total_pages: number;
}

export interface CallSummaryPage {
  success: boolean;
  summaries: CallSummaryRecord[];
  pagination: PaginationMeta;
}

export interface CallSummaryQuery {
  page: number;
  pageSize: number;
  search: string;
}
