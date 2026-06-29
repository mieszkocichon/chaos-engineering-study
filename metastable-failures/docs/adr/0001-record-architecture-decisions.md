# ADR-0001: Record architecture decisions

- **Status:** Accepted
- **Date:** 2026-06-28

## Context / Background

We changed direction twice in a single session (pivoting from an adaptive-gossip sim to metastable-failures), changed branches a bunch of times, and made some pretty consequential research choices. 
If we don't write this down, the *reasons* for the current setup—especially why we abandoned earlier ideas—will get lost. 
Future us (or reviewers) would prob just repeat the same dead ends.

## Decision

Maintain an Architecture Decision Record log under
`metastable-failures/docs/adr/`. One record per significant decision, in
lightweight MADR format (Status, Context, Decision, Consequences, Alternatives,
Related). Record superseded decisions too; mark them `Superseded by NNNN` rather
than deleting them.

## Consequences

**Pros**
- The rationale behind the pivot from Frugal Gossip to metastable failures is saved.
- Reviewers can see we're honest about our dead ends.
- Easier onboarding (people can read decisions, not just code).

**Cons**
- Small overhead: gotta write down every big decision (kinda tedious ngl).
- Might get stale if we forget to update them (hopefully `Superseded by` helps here).

## Alternatives

- **No ADRs, rely on git history and prose notes.** Rejected: commit messages and
  the `notes/` folder capture *what* but not the considered-and-rejected
  alternatives that make a decision legible later.

## Related

- All subsequent ADRs.
