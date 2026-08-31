# ADR-003: Store the fork point as a ref, not a git-config value

- Status: accepted
- Date: 2026-08-31

## Context

Lane needs to remember one thing per lane: the commit it forked from, so it can tell a lane
that has never committed from one whose work has been fully squash-merged and reset to
match trunk (DD-006). Somewhere has to hold that commit sha for the life of the lane.

## Decision

Store it as a real git reference, `refs/lane/<name>`, created with the lane and deleted with
it. Not a `git config` value.

## Alternatives considered

**A git-config value**, e.g. `lane.<branch>.fork = <sha>`. This was tried and demonstrated
to lose data in both directions (FR-002):

- A config string naming a commit is opaque to git's reachability analysis. Resetting the
  base branch and running `git gc --prune=now` collected the named commit while config
  still pointed at it — the fork point silently became a dangling reference to nothing.
- Deleting the per-lane config key when a lane was removed was never wired up, so
  `.git/config` accumulated dead `lane.*` keys forever — three created-and-removed lanes
  left six stale entries with no code path that ever cleaned them.

A ref does not have either problem. `refs/lane/<name>` keeps the commit it names reachable
by definition — that's what a ref *is* to git — so gc can never collect out from under it.
And because the ref lives at a name derived from the lane, deleting the lane's ref is one
call in the same code path that already removes the lane's branch and worktree, with no
separate bookkeeping to forget.

## Consequences

- The fork point can never silently go stale relative to what gc has collected — it is
  either present and correct, or the lane (and its ref) don't exist.
- Refs are cheap and git already has fast machinery for creating, resolving, and deleting
  them by name; no new storage format was needed.
- The ref is local-only by convention (never pushed or fetched), which needs to stay true
  as long as `refs/lane/*` isn't given a reason to be shared — see ADR-005 for why that's
  judged unnecessary today.
