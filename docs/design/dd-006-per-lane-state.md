# DD-006: Per-lane state

The one thing lane persists about a lane, and where it lives.

- Status: current
- Date: 2026-08-31

## Summary

The only state lane keeps per lane is the commit it forked from — its **fork point** —
stored as a git ref at `refs/lane/<name>`, written when the lane is created and deleted with
the lane. Nothing else about a lane is persisted anywhere; everything else (its worktree
path, its branch, whether it's landed) is either implicit in git's own worktree/branch state
or computed at read time (DD-003).

## Why a fork point needs to exist at all

A lane whose branch tip equals trunk is ambiguous without it: that's either a lane that has
never committed anything, or a lane whose work has been fully squash-merged and whose branch
has since been reset or rebased to match. `started()` (`worktree.rs`) resolves the ambiguity
by comparing the branch's current tip to its recorded fork point — if they still match,
nothing has been committed since creation, regardless of what trunk looks like now.

A lane this tool did not create — one with no recorded fork point — is treated as
`started()` unconditionally, which means it is judged on its refs alone, exactly as every
lane was before fork points existed. Since `landed()` is `started() && (upstream retired ||
contained in trunk)`, answering yes leaves such a lane *collectable*: an unrecorded lane
sitting at trunk's tip can be pruned. The alternative — treating an unrecorded lane as
never-started — would make it permanently uncollectable, which is the worse failure, and the
exposure from answering yes is bounded, because `losses()` still refuses any lane holding
uncommitted work or commits trunk does not have.

## Why a ref, not a config value

`refs/lane/<name>` is a real git reference, not a string in `.git/config`. This is a
deliberate correction of an earlier design that stored the same information as
`lane.<branch>.fork = <sha>` in git config — see ADR-003 for the full account, and FR-002
for how it was demonstrated to actually lose data (`git gc` can collect a commit a config
string merely *names*, since config is opaque to git's reachability analysis; a ref keeps
its target reachable by definition).

## Scope

The ref is local only — it is never pushed, never fetched, and carries no information other
than which commit to compare against. It is removed as part of deleting the lane (`lane -d`/
`-D`), so there is no separate garbage-collection pass and no state left behind by a lane
that no longer exists.
