# FR-007: The tree walk looked like `-g`'s bottleneck, and wasn't

- Date: 2026-08-31

## What was hit

Investigating why `lane -g` felt slow from an external picker (a Herdr popup that shells out
to `lane -g --json` on every open), the obvious suspect was `disk_estimate`: it walks every
file in every lane's working tree, and the test fixture used for measurement was an 11k-file,
527 MB tree. Cold, that walk really does cost ~0.34 s — exactly what the shape of the code
suggests should dominate.

## Symptom

Measured against a release binary, warm page cache, one repository with one lane:

- `lane --version` (bare process startup): 6 ms
- `lane --list`: 34 ms
- `lane -g`: 105 ms

Stubbing `disk_estimate` to return `0` unconditionally and re-measuring warm `-g` changed the
timing **not at all**. The walk that looked like the obvious cost was already nearly free
once the OS had it cached — the 0.34 s figure only shows up cold.

## Resolution

Counted what `-g` actually does per lane instead of guessing from the code shape: it spawns
8 git subprocesses to build one row (`worktree list`, `symbolic-ref`, two `rev-parse
--verify`, a `rev-parse` for the fork point, `rev-parse @{upstream}`, `rev-list --left-right
--count`, `log -1`). That count is what scales with lane count and dominates the warm
timing; the tree walk dominates only the cold path. The cache added in ADR-013 stores the
whole computed row rather than the disk estimate alone, because caching only the estimate —
the thing that looked slow — would have added a cache for a number that was never the
bottleneck.

## What it informed

ADR-013 records the measurements and the decision to cache the whole row set rather than one
column of it; its "Alternatives considered" section keeps the caching-only-the-estimate idea
on record as the thing this entry demonstrated to be wrong.
