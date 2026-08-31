# DD-003: Landing detection

How lane decides whether a lane's work has reached trunk, with no marker of its own.

- Status: current
- Date: 2026-08-31

## Summary

Lane keeps no record of what merged. "Landed" is answered entirely from git refs, at read
time, by `wt::landed()`. Two independent proofs are checked, either sufficient:

1. **The branch's upstream is gone.** `for-each-ref --format=%(upstream:track)` reports
   `[gone]` when the branch this lane's branch tracks was deleted on the remote — the
   ordinary shape of "the PR merged and GitHub deleted the branch".
2. **Trunk contains the branch's commits.** `contained_in()`, below.

A lane that has committed nothing is excluded first (`started()`): its tip equals its fork
point, so it can never be "contained" in a way that means anything, and treating it as
landed would prune a lane nobody has touched yet.

## Why containment can't be one git call

The obvious check — `merge-base --is-ancestor branch trunk` — only answers yes for a fast-
forward or an ordinary merge commit. Two real merge strategies defeat it:

**Squash merge changes the commit, not just its ancestry.** The merged commit on trunk has a
different sha and a different parent than anything on the lane's branch, so ancestry alone
never matches. `contained_in()` falls back to a synthetic probe: build a commit with the
lane branch's tree on top of the merge-base, and check whether *that* commit's patch-id
matches something reachable from trunk. This mirrors what a squash merge actually preserves
— the resulting tree — rather than the discarded intermediate history.

That probe still has a false-negative mode: if anything else lands on trunk between the
lane's fork point and its own merge, the squash's diff against *trunk's current state* can
differ from the lane's diff against *its own fork point* by exactly the intervening changes
(a line of context moved, a symbol's visibility changed underneath it), which changes the
patch-id despite the actual work matching. There is no full fix for this from patch
comparison alone; the remote-retired check (proof 1) covers the common case where a squash
PR's branch also gets deleted, which is why landing is never decided from patch-id alone.

**Rebase merge replays commits one at a time**, so a branch that rebased and merged can have
several distinct patch-ids, none of which is "the" squash patch. `contained_in()` checks this
case with `git cherry trunk branch`: every line prefixed `-` means every commit on the branch
already has an equivalent patch reachable from trunk.

**An empty diff has no patch-id to match.** If the branch's tree at its tip is identical to
the tree at its merge-base, `cherry`'s patch-id approach can't express "nothing changed here"
— `contained_in()` special-cases this by comparing trees directly.

## What this deliberately does not do

There is no landing marker, no state file, and no command that "does" a landing — landing is
git merging or pushing, done by the user or their normal tooling, and lane only ever
observes the aftermath. See ADR-002 for why detection is read-only, and ADR-001 for why the
tool that used to write such a marker (as part of a `lane done` that rebased, audited, and
merged in one step) is gone entirely.

A branch that keeps committing *after* its own merge — for instance, a squash lands, then
two more commits ship separately through a second lane — is not specially handled: each
commit's fate is judged on its own patch-id independently, which is the correct behaviour
for "has this specific content landed", even though the branch as a whole no longer reads as
one clean unit.
