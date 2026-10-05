export interface ViewingBooking {
  id: string;
  listing_id: string;
  user_id: string;
  slot_time: string;
  status: "confirmed" | "cancelled";
  created_at: string;
  idempotency_key: string;
}

export interface BookingResponse {
  success: boolean;
  booking?: ViewingBooking;
  error_code?: string;
  error?: string;
}
