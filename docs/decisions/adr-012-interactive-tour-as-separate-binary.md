# ADR-012: Ship an interactive tour as a separate binary (removed)

- Status: superseded by ADR-001
- Date: 2026-08-31 (recording history)

## Context

Lane's value, particularly the memory subsystem's (ADR-006), was hard to see from a README
alone — it only became apparent once several lanes existed at once, a note had drifted, and
landings had interleaved, none of which a reader would set up by hand just to satisfy their
curiosity. A long-running interactive program built a disposable sandbox repository, printed
its path, and let the reader drive a numbered menu of scenes, each printing the real lane
command before running it.

## Decision

Ship it as its own binary (`crates/example`, the `lane-tour`-style crate referenced in later
plans), driven by a scene table (`scenes.rs`) that was treated as fixed content the
implementing plan was explicitly forbidden to rewrite, reword, or reorder — the plan built
only the sequencing driver around it. The constraint that it must not add a single byte to
the `lane` binary itself was explicit from the start: this was a teaching aid, not a product
surface, and was never allowed to affect the thing it was teaching.

## What removal traded away

- The only piece of documentation that demonstrated the memory subsystem's cross-lane,
  cross-time behavior by actually running it, rather than describing it in prose.
- Nothing about this pattern (a fixed-content scene table plus a thin sequencing driver, in
  a separate crate with a hard boundary against affecting the real binary) is specific to
  the feature it taught — it would work equally well as a teaching aid for whatever lane's
  surface looks like in the future, should an interactive walkthrough be worth building
  again.
