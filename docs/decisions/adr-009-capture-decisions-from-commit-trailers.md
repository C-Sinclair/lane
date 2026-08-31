# ADR-009: Capture decisions from a commit trailer, not the git log (removed)

- Status: superseded by ADR-001
- Date: 2026-08-31 (recording history)

## Context

`lane note` (ADR-006) had to be remembered, in a separate command, at the moment you
understood something worth keeping. The reason for a change is typed anyway, into the
commit message, and was then thrown away as far as the memory store was concerned.

The tempting fix — read the git log and derive notes from it — was explicitly rejected as a
design direction. A commit message says what changed, once; a note says what must stay
true, and outlives the step that established it. Importing commit messages wholesale would
bury the second kind of statement in the first and make `lane why` useless, judged worse
than not having the feature at all.

## Decision

Add one narrow, explicit place to record a decision inside a normal commit: a `Why:`
trailer, shaped so an ordinary commit summary can't fit in it —

```
make verify constant-time

Rewrites the early-return path so both branches do the same work.

Why: src/auth.rs#fn verify | early return leaks token length
```

— captured by a `post-commit` hook lane installed, into a queue and then a note (see below).

This queue moved twice before the feature was removed entirely. It first lived at
`.wt/pending.jsonl` — `.wt` being the tool's name before it was called `lane`, undocumented
and inconsistent with everything else that had moved to `.context/`/`.lane/` — and being
gitignored, a new lane silently inherited its parent's *unpromoted* notes, since lane clones
everything git ignores by reference (DD-001). Moving the queue to a location outside the
worktree (plan 018) fixed the inheritance problem by removing the mechanism that caused it —
until real notes were lost by a different path entirely: two lanes captured trailers,
were pushed and later removed without ever being promoted through a landing command, and the
queue — never itself a git object — went with the deleted worktree. Neither note had ever
existed as a file, so git had no copy of either. The final design (plan 038) removed the
queue altogether: a capture wrote the note file immediately, with **no baseline** — baselining
at capture time would anchor a note against content a later rebase might rewrite, showing
drift that never happened, so the first real baseline was deferred to the first audit after
any rebase had settled.

## What removal traded away

- The lowest-friction way to record a decision that existed: writing it as part of a commit
  you were already making, rather than reaching for a separate command.
- The queue's final shape — write straight to the note file, no baseline until first audit —
  is worth keeping in mind if anything like this returns: the two real data-loss incidents
  behind this ADR were both caused by treating an unpromoted decision as safe to leave
  un-committed, in a gitignored location, for any length of time.

See FR-002 through the friction log for other cases where storage-that-looks-durable turned
out not to be, informing the eventual move toward not committing derived state at all
(ADR-008).
