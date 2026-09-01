# ADR-016: The DISK estimate is opt-in

- Status: accepted
- Date: 2026-09-01

## Context

[ADR-013](adr-013-cache-the-whole-global-row-set.md) cached `-g`'s row set to spare an
external picker the git subprocesses behind it. That works — a warm `-g --json` returns in
2 ms — but the 120-second TTL guarantees a miss roughly every couple of minutes, and the
picker's user feels the recomputation in full whenever they open it after a pause. A cache
bounds staleness; it does nothing for the cost of a miss.

Two bugs inflated that miss to 22.7 s and have been fixed
([FR-008](../friction/FR-008-ignored-entries-collapse-above-the-lanes-directory.md),
[FR-009](../friction/FR-009-every-worktree-looked-like-a-lane.md)). With both gone, the
remaining cost is almost entirely the DISK column's tree walk. Measured on a release binary
over three registered repositories:

| | cold | warm |
|---|---|---|
| `-g --refresh` | 1.22 s | 0.30 s |
| same, `disk_estimate` stubbed to `0` | 0.09 s | 0.09 s |

This is the opposite of what the same experiment showed in
[FR-007](../friction/FR-007-the-tree-walk-was-not-the-bottleneck.md), where stubbing the walk
changed nothing. Both measurements are correct: FR-007 measured one lane in a small fixture
where 19 git subprocess spawns dominate, and a real Elixir monorepo lane is 43,812 files
where the walk dominates instead. The walk needs two stats per file — one in the lane, one at
the matching path in the main checkout — and there is no cheaper way to ask what a reflinked
file has stopped sharing.

Without the walk, `-g` is ~90 ms whatever the cache holds, which is the floor set by process
startup and ~19 git invocations.

## Decision

**`-g` does not compute the disk estimate. `-g --disk` opts in.**

- The text table grows a `DISK` column only under `--disk`; a header with nothing beneath it
  is worse than no header.
- In `--json`, `disk_estimate_bytes` is **absent** rather than zero when it was not measured,
  so a reader cannot mistake "not measured" for "measured, nothing unshared". The field is
  `Option<u64>` with `skip_serializing_if`.
- `--disk` is pinned to `-g`, rejected elsewhere as a usage error, for the same reason
  `--refresh` is: it names what that one listing computes and means nothing anywhere else.
- `lane -i <name>` keeps the estimate unconditionally. It covers one named lane, the walk is
  a fraction of the cost, and a reader who asked about that lane specifically wants it.

Cache interaction is settled in both directions. A cached row set computed without the walk
cannot answer `--disk`, so that combination is a miss and recomputes rather than printing a
column of blanks. A cached set that *does* carry estimates is still usable without `--disk`;
the field is cleared before output. Output therefore depends only on the flags given, never
on what a cache happened to hold — the byte-identical guarantee ADR-013 made.

## Alternatives considered

**A separate, longer-lived cache for the disk numbers** (say an hour, against the row set's
120 s), so a routine refresh never re-walks. Rejected as the primary answer: it keeps the
column at the price of a second freshness mechanism with its own TTL, and the first call
after expiry still costs a second or more — the exact experience being fixed. It also caches
the *least* trustworthy number lane prints for the longest, which is backwards. Nothing here
forecloses adding it later under `--disk`.

**A time budget per lane, printing `—` on overrun.** Rejected: it makes the column
nondeterministic. The same lane would show a number or a dash depending on the page cache,
which is indistinguishable from a bug to anyone reading the output, and an external consumer
of `--json` cannot tell a slow walk from a lane with nothing unshared.

**Keep computing it and accept ~1.2 s.** Rejected: the column exists to answer "which lane
is eating my disk", which is an occasional question, and it was being paid for on every
invocation of a menu that never displayed it.

## Consequences

- `-g` is ~90 ms cold and ~2 ms warm, and the TTL's expiry is no longer something the user
  feels. The cache is now an optimization rather than the only thing standing between a
  picker and a multi-second stall.
- The default `-g` no longer answers "which lane is eating my disk". `--disk` does, and pays
  for it — a second or so across a few large repositories.
- `--json`'s shape is now conditional on a flag. That is a real cost for a consumer that
  assumed the field: it must handle absence. Absence is the honest encoding, and a `0` there
  would have been a silent wrong answer rather than a visible missing one.
