# DD-004: Shell integration

How `lane <name>`, `lane --exit` and `lane -d` change the calling shell's directory.

- Status: current
- Date: 2026-09-08

## Summary

The `lane` binary is a subprocess: it can change its own working directory, never its
parent shell's. Entering or exiting a lane therefore needs a thin shell function that runs
`lane`, captures the path it prints, and `cd`s to it itself. `lane --shellenv [shell]`
prints that function; installing it (`eval "$(lane --shellenv)"` for POSIX shells, or
`lane --shellenv fish | source` for fish) is what makes `lane <name>` and `lane --exit`
actually move you. Without it, both commands still print the destination — you just `cd`
yourself.

## The wrapper's job is narrow

Every other flag and bare `lane` with no shell-affecting operation is passed straight
through to the real binary, unwrapped — the function only intercepts the three cases that
can need a `cd`:

```sh
new)    p=$(command lane "$@") && cd "$p" ;;
--exit) p=$(command lane "$@") || return; cd "$p" ;;
-d|-D)  p=$(command lane "$@"); code=$?; [ -n "$p" ] && cd "$p"; return $code ;;
```

`lane` prints the destination path to stdout on success and nothing (or an error) on
failure, so the wrapper's whole job is: run it, and only `cd` if it produced a path.

`-d` is the one case where an empty stdout is a success. Deleting a lane from anywhere else
in the repository leaves the shell exactly where it belongs, so `-d` prints a path only when
the removal took the directory the shell was standing in, and its `removed lane <name>`
report goes to stderr to keep stdout free for that path. This is why its branch keeps
`lane`'s exit code rather than returning early: a lane kept back for unlanded work exits 1
with nothing on stdout, and the caller has to see that 1. See
[ADR-017](../decisions/adr-017-deleting-the-lane-you-are-standing-in.md).

## What this replaced

An earlier version of this wrapper had two independent breakages, both from the same root
cause — the shell had no reliable channel to learn the destination:

- Piping through `tail -1` to strip a duplicated bolded/bare print meant the wrapper tested
  `tail`'s exit status, not `lane`'s, so a failure `cd`'d into the error text.
- `lane done` (a landing command that has since been removed entirely — see ADR-001) tried
  to `cd` to `$(git rev-parse --show-toplevel)` *after* it had already deleted the worktree
  the shell was standing in, so `git rev-parse` failed from a deleted cwd and the `cd` was a
  silent no-op.

The current design avoids both by making the binary print at most one path on success and
nothing on failure. `-d` does now delete the directory the invoking shell is standing in,
which is why `worktree::remove` chdirs the binary to the main root first: the path is
computed and printed from a directory that still exists, and the shell follows afterwards.

## Fish gets its own branch, not sourced POSIX

`lane --shellenv fish` emits a fish function, not a POSIX script piped through some
compatibility shim — fish's `switch`/`case` and variable syntax are different enough that
sharing one script would mean writing to the lowest common denominator of both. See the
friction log (FR-005) for the two fish-specific gotchas this integration has to get right:
quoting `switch "$argv[1]"` so an empty argv still matches, and quoting `case` patterns so a
leading `-h` can't be parsed as a flag.
