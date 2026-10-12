use crate::models::{BookingResponse, OutboxNotification, ViewingBooking};
use chrono::Utc;
use uuid::Uuid;
use super::{this_lock, DatabaseStore};

impl DatabaseStore {
    pub fn book_viewing_slot(
        &self,
        listing_id: &str,
        user_id: &str,
        slot_time: &str,
        idempotency_key: &str,
    ) -> BookingResponse {
        if let Ok(idemp_guard) = this_lock(&self.idempotency_store) {
            if let Some(previous_res) = idemp_guard.get(idempotency_key) {
                return previous_res.clone();
            }
        }

        let user = self.get_user(user_id);
        let cfg = config::AppConfig::load().unwrap_or_else(|_| config::AppConfig::load_default());

        if cfg.kyc_required_for_booking && (user.is_none() || !user.as_ref().map_or(false, |u| u.is_verified)) {
            let error_msg = cfg.kyc_unverified_message
                .replace("{document_name}", &cfg.kyc_document_name);

            let error_res = BookingResponse {
                success: false,
                booking: None,
                error_code: Some("VERIFICATION_REQUIRED".to_string()),
                error: Some(error_msg),
            };

            if let Ok(mut idemp_guard) = this_lock(&self.idempotency_store) {
                idemp_guard.insert(idempotency_key.to_string(), error_res.clone());
            }

            return error_res;
        }

        let mut bookings_guard = match this_lock(&self.bookings) {
            Ok(g) => g,
            Err(_) => {
                return BookingResponse {
                    success: false,
                    booking: None,
                    error_code: Some("SYSTEM_ERROR".to_string()),
                    error: Some("Internal database lock failure.".to_string()),
                }
            }
        };

        let slot_already_booked = bookings_guard.iter().any(|b| {
            b.listing_id == listing_id && b.slot_time == slot_time && b.status == "confirmed"
        });

        if slot_already_booked {
            let error_res = BookingResponse {
                success: false,
                booking: None,
                error_code: Some("SLOT_UNAVAILABLE".to_string()),
                error: Some(format!("The slot {} is already booked.", slot_time)),
            };

            if let Ok(mut idemp_guard) = this_lock(&self.idempotency_store) {
                idemp_guard.insert(idempotency_key.to_string(), error_res.clone());
            }

            return error_res;
        }

        let booking = ViewingBooking {
            id: format!("book-{}", Uuid::new_v4()),
            listing_id: listing_id.to_string(),
            user_id: user_id.to_string(),
            slot_time: slot_time.to_string(),
            status: "confirmed".to_string(),
            created_at: Utc::now().to_rfc3339(),
            idempotency_key: idempotency_key.to_string(),
        };

        bookings_guard.push(booking.clone());

        if let Ok(mut outbox_guard) = this_lock(&self.outbox) {
            outbox_guard.push(OutboxNotification {
                id: format!("outbox-email-{}", Uuid::new_v4()),
                booking_id: booking.id.clone(),
                user_id: user_id.to_string(),
                channel: "email".to_string(),
                recipient: user.as_ref().map(|u| u.email.clone()).unwrap_or_default(),
                payload_json:
                    serde_json::json!({ "type": "SLOT_CONFIRMATION_PDF", "slot_time": slot_time })
                        .to_string(),
                status: "pending".to_string(),
                created_at: Utc::now().to_rfc3339(),
                processed_at: None,
            });
            outbox_guard.push(OutboxNotification {
                id: format!("outbox-sms-{}", Uuid::new_v4()),
                booking_id: booking.id.clone(),
                user_id: user_id.to_string(),
                channel: "sms".to_string(),
                recipient: user.as_ref().map(|u| u.phone.clone()).unwrap_or_default(),
                payload_json:
                    serde_json::json!({ "type": "SMS_CALENDAR_INVITE", "slot_time": slot_time })
                        .to_string(),
                status: "pending".to_string(),
                created_at: Utc::now().to_rfc3339(),
                processed_at: None,
            });
        }

        let success_res = BookingResponse {
            success: true,
            booking: Some(booking),
            error_code: None,
            error: None,
        };

        if let Ok(mut idemp_guard) = this_lock(&self.idempotency_store) {
            idemp_guard.insert(idempotency_key.to_string(), success_res.clone());
        }

        success_res
    }
}
