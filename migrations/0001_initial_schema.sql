-- Initial schema matching crates/libs/db/src/models.rs.

-- Application persistence is wired separately; no demo data is inserted.

CREATE TABLE public.property_listings (
    id text PRIMARY KEY,
    title text NOT NULL,
    secondary_title text NOT NULL,
    description text NOT NULL,
    secondary_description text NOT NULL,
    city text NOT NULL,
    location text NOT NULL,
    price_aed numeric(20,0) NOT NULL CHECK (price_aed BETWEEN 0 AND 18446744073709551615),
    listing_type text NOT NULL,
    bedrooms smallint NOT NULL CHECK (bedrooms BETWEEN 0 AND 255),
    bathrooms smallint NOT NULL CHECK (bathrooms BETWEEN 0 AND 255),
    area_sqft bigint NOT NULL CHECK (area_sqft BETWEEN 0 AND 4294967295),
    pet_friendly boolean NOT NULL,
    near_metro boolean NOT NULL,
    agent_id text NOT NULL,
    available_slots text[] NOT NULL
);

CREATE TABLE public.user_profiles (
    id text PRIMARY KEY,
    name text NOT NULL,
    email text NOT NULL,
    phone text NOT NULL,
    is_verified boolean NOT NULL,
    emirates_id_number text,
    role text NOT NULL,
    auth_methods text[] NOT NULL
);

CREATE TABLE public.viewing_bookings (
    id text PRIMARY KEY,
    listing_id text NOT NULL,
    user_id text NOT NULL,
    slot_time text NOT NULL,
    status text NOT NULL,
    created_at text NOT NULL,
    idempotency_key text NOT NULL
);

CREATE TABLE public.stream_events (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    stream text NOT NULL,
    session_id text NOT NULL,
    trace_id text NOT NULL,
    event_type text NOT NULL,
    payload_json text NOT NULL,
    timestamp text NOT NULL
);

CREATE TABLE public.tenancy_contracts (
    id text PRIMARY KEY,
    filename text NOT NULL,
    r2_vault_path text NOT NULL,
    landlord_name text NOT NULL,
    tenant_name text NOT NULL,
    property_address text NOT NULL,
    annual_rent_aed numeric(20,0) NOT NULL CHECK (annual_rent_aed BETWEEN 0 AND 18446744073709551615),
    security_deposit_aed numeric(20,0) NOT NULL CHECK (security_deposit_aed BETWEEN 0 AND 18446744073709551615),
    cheques_count smallint NOT NULL CHECK (cheques_count BETWEEN 0 AND 255),
    notice_period_days integer NOT NULL CHECK (notice_period_days BETWEEN 0 AND 65535),
    confidence_score real NOT NULL,
    needs_human_review boolean NOT NULL,
    status text NOT NULL,
    clause_summary text NOT NULL,
    created_at text NOT NULL
);

CREATE TABLE public.outbox_notifications (
    id text PRIMARY KEY,
    booking_id text NOT NULL,
    user_id text NOT NULL,
    channel text NOT NULL,
    recipient text NOT NULL,
    payload_json text NOT NULL,
    status text NOT NULL,
    created_at text NOT NULL,
    processed_at text
);

CREATE TABLE public.call_summaries (
    call_id text PRIMARY KEY,
    user_id text NOT NULL,
    duration_seconds bigint NOT NULL CHECK (duration_seconds BETWEEN 0 AND 4294967295),
    text_presigned_url text NOT NULL,
    audio_presigned_url text NOT NULL,
    summary text NOT NULL,
    qualified_budget_aed numeric(20,0) NOT NULL CHECK (qualified_budget_aed BETWEEN 0 AND 18446744073709551615),
    preferred_location text NOT NULL,
    created_at text NOT NULL
);

CREATE TABLE public.call_endpoints (
    session_id text PRIMARY KEY,
    audio_url text NOT NULL,
    text_url text NOT NULL,
    created_at text NOT NULL
);

CREATE TABLE public.admin_graph_nodes (
    id text PRIMARY KEY,
    step_type text NOT NULL,
    instruction text NOT NULL,
    status text NOT NULL,
    is_preset boolean NOT NULL
);

CREATE TABLE public.knowledge_documents (
    id text PRIMARY KEY,
    title text NOT NULL,
    category text NOT NULL,
    content text NOT NULL,
    keywords text[] NOT NULL,
    metadata jsonb NOT NULL
);

CREATE TABLE public.booking_idempotency (idempotency_key text PRIMARY KEY, response jsonb NOT NULL);

CREATE INDEX ON public.property_listings (city, listing_type);

CREATE INDEX ON public.user_profiles (email);

CREATE INDEX ON public.viewing_bookings (user_id);

CREATE INDEX ON public.viewing_bookings (listing_id, slot_time);

CREATE INDEX ON public.stream_events (session_id);

CREATE INDEX ON public.outbox_notifications (status);

CREATE INDEX ON public.call_summaries (user_id);
