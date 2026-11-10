# Real Estate Voice Assistant

A Rust and Next.js application for real-estate voice calls, property and knowledge search, viewing bookings, contract review, and call summaries. The development Compose stack runs the web app, backend API, control-plane, and summary worker. Neon PostgreSQL, LiveKit, Redis, Cloudflare R2, and the configured LLM provider are external dependencies.

## Architecture

```mermaid
flowchart TB
    Browser[Browser / Next.js web app]
    Backend["Axum backend<br/>REST, WebSocket, SSE"]
    Dispatcher[Orchestrator dispatcher]
    Session[Per-call SessionActor]
    STS["LiveKit STS worker<br/>VAD, STT, TTS"]
    Agent[LLM agent actor]
    ControlPlane["Control-plane gRPC<br/>cell control and tool execution"]
    RAG["RAG pipeline<br/>listings and knowledge"]
    Fanout["Per-session transcript broadcast<br/>S3 and Redis subscribers"]
    Recording["Recording worker<br/>sample-timed PCM → stereo WAV"]
    R2[(Cloudflare R2)]
    Redis[(Redis)]
    Worker[Summary worker]
    LLM[Configured LLM provider]
    Store["Neon PostgreSQL<br/>shared domain data and call history"]
    LiveKit[(LiveKit Cloud)]

    Browser -->|HTTP requests| Backend
    Backend -->|SSE suggestions| Browser
    Browser <-->|WebRTC media| LiveKit
    Backend --> Dispatcher --> Session
    Backend <-->|CellControl gRPC stream| ControlPlane
    Session --> STS
    Session --> Agent
    STS <-->|agent audio tracks| LiveKit
    STS -->|final transcripts| Session
    Agent -->|assistant transcript| Session
    Session --> Fanout
    Fanout -->|transcript.txt| Recording
    Fanout -->|session-named stream| Redis
    STS -->|user and assistant PCM| Recording
    Agent -->|tool requests| ControlPlane
    ControlPlane --> RAG --> Store
    Redis -->|conversation worker persists events| Store
    Recording -->|assembled WAV| R2
    Recording -->|recording URL| Store
    Redis -->|summary jobs| Worker
    Store -->|transcript and recording context| Worker
    Worker --> LLM
    Worker -->|summary| Store
```

## Call workflow

When a caller starts a session, the backend creates a per-call `SessionActor` and returns the credentials needed to join a LiveKit room. The browser sends and receives audio directly through LiveKit; the backend's STS worker joins the same room to process incoming audio and publish the assistant's voice. VAD detects speech, STT produces a transcript, and the agent uses listing or knowledge context to answer. When a tool is needed, the agent calls the control-plane over gRPC, where the RAG and booking tools run.

## Search, agent, and voice modes

Manual property search uses `POST /api/listings/search` without a `session_id`
and with structured filters
(`location`, `max_price_aed`, `bedrooms`, and `listing_type`). It reads listings
directly and does not invoke an LLM, RAG, a tool, or a voice session.

Agent requests include an existing `session_id`. The session actor
owns its agent, task cancellation tokens, recording pipeline, and transcript
fanout. It sends only the typed `search_properties` and `book_viewing_slot`
tool requests to the control-plane. The orchestrator has no in-process tool
fallback, preventing single-process and deployed behaviour from diverging.

The product Agent Mode switch controls whether the web client starts a voice
session; it does not choose a speech pipeline. Configure the server pipeline in
the relevant service `.env` file:

```env
# Whisper STT -> LLM -> TTS
STS_MODE=STS_NORMAL

# Realtime streaming WebSocket pipeline
STS_MODE=STS_STREAM
```

Each session has a bounded inbox and task-owned cancellation tokens. The global
dispatcher releases its queue lock before delivering to a session, so a blocked
or cancelled call does not hold up unrelated sessions.

## Workflow graph lifecycle

`crates/libs/workflow` is the graph package. `WorkflowRepository` owns durable
admin graph creation, retrieval, update, and deletion. A call receives a
`SessionWorkflow` copy of that definition; it is released on call cancellation
or when the 25-minute session reaper ends the call. Releasing a session graph
clears only in-memory run state and never removes the durable admin graph.

User and assistant PCM frames are collected alongside the live call. Each call owns a Rust broadcast channel: the S3 subscriber writes `transcript.txt` beside the stereo 16 kHz WAV, while the Redis subscriber writes the same events to a stream named exactly after the session. Redis conversation streams have no TTL. The conversation worker reads the Redis stream, commits events to Neon idempotently, then deletes the Redis stream only after that commit succeeds. S3 is an archive and is never used to update Neon. Each agent actor has its own MPSC Redis-poll worker and delay queue, so it can refresh session context without blocking its command loop.

## Context, memory, and turn-taking

The conversation graph is the immutable, session-scoped evidence trail: it records caller turns, verified tool results, assistant responses, interruptions, and their links. `SessionActor` owns this graph and is the only component that can mutate it. The DashMap is a lightweight reference index for actor-safe lookups; it is not the source of truth and does not replace graph bodies.

`SessionMemory` is the bounded working-memory layer derived from verified graph events and persisted in Sled for the active session. It carries the active topic, verified result-set entity IDs, ordinal mappings such as `first → listing-id`, selected entity, aliases such as `it`, and recent turn summaries. It is cleared with the session. Deterministic memory lookup handles obvious follow-ups before LLM reasoning; the main model receives the resolved, verified context rather than inferring it from titles alone.

```mermaid
flowchart LR
    U[Caller audio or browser text] --> T[Server turn collector]
    T -->|quiet window / final speech| S[SessionActor]
    S --> G[ConversationGraph: evidence and provenance]
    G --> M[SessionMemory: bounded working state]
    M --> R[Deterministic reference resolver]
    R -->|ambiguous only| F[Fast LLM planner]
    R -->|resolved context| A[Main LLM]
    F --> A
    A -->|answer + verified references| S
    A --> I[Background graph improver]
    I -->|typed proposal| V[Actor validation]
    V --> G
    V --> M
```

Turn-taking is server-authoritative. Browser transcript fragments are debounced by the owning `SessionActor`, so nearby messages become one final user turn before the fast LLM runs. Voice pipelines use VAD and final STT events; partial transcripts only extend the quiet window. Caller audio is preserved for live VAD/STT, while the archival recording path performs bounded level normalization on the caller channel when it assembles the final S3/R2 WAV. This improves quiet recordings without changing live recognition or assistant playback.

The background graph improver never writes memory directly. It proposes a typed JSON payload containing a topic summary, verified entity IDs, ordinal mappings, aliases, and graph references. The actor validates every proposed node and entity ID against the graph and verified tool data, discarding unknown or malformed fields before storing the proposal as an auditable graph node and rebuilding `SessionMemory`.

## Services

| Service | Role |
|---|---|
| `web` | Next.js caller and admin interface. |
| `backend` | Axum APIs, LiveKit token issuance, session orchestration, and SSE. |
| `control-plane` | Cell control stream and gRPC tool execution for RAG and bookings. |
| `worker` | Redis summary-job polling, transcript loading, and LLM summaries. |

## Run locally

From this directory:

```bash
docker compose -f docker-compose.dev.yml up --build
```

Set the required service configuration in the relevant `.env` files before starting the stack. Compose starts the four services above; configure LiveKit, Redis, R2, and the LLM provider separately. Set the same Neon pooled `NEON_POOLER_URL` (a hostname ending in `-pooler`) for all three Rust services, then run the migrations below before starting them. `POSTGRES_DIRECT_URL` is optional and is used only by the migration runner. Services share PostgreSQL data through asynchronous connection pools and fail startup if Neon or the schema is unavailable. Domain records have no in-memory or SQLite fallback. Sled is used only for local orchestration slots, not user records or call history.

## Phone-provider ingress

Select one provider through `TELEPHONY_PROVIDER` in `.env`: `twilio`, `telnyx`, `vonage`, `plivo`, `sip`, or `custom`. The default is `disabled`; callbacks for all other providers are rejected. Copy the relevant settings from [telephony.env.example](telephony.env.example), supply that provider's credentials, set `TELEPHONY_PUBLIC_BASE_URL` to the public HTTPS backend origin, and restart the backend. Existing web and model configuration stays the same.

Configure your provider's phone number/application to send **POST** answer/incoming and status callbacks to `https://your-backend.example.com/api/telephony/{provider}/incoming`. For Vonage, explicitly set the answer method to POST and enable signed webhooks. Telnyx requires a Voice API application and `TELNYX_PUBLIC_KEY`; Vonage requires `VONAGE_SIGNATURE_SECRET` (separate from its API secret). Twilio, Telnyx, Plivo and Vonage callbacks use their native signature verification. `TELEPHONY_WEBHOOK_SECRET` signs short-lived, session-bound media URLs for every provider. Do not log these capability URLs.

Incoming calls create `phone:{provider}:{provider_call_id}` sessions. The provider connects to the returned WebSocket URL, caller audio is transcribed through the existing STT service, and the agent's TTS audio returns only to that provider socket. No LiveKit room/token is created for phone sessions. Each call has a bounded output channel, one media socket, barge-in clearing and shutdown cleanup. Twilio, Telnyx and Plivo use mono 8 kHz μ-law; Vonage uses mono 16 kHz signed little-endian PCM. These follow the provider protocols documented by [Twilio](https://www.twilio.com/docs/voice/twiml/stream), [Telnyx](https://developers.telnyx.com/docs/voice/programmable-voice/media-streaming), [Plivo](https://docs.plivo.com/docs/voice/xml/audio-streaming), and [Vonage](https://developer.vonage.com/en/voice/voice-api/concepts/websockets).

Phone audio, transcripts, tool events, recordings and session details are excluded from the browser event bus and web call/admin responses. Server-side recording and persistence still run. Browser call routes reject phone session IDs, so the web client cannot join, pause or end a phone call.

For `sip` and `custom`, the external gateway must translate its provider protocol into this contract:

- Send a POST JSON callback with `call_id`, optional `from` and `event: "start"`, authenticated using `Authorization: Bearer <TELEPHONY_WEBHOOK_SECRET>` (or `x-telephony-secret`). Send `event: "hangup"` when the call ends.
- Connect to the returned `media_url` within 30 seconds and send `{"event":"start","call_id":"<same call_id>","encoding":"pcm_s16le","sample_rate":16000,"channels":1}`.
- Send and receive mono 16 kHz signed little-endian PCM as binary WebSocket packets. Outbound audio is paced in 20 ms packets. On `{"event":"clear"}`, discard any buffered assistant audio.

`GET /api/telephony/status` reports the selected provider and whether its settings are present, without returning credentials. Real provider credentials, a routed phone number and a public HTTPS/WSS endpoint are required for a live call test; local protocol tests do not make billable calls.

The web app is served on port `3000`, the backend on `4000`, and the control-plane gRPC service on `50051` (metrics on `9090`).

## Repository layout

- `apps/web` — Next.js application
- `crates/services/server` — backend API
- `crates/services/control-plane` — cell control and tool execution service
- `crates/services/worker` — call summary worker
- `crates/libs/orchestrator` — session actors, LiveKit STS, recording assembly, and R2 upload
- `crates/libs/agent` — LLM agent and tool request handling
- `crates/libs/rag` — listing and knowledge retrieval
- `crates/libs/db` — domain store and Redis integration
- `proto` — gRPC protocol definitions

Control-plane admission, discovery, routing boundaries, and rollout requirements are documented in [docs/control-plane-routing.md](docs/control-plane-routing.md).

## PostgreSQL schema migrations

Run `python3 scripts/migrate-db.py` with Python 3 and a PostgreSQL `psql` client installed.
The runner reads `POSTGRES_DIRECT_URL` first, then `NEON_POOLER_URL`, `POSTGRES_URL`, or `DATABASE_URL` from the process environment,
then the backend `.env`, then the root `.env`. It applies SQL files in `migrations/`
in one transaction, records checksums in `public.schema_migrations`, and serializes
concurrent migration runs with an advisory lock. Use `--status` to list public tables.
Do not edit applied migration files; add a new numbered SQL migration instead.

Application reads and writes use these tables directly. Booking, notification outbox,
and booking retry responses commit in one transaction; PostgreSQL also enforces one
confirmed booking per listing/time slot. Audio WAV files remain in R2; their locations,
transcript events, user context, and generated summaries are stored in Neon.

For an installation upgrading from Redis call-history storage, run
`cargo run -p db --example import_legacy_redis` with the service environment loaded.
This imports existing summaries, recording metadata, and stream events without deleting
Redis data or overwriting existing Neon records. Re-running it does not duplicate events.
Export any process-local records from the old running service before replacing it;
process-local data cannot be recovered after that process has exited.

`cargo test -p db --lib -- --ignored` runs the opt-in PostgreSQL persistence test using
the configured service environment. It writes uniquely named fixtures, verifies reads
through new connections and concurrent booking protection, then deletes its fixtures.
The older end-to-end tests require their own seeded test database and external services;
do not point those tests at a production database.
