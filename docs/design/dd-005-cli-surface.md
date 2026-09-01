# DD-005: CLI surface

The complete command grammar, and why every bare word is a lane name.

- Status: current
- Date: 2026-08-31

## Summary

```
lane                                        list lanes
lane <name> [-b|--base <rev>] [--dirty]     enter a lane, or create it
lane -d|-D <name>...                        delete
lane -g|--global [--refresh] [--disk]       list lanes across every repository
lane --prune [--dry-run]
lane --init
lane --exit
lane --shellenv [shell]
lane --completions <shell>
```

There is no subcommand grammar. Every operation other than "enter or create a lane" is
spelled as a flag. A bare positional word is always a lane name — never a command, never
ambiguous with one.

## Why flags, not subcommands

An earlier version of lane had both: subcommands (`lane new`, `lane ls`, `lane rm`,
`lane prune`, …) alongside the everyday form of typing a bare lane name to enter or create
it. That combination has two failure modes, and eliminating the subcommand grammar removes
both at once rather than patching either:

- **A command outranks a same-named lane.** If a lane is named `ls` or `prune`, the
  subcommand parser has to pick a winner, and it isn't the lane. This is a straightforward
  argument-precedence bug for any tool that mixes free-form user-chosen names with reserved
  command words in the same position.
- **A near-miss typo needs a guard.** `lane sl` meaning `lane ls` is exactly the kind of
  error a subcommand grammar has to detect and suggest a correction for, which is machinery
  a flag-only design has no need for: `lane sl` unambiguously means "enter or create a lane
  named `sl`".

Choosing the operation by flag instead makes every bare word a lane name, full stop — a
lane may be named anything, including words that used to be commands, with no precedence
rule and no typo guard required.

## Parsing and completion

`crates/lane/src/args.rs` owns the complete parser surface as one `Parsed` enum; `help.rs`
owns the built-in help text and the strings copied into parse errors, so the two can't drift
from each other independently. `--completions fish|bash|zsh` prints a completion script that
completes lane names as bare arguments and after `-d`/`-D` — see FR-004 for why the zsh
script in particular can't be `source`d directly and must land on `$fpath` as `_lane`.

## `-g`'s cache

`-g` is answered from `cache.rs`'s snapshot (`$XDG_STATE_HOME/lane/cache.json`) whenever one
is on hand and fresh, rather than recomputed every time: ADR-013 has the measurements behind
that choice (a warm `-g` is dominated by git subprocess spawns, not the disk-estimate walk),
and `cache.rs`'s module doc has the two-mechanism freshness design — exact invalidation for
lane membership, a 120-second TTL for the derived columns. `--refresh` (valid only alongside
`-g`, enforced the same way `--dry-run` is pinned to `--prune`) bypasses the TTL and rewrites
the cache. `-g --json` is byte-identical whether the rows came from the cache or were just
computed — the cache stores the exact row shape lane prints, not a coarser summary of it.

The cost of a cache *miss* still matters, because the TTL guarantees one every 120 seconds
and a picker opened after that pays it in full. Three things dominated it:

1. Foreign worktrees counted as lanes — see DD-002's membership rule
   ([FR-009](../friction/FR-009-every-worktree-looked-like-a-lane.md)).
2. `disk_estimate` walking nested checkouts inside a lane. It now prunes any directory
   holding a `.git` entry: another checkout's storage is never the lane's, and one stat per
   directory buys skipping the whole tree beneath it.
3. The disk walk itself, which after (1) and (2) was essentially all that was left. It is
   now **opt-in**: plain `-g` does not measure disk, `-g --disk` does
   ([ADR-016](../decisions/adr-016-the-disk-estimate-is-opt-in.md)). The `DISK` column
   appears only under the flag, and `disk_estimate_bytes` is absent from `--json` rather
   than zero when it was not measured. A row set cached without it is a miss for `--disk`;
   one cached with it is reused without, the field cleared — so output follows the flags,
   never the cache's contents.

A cold `-g` went from 22.7 s to **~0.09 s** (0.5 s with `--disk`); warm is unchanged at 2 ms.
`lane -i <name>` still measures disk unconditionally: one lane, and the reader asked.
