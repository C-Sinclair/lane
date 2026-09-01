# ADR-013: Cache the whole `-g` row set, not just the disk estimate

- Status: accepted
- Date: 2026-08-31

## Context

`lane -g --json` is invoked from an external picker (a Herdr popup, in another repository)
that shells out to it on every keystroke or open; its latency is felt directly by a human
waiting on a menu to populate. Measured on a release binary, warm page cache, one registered
repository holding one lane (an 11k-file, 527 MB working tree):

- `lane --version` (bare process startup): 6 ms
- `lane --list`: 34 ms
- `lane -g`: 105 ms

`-g` spawns 8 git subprocesses to build one row (`worktree list`, `symbolic-ref`, two
`rev-parse --verify`, `rev-parse` for the fork point, `rev-parse @{upstream}`, `rev-list
--left-right --count`, `log -1`) — that count scales with the number of lanes, not the
number of repositories. The walk behind the `DISK` column was the suspected bottleneck
going in, since it crosses build caches and costs ~0.34 s cold on this tree. Stubbing
`disk_estimate` to return `0` and re-measuring warm changed the warm `-g` timing not at all.
So the tree walk is nearly free once the OS has it cached, and the warm cost is dominated
entirely by the git subprocess spawns; the cold cost is dominated by the walk instead.

## Decision

Cache the entire row set `-g` produces — the exact `GlobalRow` shape, ready to serialize —
in a snapshot file next to the registry (`$XDG_STATE_HOME/lane/cache.json`), written
atomically the way `registry.rs` already writes its own state. A repeat `-g` within the TTL
below reads this file and skips every git subprocess it would otherwise spawn.

Freshness uses two separate mechanisms on purpose:

- **Exact invalidation for membership.** Lane performs every mutation that changes which
  lanes exist, so it deletes the cache outright on lane creation, on `-d`/`-D` deletion, on
  a `--prune` that removes anything, and on `--init`. A lane just made or removed is never
  missing from, or lingering in, `-g`'s output, independent of the TTL.
- **A 120-second TTL for the derived columns** (`AGE`, `DISK`, `COMMITS`, `STATE`), which
  drift for reasons lane does not observe: a commit made inside a lane, a build that grew a
  tree, a branch that landed somewhere else. Lane cannot invalidate exactly for what it
  never sees happen, so a bound on staleness stands in for exact tracking.

`--refresh` recomputes and rewrites the cache unconditionally, for a caller that wants
current numbers regardless of age. A cache that cannot be read, is malformed, or is outside
the TTL is treated as a miss and lane falls through to computing fresh rows — never an
error, and the same holds for a cache that cannot be written.

## Alternatives considered

**Cache only the disk estimate**, since it looked like the obvious cost center walking an
11k-file tree. Measured and rejected: stubbing it to `0` left warm `-g` timing unchanged, so
caching it alone would have added a whole caching layer — invalidation, a file format, a TTL
— for a number that was never the bottleneck. The git subprocess spawns are what a repeat
lookup actually needs to skip, which means caching has to cover the row as a whole, not one
column of it.

**No cache, just fewer subprocesses.** Reducing 8 git invocations to fewer would still
leave every `-g` paying at least one process spawn per lane per repository, which does not
converge on `lane --version`'s 6 ms baseline the way skipping the work entirely does, and
does nothing for the tree-walk cost on a cold cache.

## Consequences

- A warm `-g --json` — the path the external picker exercises on every keystroke — returns
  from a single file read and a JSON parse, with the same byte-identical output as a fresh
  computation.
- Two invalidation mechanisms are more moving parts than one, but they answer genuinely
  different questions: what lane did, versus what lane cannot observe. Collapsing them into
  a single TTL would either miss a lane lane itself just created (TTL too aggressive) or
  serve minutes-stale derived numbers for no reason (TTL too lax) — DD-006 makes the same
  kind of distinction for the fork point ref versus everything computed at read time.
- The cache file carries no information lane doesn't already print; deleting it costs
  nothing but one recomputed `-g`.
