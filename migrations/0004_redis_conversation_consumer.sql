-- The conversation worker can retry reads safely before it clears the Redis session stream.
ALTER TABLE public.stream_events ADD COLUMN redis_entry_id text;
CREATE UNIQUE INDEX stream_events_session_redis_entry
    ON public.stream_events (session_id, redis_entry_id)
    WHERE redis_entry_id IS NOT NULL;
