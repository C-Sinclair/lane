# FR-008: Ignored entries collapse above the lanes directory, and lanes cloned each other

- Date: 2026-09-01

## What was hit

Creating a lane in a large Elixir/Phoenix monorepo (`arcc-center`, ~70 GB on disk) took
**2 minutes**, not the sub-second creation the COW clone layer is supposed to deliver. The
suspicion going in was the ignored-cache clone itself: `_build`, `deps` and
`assets/node_modules` are the entries lane exists to carry, and together they look like the
obvious cost.

## Symptom

```
$ time lane diag-probe-1
  reflink: yes (reflink available)
  1416354 files cloned (76405.9 MiB shared, 0 copied)
  1.89s user 83.53s system 71% cpu 2:00.05 total
```

76 GiB and 1.4 million files, where the repo's actual ignored caches are ~850 MB across
40,807 files. `du` on the repository explained the gap:

```
486M  _build      86M  deps      282M  assets
 20G  .wt         39G  .lane
```

`.wt/` is `git-wt`'s worktree directory; `.lane/` holds every lane already created. Both
are ignored, so both were being cloned into each new lane — and `.lane/` means every new
lane contains a copy of all its siblings, so the cost compounds with each lane kept.

## Resolution

`ignored_entries` filtered with `p != TREES_PATH`, i.e. `p != ".lane/trees"`. But
`git status --porcelain --ignored` collapses an ignored directory to its **shallowest**
root, and in this repo that root is `.lane`, not `.lane/trees` — so the guard never
matched. It only ever worked in the test fixture, where `.gitignore` named
`.lane/trees/` precisely and nothing else under `.lane/` was ignored.

The equality check became a containment check, and the same pass generalized it: an ignored
entry is dropped when it is, or contains, the lanes directory, **or** when it contains any
path that `git worktree list --porcelain` reports as a checkout of this repository. That
second clause covers `.wt/` and `.claude/worktrees/` without naming any other tool. The
`--dirty` materialization path, which builds its own skip closure rather than calling
`ignored_entries`, got the same pruning.

Same repo, after:

```
$ time lane diag-probe-2
  40807 files cloned (853.4 MiB shared, 0 copied)
  0.34s user 1.80s system 102% cpu 2.082 total
```

2m00s to 2.08s; 1,416,354 files to 40,807.

## What it informed

[DD-001](../design/dd-001-cow-clone.md) and [DD-002](../design/dd-002-worktree-lifecycle.md)
now state the rule as containment rather than equality, and record that a checkout is never
a cache. The unit test was rewritten to ignore `.lane/` wholesale — the shape that actually
occurs in the wild — since the old fixture's precise `.lane/trees/` ignore rule was what
hid the bug for so long. A fixture that only exercises the narrowest form of an input is
worse than no fixture, because it reads as coverage.
