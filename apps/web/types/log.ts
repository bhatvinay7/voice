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
  user_id: string;
  duration_seconds: number;
  r2_transcript_path: string;
  r2_audio_path: string;
  summary: string;
  qualified_budget: number;
  preferred_location: string;
  created_at: string;
}
