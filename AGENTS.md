# Lambda boundaries

- Ingest only through provider-supported APIs, webhooks, or explicit
  user-provided exports. Do not scrape private feeds or bypass access controls.
- Shared Auth derives subject and tenant. Caller-supplied identity is never an
  authorization source.
- Require a live connector consent grant and fail closed when authentication,
  rate limiting, contract validation, or tenant isolation cannot decide.
- Raw credentials and private messages must not enter telemetry, NATS, Ores
  Chat handoffs, dead letters, fixtures, or Git history.
- Use `ores-middleware` for the request boundary, `ores-rate-limit` behind the
  rate-limit port, `ores-otel` for bounded telemetry, Opto Sync for preference
  reconciliation, and Ores Chat behind its tenant-scoped service port.
- A generated deep link must pass `happy-wakey-pub-lib-core`; no handler may
  mint or expose a generic social-feed URL.
