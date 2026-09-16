# ADR-017: Deleting the lane you are standing in

- Status: accepted
- Date: 2026-09-08

## Context

`worktree::remove` refused outright when the caller's working directory was inside the lane
being removed:

```
error: cannot remove lane demo from inside it; cd out first
```

The refusal protected a real failure. Removing a directory leaves every process sitting in
it on a path that no longer resolves, and the shell that invoked `lane` is one of those
processes. `merge` avoided it by chdiring to the main root before calling `remove`; `-d` and
`--prune` had no such step, so they refused instead.

The refusal is also the most common way to meet `-d`. Finishing work in a lane and deleting
it is one motion, and it fails from exactly where the work happened. Every external picker
hits it too: an fzf popup that offers to delete the highlighted lane cannot know whether the
shell behind it is standing in that lane.

The shell integration already knows how to move a shell. `lane <name>` and `lane -e` both
print a destination on stdout, and the function `--shellenv` installs cds into whatever it
captures. Nothing about `-d` was incompatible with that; the destination was simply never
offered.

## Decision

**`remove` chdirs to the main root instead of refusing, and reports that it did.**

- `worktree::remove` returns `Result<bool>` — whether the caller was inside the lane. When
  it was, the process is moved to the main root before the worktree goes.
- `-d`/`-D` print their per-lane report on stderr, freeing stdout to carry a destination.
  When one of the removals moved the process out, the main root is printed on stdout;
  otherwise stdout is empty.
- The `--shellenv` function gains a `-d`/`-D` case that cds only when it captured a
  non-empty path, and returns lane's own exit code either way.
- `--prune` keeps its stdout report, so it has nowhere to put a destination. It writes
  `note: the lane you were standing in is gone; run \`lane -e\`` to stderr instead.

## Alternatives

**Keep refusing, and have callers chdir first.** This is what `merge` does, and it works for
a caller that owns its own process. It does nothing for the shell, which is the process that
actually ends up stranded, and an external picker cannot chdir the shell that spawned it.

**Print the destination for `--prune` as well.** `--prune`'s stdout is its report — one
`removed <name>` line per lane, and `would remove <name>` under `--dry-run`. Moving that to
stderr to make room for a path would change what every existing consumer reads.

## Consequences

- `lane -d <name>` now succeeds from inside `<name>`, and with the shell integration
  installed it leaves the shell in the main root.
- `lane -d` writes `removed lane <name>` to stderr rather than stdout. A script that parsed
  that line from stdout reads an empty stream; one that checks the exit code is unaffected.
- Every caller of `worktree::remove` handles a `bool` return.
