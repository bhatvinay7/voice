use crate::models::{BookingResponse, OutboxNotification, UserProfile, ViewingBooking};
use chrono::Utc;
use uuid::Uuid;
use super::{DatabaseStore, DbResult, postgres::{error, Record}};

fn rejected(code: &str, message: String) -> BookingResponse {
    BookingResponse { success: false, booking: None, error_code: Some(code.into()), error: Some(message) }
}

impl DatabaseStore {
    pub async fn book_viewing_slot(&self, listing_id: &str, user_id: &str, slot_time: &str, idempotency_key: &str) -> DbResult<BookingResponse> {
        if [listing_id, user_id, slot_time, idempotency_key].iter().any(|value| value.trim().is_empty()) {
            return Ok(rejected("MISSING_BOOKING_DETAILS", "Listing, user, time slot and request key are required".into()));
        }
        let mut tx = self.pool.begin().await.map_err(error)?;
        // Serialize matching request keys, then matching slots across every service instance.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))").bind(format!("booking-key:{idempotency_key}")).execute(&mut *tx).await.map_err(error)?;
        if let Some(saved) = sqlx::query_scalar::<_, String>("SELECT response::text FROM public.booking_idempotency WHERE idempotency_key = $1")
            .bind(idempotency_key).fetch_optional(&mut *tx).await.map_err(error)? {
            return serde_json::from_str(&saved).map_err(|_| "Invalid saved booking response".into());
        }
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))").bind(format!("booking-slot:{listing_id}:{slot_time}")).execute(&mut *tx).await.map_err(error)?;
        let user = sqlx::query_scalar::<_, String>("SELECT row_to_json(t)::text FROM public.user_profiles t WHERE id = $1")
            .bind(user_id).fetch_optional(&mut *tx).await.map_err(error)?
            .map(|s| serde_json::from_str::<UserProfile>(&s).map_err(|_| "Invalid user record")).transpose()?;
        let cfg = config::AppConfig::load().map_err(|_| "Invalid application configuration")?;
        let listing_exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM public.property_listings WHERE id = $1)").bind(listing_id).fetch_one(&mut *tx).await.map_err(error)?;
        let occupied: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM public.viewing_bookings WHERE listing_id = $1 AND slot_time = $2 AND status = 'confirmed')")
            .bind(listing_id).bind(slot_time).fetch_one(&mut *tx).await.map_err(error)?;
        let response = if user.is_none() {
            rejected("USER_NOT_FOUND", "User not found".into())
        } else if !listing_exists {
            rejected("LISTING_NOT_FOUND", "Property listing not found".into())
        } else if cfg.kyc_required_for_booking && !user.as_ref().unwrap().is_verified {
            rejected("VERIFICATION_REQUIRED", cfg.kyc_unverified_message.replace("{document_name}", &cfg.kyc_document_name))
        } else if occupied {
            rejected("SLOT_UNAVAILABLE", format!("The slot {slot_time} is already booked."))
        } else {
            let booking = ViewingBooking { id: format!("book-{}", Uuid::new_v4()), listing_id: listing_id.into(), user_id: user_id.into(), slot_time: slot_time.into(), status: "confirmed".into(), created_at: Utc::now().to_rfc3339(), idempotency_key: idempotency_key.into() };
            sqlx::query("INSERT INTO public.viewing_bookings SELECT * FROM json_populate_record(NULL::public.viewing_bookings, $1::json)")
                .bind(serde_json::to_string(&booking).map_err(|_| "Invalid booking")?).execute(&mut *tx).await.map_err(error)?;
            let user = user.as_ref().unwrap();
            for (channel, recipient, kind) in [("email", &user.email, "SLOT_CONFIRMATION_PDF"), ("sms", &user.phone, "SMS_CALENDAR_INVITE")] {
                let notification = OutboxNotification { id: format!("outbox-{}", Uuid::new_v4()), booking_id: booking.id.clone(), user_id: user_id.into(), channel: channel.into(), recipient: recipient.clone(), payload_json: serde_json::json!({"type": kind, "slot_time": slot_time}).to_string(), status: "pending".into(), created_at: Utc::now().to_rfc3339(), processed_at: None };
                sqlx::query(OutboxNotification::UPSERT).bind(serde_json::to_string(&notification).map_err(|_| "Invalid notification")?).execute(&mut *tx).await.map_err(error)?;
            }
            BookingResponse { success: true, booking: Some(booking), error_code: None, error: None }
        };
        sqlx::query("INSERT INTO public.booking_idempotency VALUES ($1, $2::jsonb)")
            .bind(idempotency_key).bind(serde_json::to_string(&response).map_err(|_| "Invalid booking response")?).execute(&mut *tx).await.map_err(error)?;
        tx.commit().await.map_err(error)?;
        Ok(response)
    }
}
