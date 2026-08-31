# ADR-005: Not adopting Radicle-style collaborative-object storage

- Status: accepted
- Date: 2026-08-31

## Context

Radicle and similar tools represent shared, mutable state (identities, patches, issues) as
signed, replicated git objects designed for multiple collaborators to write to the same
logical record without a central server arbitrating merges. Lane's own per-lane state
(DD-006) is a single commit sha, naming where a lane forked from.

## Decision

Do not adopt a collaborative-object storage model for lane's state. The fork point stays
exactly what ADR-003 already made it: one ref per lane, written once by the person who
created it, read by nobody else.

## Alternatives considered

**Collaborative-object storage**, evaluated and set aside rather than built. It solves a
problem lane doesn't have: multiple authors writing to the same record concurrently. A
lane's fork point has exactly one writer (whoever created the lane), is never shared (the
ref is local-only, ADR-003), and never needs a merge — there's nothing to reconcile between
two people's view of the same lane, because a lane belongs to one person's checkout.

## Consequences

- A ref is already the minimal correct representation of this state; there's no simpler
  design left to reach for, and no more complex one is justified by anything lane currently
  does.
- This is explicitly revisitable. If lane grows **stacked lanes** — one lane built on
  another, unlanded lane, where more than one person's view of "what forked from what"
  needs to be reconciled — the single-writer assumption behind this decision stops holding,
  and collaborative-object storage (or something with similar properties) becomes worth
  evaluating again for real, not hypothetically.
