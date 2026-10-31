-- Shared retry results belong in Neon, not a process-local embedded database.
CREATE TABLE public.actor_idempotency (
    key text PRIMARY KEY,
    response bytea NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX viewing_bookings_confirmed_slot
    ON public.viewing_bookings (listing_id, slot_time) WHERE status = 'confirmed';
CREATE UNIQUE INDEX viewing_bookings_idempotency_key
    ON public.viewing_bookings (idempotency_key);
CREATE INDEX user_profiles_email_lower ON public.user_profiles (lower(email));
CREATE INDEX stream_events_session_order ON public.stream_events (session_id, id);
