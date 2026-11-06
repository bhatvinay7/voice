# Cell admission and control-plane routing

The cell is the authority for capacity. Scheduler state is a disposable view of
connected cells and is not a shared allocator or a durable call directory.

## Implemented

- Sled free-slot and live-route trees are modified together in transactions.
  All local and gRPC admission paths share one cell registry. A live route wins
  over a stale free entry, duplicate admission is idempotent, and full/draining
  cells reject requests. The HTTP API returns 503 when admission fails.
- An orchestrator Tokio task maintains a `DelayQueue` with a fixed 25-minute
  deadline from each route's registration timestamp. Media activity updates the
  separate heartbeat timestamp but cannot extend this deadline. Normal release
  removes the timer; an expired deadline marks the route for cancellation, with
  retries if the event queue is full. Slots stay occupied until actor shutdown
  completes. Cleanup atomically deletes the live Sled route and returns its slot
  to the free pool, waking every scheduler connection's async capacity publisher.
  Storage errors fail admission closed.
- A cell connects to every address returned by `CONTROL_PLANE_DISCOVERY_ADDR`
  (host:port). Without it, `CONTROL_PLANE_ENDPOINT` remains the single-endpoint mode.
  Capacity changes wake every connection immediately; periodic heartbeats run
  every ten seconds. The scheduler drops streams silent for 35 seconds.
- `CellControl.Start` routes `session_<UUID>_<cell-pod-name>` to that cell's
  stream, waits for the admission result, and correlates replies with a request
  number. `Stop` queues cancellation to that same cell. Stream replacement is
  fenced by channel identity, so old disconnects cannot delete new connections.
- Explicit admission rejection is safe to retry elsewhere with a new routed ID.
  A deadline means the outcome is unknown: retry the SAME routed ID. An ID is
  routing metadata, never authorization. Keep control-plane gRPC on a private trusted
  network; no public ingress is configured for it. End-user authorization remains
  at the HTTP/provider entrypoint.
- `/metrics` on `CELL_METRICS_PORT` exposes `voice_agent_slots_used` and
  `voice_agent_slots_total`. Scrape cells, not the summed scheduler snapshots.

## Deliberate compatibility boundary

Existing browser call IDs and `phone:<provider>:<call-id>` remain unchanged.
Those HTTP entrypoints still admit locally. The new routed control API does not
move their sockets or stream audio through schedulers. There is no automatic
cross-cell allocator retry or bidirectional media relay in this change. The
scheduler's `find_best_cell` is a capacity hint, not a reservation.

Before enabling multiple backend replicas, implement a trusted ingress/relay
that obtains cell acceptance, stores or returns the routed identity, sends all
subsequent provider callbacks and audio to that owner, and returns audio only
to the matching caller. Phone sessions must retain their isolated output route.
Do not simply put the current callback endpoints behind random load balancing.

KEDA cannot choose which pod Deployment removes. The supplied scaler remains
paused at one backend and disables automatic scale-down. A drain-aware controller
and application shutdown handling are also needed before enabling safe removal.
Pod failure still disconnects live sessions; local disk is not replication.

## Session event and graph release invariant

The dispatcher uses bounded, non-blocking delivery for ordinary session events.
If a session inbox is full, only that session's best-effort event is dropped.
`SessionCancelled` is control-plane data: when its inbox is full, the event is
requeued asynchronously until it is accepted or the global queue closes. The
session then releases its in-memory workflow graph, cancels task-owned work,
closes provider media, and returns its slot. The same path is used by the
25-minute session reaper. Durable admin graph definitions and the existing S3
and Redis fanout remain untouched.

## Verification

`cargo test -p scheduler` covers slot contention, duplicate release, stale free
entries, registration expiry, gRPC admission, replacement fencing, and the
current tool contract. Workflow and configuration tests cover session graph
release and `STS_MODE` parsing. Full LiveKit-backed server test compilation
requires the host `glib-2.0` development package.
The apps Kustomization can be rendered after provisioning its two generated
secret manifests. Never use live credentials for rendering or unit tests.
