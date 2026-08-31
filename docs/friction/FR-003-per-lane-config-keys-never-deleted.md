# FR-003: Deleting per-lane config keys was never wired up

- Date: 2026-08-31

## What was hit

Under the git-config-based fork point design (FR-002), removing a lane never removed the
config key that had been written for it.

## Symptom

Creating and removing three lanes in sequence left six dead `lane.*` keys in `.git/config` —
nothing in lane's deletion path ever touched them, so the file grew without bound as lanes
churned.

## Resolution

Not patched directly — informed the same move as FR-002: switch to `refs/lane/<name>`, where
removing the ref is part of the same deletion call that already removes a lane's branch and
worktree, rather than a second thing to remember to wire up.

## What it informed

ADR-003 (fork point as a ref). This is one of the two concrete failures that motivated
moving off git config for this state; FR-002 is the other.
