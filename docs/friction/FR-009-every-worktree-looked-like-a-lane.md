# FR-009: Every worktree looked like a lane, and `--prune` would have deleted them

- Date: 2026-09-01

## What was hit

`lane -g` felt slow from the Herdr popup even after [ADR-013](../decisions/adr-013-cache-the-whole-global-row-set.md)
added the row cache. The cache was working — a warm `-g --json` returns in 2 ms — but the
cold recomputation it falls back to whenever the 120-second TTL lapses took **22.7 seconds**,
and a picker opened a few minutes after the last one pays that in full. A TTL only bounds
staleness; it does nothing for the cost of a miss.

## Symptom

```
$ time lane -g --json      # cold
  6.28s user 25.70s system 140% cpu 22.727 total
$ time lane -g --json      # warm
  0.00s user 0.00s system 58% cpu 0.003 total
```

59 rows, in a registry of two repositories holding two lanes. The extra 57 explain
themselves:

```
arcc-center  /Users/conor/.../arcc-center/.wt/conor/arc-8-mass-rate-update   1d  1.4 GB  -7  landed
arcc-center  /Users/conor/.../arcc-center/.claude/worktrees/bold-rosalind-2748ca  ...  landed
```

`list_lanes` filtered `git worktree list --porcelain` to exclude the primary worktree and
nothing else, so every worktree in the repository was a lane: `git-wt`'s `.wt/`, an agent's
`.claude/worktrees/`, any hand-made `git worktree add`. Their names rendered as absolute
paths, because `name_of` falls back to the whole path when the lane directory is not a
prefix — visible in the output all along, and read as a formatting quirk.

The performance cost was the least of it. `--prune` walks the same list:

```
$ lane --prune --dry-run
would remove /Users/conor/.../arcc-center/.wt/conor/arc-8-mass-rate-update
... 12 lines
```

Twelve foreign worktrees, and their branches, one `lane --prune` away from deletion. Landed
and clean by lane's own reckoning, which is exactly the state a `git-wt` worktree sits in
between finishing a branch and tidying up.

## Resolution

Membership is by location: a lane is a worktree under `.lane/trees/`, which is the same rule
`create` uses to decide where to put one. `list_lanes` now filters on it, comparing
canonically because git reports `/private/...` on macOS where the repository root keeps the
shorter spelling — a naive comparison would have hidden every real lane instead. `name_of`
strips canonically for the same reason, so a name is never an absolute path again.
Recorded as [ADR-015](../decisions/adr-015-lane-membership-is-by-location.md).

That took `-g` to 2 lanes and 20.6 s — still slow, and now for an unrelated reason. The
remaining cost was one surviving lane created *before*
[FR-008](FR-008-ignored-entries-collapse-above-the-lanes-directory.md)'s fix, so it still
held the nested checkouts that bug cloned into it: 37 GB across 709,609 files.
`disk_estimate` walked every one, stat-ing it against the main tree, and counted almost
nothing — a cloned `.wt/foo/lib/x.ex` matches the real `.wt/foo/lib/x.ex` by size and mtime,
so it is correctly judged shared. 1.4 million syscalls to add zero to a sum.

`disk_estimate` now prunes any directory holding a `.git` entry. One stat per directory
buys skipping whole nested working trees, and the rule needs no git invocation to apply.

```
$ time lane -g --refresh --json
  0.10s user 0.92s system 50% cpu 2.036 total
```

22.7 s to 2.04 s cold, 2 ms warm.

## What it informed

[ADR-015](../decisions/adr-015-lane-membership-is-by-location.md) records the membership
rule and why the fallback in `name_of` was the wrong kind of tolerance.
[DD-002](../design/dd-002-worktree-lifecycle.md) and
[DD-003](../design/dd-003-landing-detection.md) state it where they describe listing and
pruning.

Two lessons worth keeping separately from the fix:

- **A cache measured only warm hides the cost of a miss.** ADR-013 measured `-g` at 105 ms
  against a fixture with one lane, and every number in it was true. It could not have
  revealed a 22-second cold path, because the fixture had no foreign worktrees to
  misclassify. Measuring on a real repository is not the same as measuring on a big one.
- **Absolute paths where names belong were the bug announcing itself in the output.** They
  were legible in `lane --list` for as long as the misclassification existed, and read as
  cosmetic. A fallback that turns a failed assumption into slightly odd output, rather than
  an error, buys silence at the price of the diagnosis.
