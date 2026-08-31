# FR-002: A config value naming a commit is not a reference

- Date: 2026-08-31

## What was hit

An earlier design stored a lane's fork point as a git-config value:
`lane.<branch>.fork = <sha>`. Demonstrated to lose data in both directions.

## Symptom

**Direction one — gc collects what config merely names.** Resetting the base branch and
running `git gc --prune=now` collected the commit the config value pointed at, because a
string in `.git/config` is opaque to git's reachability analysis — nothing about it tells
gc "keep this commit alive." The fork point silently became a dangling reference to a
commit that no longer existed anywhere.

**Direction two — deletion was never wired up.** Removing a lane's config key on lane
deletion had no code path at all, so `.git/config` accumulated dead `lane.*` entries
indefinitely: three lanes created and removed in sequence left six stale keys behind, with
nothing that would ever clean them.

## Resolution

Store the fork point as a real git reference, `refs/lane/<name>`, instead. A ref keeps the
commit it names reachable by definition, so gc can't collect out from under it; and because
deleting a lane already has one code path that removes its branch and worktree, deleting its
ref is a natural addition to that same path rather than a second bookkeeping mechanism that
can be forgotten.

## What it informed

ADR-003 (fork point as a ref, not a git-config value) — this is the demonstrated failure
that decision exists to avoid. DD-006 describes the resulting mechanism.
