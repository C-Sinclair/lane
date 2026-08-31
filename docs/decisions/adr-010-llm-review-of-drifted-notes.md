# ADR-010: Take the review model out of the binary, verdicts as verbs (removed)

- Status: superseded by ADR-001
- Date: 2026-08-31 (recording history)

## Context

`lane audit` could send a drifted note (ADR-006) to a language model and apply its verdict
automatically: `holds` refreshed the note's fingerprint, `superseded` wrote a brand-new note
in the model's own words and retired the old one, `contradicted` evicted it. This was off by
default — nothing broke for a user who never set `ANTHROPIC_API_KEY` or `LANE_REVIEW_CMD` —
but where it was on, it quietly broke the design principle the rest of the notes subsystem
was built on.

The tool's own explainer argued a note is worth keeping precisely *because* a person decided
to write that sentence — a note nobody chose to write would just be the git log again, the
exact thing the subsystem was designed to refuse to become. Automated review was the one
place the tool violated that rule on itself: a `superseded` verdict produced a note that was
byte-for-byte indistinguishable, in storage, from one a person had typed — same branch,
same date, same rendering under `lane why` — with the only trace of its true origin a
`kind: "verdict"` log record nobody was likely to read six months later.

The verdicts also weren't exposed as first-class operations: `refresh_holds` was private and
reachable only from the review path, `lane note` couldn't supersede anything on its own, and
`lane check --json` emitted pointers rather than actionable work items. The model ended up
living inside the binary partly because there was nowhere else for its output to go.

## Decision

Correct the design rather than remove the feature outright (removal came later, with
ADR-001): mark model-authored notes as such, and make every verdict a verb a person could
also invoke directly — `confirm`, `retire`, `replace` — so review became one caller among
several of the same lifecycle operations (ADR-007), not a privileged path with its own
side effects.

## What removal traded away

- Automated triage of stale notes for anyone willing to point the tool at a model.
- The corrected design — verdicts as ordinary lifecycle verbs, with authorship marked — is
  worth remembering as the shape any future automated-assistance feature in this tool
  should take: never let automation produce output indistinguishable from something a
  person deliberately wrote.
