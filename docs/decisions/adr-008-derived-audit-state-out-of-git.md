# ADR-008: Keep derived audit state out of git entirely (removed)

- Status: superseded by ADR-001
- Date: 2026-08-31 (recording history)

## Context

The immutable-note split (ADR-006) put mutable audit state — fingerprints, review verdicts,
read counts — into per-branch files, committed to git alongside the notes themselves. That
state was derived, not authored, and committing derived data into a version-controlled
store that multiple branches wrote to independently produced a sustained run of bugs, each
patched and each revealing the next:

- **The audit erased the drift it had just found.** `lane audit` overwrote the stored
  fingerprint with the *current* hash unconditionally, before a reviewer's verdict had a
  chance to act on the difference — so the next check compared current code against a
  baseline taken from current code, and drift a person had just been told about vanished
  the moment they looked away.
- **Landing erased it again, a different way.** The fix for the bug above worked inside a
  lane, but `Checker` read state merged across every branch while `record_state` wrote to
  the lane's own file alone — two different views of the same concept — so a drifted note
  that correctly stayed flagged inside the lane came out clean the moment the lane landed.
- **A landing lock was needed, and still wasn't enough.** `lane done` rewrote *trunk's*
  state file from inside the lane's own worktree, folding the lane's entries in; a lock
  serialized this against other local `lane done` runs, but a pull request merged on GitHub
  bypassed the lock entirely, leaving orphaned per-branch state on trunk with nothing to
  fold it and nothing to eventually clean the log half of it up.
- **The garbage collector destroyed confirmed decisions.** State for any branch with no
  local ref was deleted on the next audit — which is exactly the situation every merged pull
  request leaves on a collaborator's machine, so a `lane holds` confirmation, stored nowhere
  but that state, vanished the first time anyone next ran an audit.
- **A half-written state file was unrecoverable.** The write path truncated the file before
  writing the replacement, so a crash or full disk mid-write left a JSON prefix that could
  not be parsed, taking every fingerprint for that branch down with it. Fixed by writing to
  a sibling temp file and renaming over the target, which is atomic within a filesystem.

## Decision

Move derived state out of git-committed storage. The clearest step in this direction
(plan 033) dropped the shared append-only log (`landing`/`evict`/`holds` records) entirely
and gave each note its own baseline instead, since notes were already one file each and had
never conflicted — the log's only genuinely shared content, `holds`/`rebaseline`, moved onto
the note it was about. `lane holds` became a decision the note itself carried, not an entry
in a separate ledger. This was the last landed step before the whole subsystem was removed.

## What removal traded away

- A shared record of what an audit found, across branches, without every branch needing to
  re-derive it from scratch.
- The fixes above are a real account of what it costs to put multiple independent writers'
  derived data into git as the source of truth: every partition of "who writes what, when"
  needs its own serialization story, and git's merge machinery does not help with any of
  them, because none of it is content a human author actually wrote by hand.

The corrected end state — one file per note, carrying its own baseline, no shared log — is
recorded here because it's the most defensible point this subsystem ever reached, even
though none of it survives ADR-001.
