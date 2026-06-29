# ADR-0004: Run all builds and simulations via Docker, no host toolchains

- **Status:** Accepted
- **Date:** 2026-06-28

## Context / Background

Dev host (Win 11) has zero toolchains installed. No Go, Node is only in containers. Not installing them locally, hard pass. just no. Also, artifact evaluation needs one-command repro anyway, so "works on my machine" won't fly.

## Decision

Run/build all the things in pinned Docker images:
- Go simulator (`frugal-gossip/`): `golang:1.22-bookworm`. Named volume for module cache so rebuilds don't take forever.
- Node testbed & metastable harness: `node:20-bookworm`, bind mount repo at `/app`.

Gotta remember to start Docker Desktop first. Daemon takes a minute to boot (ugh).

## Consequences

**Pros**
- Clean host. Pinned versions. Reproducible.
- Artifact eval ready.

**Cons**
- First pull is slow.
- PowerShell -> docker `sh -c '...'` truncates stdout after the first word. Super annoying. Fix: dump multi-step commands into `scripts/*.sh` and run `sh scripts/file.sh`. Works fine. Now standard practice for container commands.

## Alternatives

- **Install Go/Node locally.** Nope. Worse for pinning anyway.
- **Inline `docker run ... sh -c`.** Nope, stdout gets truncated. Use scripts.

## Related

- ADR-0003, ADR-0009.
