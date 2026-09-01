# DD-002: Worktree lifecycle

How a lane is created, entered, listed, and removed.

- Status: current
- Date: 2026-08-31

## Summary

A lane is a git worktree at `<repo>/.lane/trees/<name>` plus a branch of the same name.
`lane <name>` creates it if absent and enters it either way; `lane -d`/`-D` removes it;
`lane --prune` removes every lane whose branch has landed. Lanes live inside the repository
they belong to and are addressed with relative git-dir pointers, so moving the repository
does not break them.

## Location and addressing

Lanes are created under `.lane/trees/<name>`, inside the repository, not in a sibling
directory. `git worktree add` defaults to an absolute `gitdir:` pointer; lane overrides that
so both the lane's `.git` file and the admin worktree entry under
`.git/worktrees/<name>/gitdir` use relative paths (`../../.git/worktrees/<name>` and
`../../../.lane/trees/<name>/.git` respectively). A repository plus its lanes can be moved
as a unit without invalidating either side of the link.

`.lane/trees/` is excluded via `.git/info/exclude`, not `.gitignore` — the exclusion is
local and never committed, matching the fact that lanes are a local, per-machine concern.

## Creating a lane

`lane <name> [-b|--base <rev>] [--dirty]`:

1. Resolve the branch. An existing local branch of that name is adopted as-is. Otherwise,
   with no `--base`, a name matching exactly one remote-tracking branch across the
   configured remotes branches from it and tracks it
   ([ADR-014](../decisions/adr-014-a-lane-name-adopts-a-matching-upstream-branch.md));
   more than one match is an error naming the remotes. Only refs already fetched count —
   creating a lane never reaches the network. Failing all that, the base is `--base <rev>`
   if given, otherwise the current branch, or the repository's default branch when HEAD is
   detached (`wt::trunk_name`).
2. `git worktree add` the new branch and directory.
3. Record the fork point at `refs/lane/<name>` (see DD-006).
4. Clone in the warm set — every git-ignored entry that is not itself a checkout, plus
   dirty/untracked-non-ignored content under `--dirty` — via the copy-on-write layer
   (DD-001).

`--base` and `--dirty` only apply at creation; re-running `lane <name>` against an existing
lane just enters it.

## What counts as a lane

A lane is a worktree under `.lane/trees/` — the same rule `create` uses to decide where to
put one. A repository routinely holds worktrees lane did not create (`git-wt`'s `.wt/`, an
agent's `.claude/worktrees/`, a hand-made `git worktree add`), and those are not lanes:
listing them misreports them, and `--prune` would delete them and their branches
([ADR-015](../decisions/adr-015-lane-membership-is-by-location.md)). `list_lanes` applies
the rule once and every command reading it inherits it.

Paths are compared canonically, since git reports `/private/...` on macOS where the
repository root keeps the shorter spelling.

## Listing

`lane` with no operation flag (or `-l`/`--list`) lists every lane from
`git worktree list --porcelain`, minus the primary worktree and anything the membership rule
above rejects. Each row reports:

- **state** — `open`, `pushed`, or `landed` (DD-003 covers how this is decided)
- **dirty** — computed in parallel across lanes via a scoped thread per lane, since
  `git status` per worktree is the dominant cost of listing many lanes

`--json` emits the same rows as structured data.

## Deleting

`lane -d <name>...` refuses to remove a lane that would discard work: uncommitted changes,
or commits trunk does not have. `losses()` reports what would be lost without guessing a
count when the branch was squash-merged, since a squash makes `rev-list` count patches that
already landed under a different sha. `-D`/`--force-delete` deletes anyway.

## Pruning

`lane --prune` fetches (best-effort — a failed fetch degrades to deciding on cached remote
refs rather than blocking) and removes every lane whose branch has landed (DD-003), skipping
any that would still lose uncommitted or unlanded work. `--dry-run` reports what would be
removed without touching anything.

## Entering and exiting

`lane <name>` on an existing lane, and `lane --exit`, both print the destination path.
Actually changing the calling shell's directory needs the shell integration described in
DD-004 — the lane binary itself can only ever move its own process, never its parent shell.
