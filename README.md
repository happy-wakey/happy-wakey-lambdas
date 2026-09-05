# happy-wakey-lambdas

Consent-scoped ingestion and briefing assembly functions for Happy Wakey.
Connectors use provider-supported APIs, webhooks, or user-provided exports;
this repository does not bypass platform access controls or scrape private
feeds.

The provider-neutral Rust pipeline verifies Shared Auth claims, live connector
consent, fail-closed Ores Rate Limit grants, classifier/source consistency, and
the public deep-link policy before it emits a tenant-scoped NATS candidate.
Only bounded metadata goes to Ores OTEL. A security or customer escalation may
produce an opaque Ores Chat handoff containing a card ID—not the original
message. Opto Sync owns preference reconciliation.

`integration/dependencies.lock.json` records immutable revisions for
`ores-middleware`, `ores-rate-limit`, `ores-otel`, `opto-sync`, `ores-chat`, and
`shared-auth`. Public libraries are linked directly; private authentication,
rate-limit, and chat implementations remain behind service ports so public CI
does not require broad fleet credentials.

## Verify

```sh
python3 scripts/validate_repository.py
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
```
