-- PostgreSQL outbox for retryable delivery to short-lived Redis conversation streams.
ALTER TABLE public.stream_events ADD COLUMN redis_delivered_at timestamptz;
-- Existing history remains in Neon; do not republish old conversations as new live streams.
UPDATE public.stream_events SET redis_delivered_at = now();
CREATE INDEX stream_events_redis_pending ON public.stream_events (id)
    WHERE stream = 'stream:conversation' AND redis_delivered_at IS NULL;
