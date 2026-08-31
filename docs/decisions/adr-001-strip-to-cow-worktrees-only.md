# ADR-001: Strip the tool to copy-on-write worktrees only

- Status: accepted
- Date: 2026-08-31

## Context

Lane had grown a second, much larger subsystem alongside its original job of making git
worktrees cheap: a notes/memory system that anchored durable findings to files and symbols,
audited them for drift, optionally sent drifted ones to a language model for a verdict,
captured decisions from commit trailers, and installed itself into a repository's agent
skill and git hooks. It also had a full subcommand CLI to drive all of that
(`lane new`, `lane ls`, `lane rm`, `lane note`, `lane audit`, `lane why`, `lane done`, …).

That subsystem accreted real complexity: immutable note files with per-writer mutable
state, a landing lock to serialize concurrent writers, a rollup step to fold per-branch
state into trunk, a garbage collector for the rollup's leftovers, an LLM review pipeline,
and a machine-readable command family (`lane note add/replace/confirm/retire/restore`,
`lane anchors`) built to give agents a stable, structured way to drive all of it. Plans
013, 024, 025, 026, 027, 030, 032, and 033 alone are a sequence of bug fixes and redesigns
chasing the same underlying problem: derived, mutable, per-branch state stored inside git
and shared across branches does not merge cleanly, no matter how it's partitioned.

## Decision

Delete the notes/memory subsystem in its entirety — storage, audit, drift detection, LLM
review, commit-trailer capture, the note lifecycle command family, the installable agent
skill and hooks, and the interactive tour that taught it. Reduce lane to exactly what its
name says: copy-on-write git worktrees, entered and created by one command, listed,
deleted, and pruned.

The CLI surface was rebuilt at the same time from a subcommand grammar to a flag-only one
(ADR-004) — the two changes shipped together because removing most of the subcommands left
too few to justify keeping the grammar that adjudicated between commands and lane names.

## Alternatives considered

**Keep memory, keep fixing it.** The bug sequence above was trending toward a design that
worked (032/033's move to a per-note baseline with no shared log was the last and most
promising iteration). But every fix added a new invariant the CLI, storage layer, and both
readers (`Checker` and `audit::run`) all had to agree on simultaneously, and each fix so
far had uncovered another instance of the same class of bug. The subsystem was not
converging on simple.

**Keep memory, make it opt-in.** Gating the whole subsystem behind an init flag would have
kept the code and its maintenance burden without the value proposition of it being on by
default — untested opt-in features rot fastest.

## Consequences

- Lane is now a much smaller tool: one job, no LLM dependency (`ANTHROPIC_API_KEY`/
  `LANE_REVIEW_CMD` are gone along with everything that read them), no state file that can
  half-write or conflict, no marker that can go stale.
- Every plan describing the memory subsystem's mechanism now describes something that does
  not exist. ADR-006 through ADR-012 record what existed and why it's gone, so that history
  isn't lost even though the mechanism isn't preserved as live design.
- Agents lose durable, file-anchored memory that survived across sessions and lanes. What
  they keep is `AGENTS.md` itself, maintained by hand, and this `docs/` tree.
- The traded-away decision-capture path (a `Why:` trailer parsed off commits, ADR-009) has
  no replacement. A reason for a change now lives only in the commit message itself, or
  wherever the author chooses to write it down — including this friction log.
