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

## Repository-local Git worktrees

- Create or use a Git worktree only when the human operator explicitly authorizes it for the current task. Concurrency or a dirty checkout is not permission by itself.
- Put every authorized worktree at `<repository-root>/tmp/worktrees/<name>`; from the repository root, use `./tmp/worktrees/<name>`. Never place worktrees beside repositories or organization directories.
- Keep `tmp`, `temp`, `tmp/worktrees`, and `temp/worktrees` ignored in the repository-root `.gitignore`. Do not commit files from those directories.
- Relocate or remove a worktree only when the operator explicitly requests it. Before removal, preserve and publish intended changes, verify its commit is represented on the target branch, and confirm there are no tracked, untracked, ignored-sensitive, or in-use files that must survive. Remove it with `git worktree remove <path>` without `--force`; never delete a worktree directory with `rm`.
