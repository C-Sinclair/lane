# DD-004: Shell integration

How `lane <name>` and `lane --exit` change the calling shell's directory.

- Status: current
- Date: 2026-08-31

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
through to the real binary, unwrapped — the function only intercepts the two cases that
need a `cd`:

```sh
new)  p=$(command lane "$@") && cd "$p" ;;
--exit) p=$(command lane "$@") || return; cd "$p" ;;
```

`lane` prints the destination path to stdout on success and nothing (or an error) on
failure, so the wrapper's whole job is: run it, and only `cd` if it produced a path.

## What this replaced

An earlier version of this wrapper had two independent breakages, both from the same root
cause — the shell had no reliable channel to learn the destination:

- Piping through `tail -1` to strip a duplicated bolded/bare print meant the wrapper tested
  `tail`'s exit status, not `lane`'s, so a failure `cd`'d into the error text.
- `lane done` (a landing command that has since been removed entirely — see ADR-001) tried
  to `cd` to `$(git rev-parse --show-toplevel)` *after* it had already deleted the worktree
  the shell was standing in, so `git rev-parse` failed from a deleted cwd and the `cd` was a
  silent no-op.

The current design avoids both by making the binary print exactly one path on success and
nothing on failure, and by never having any operation delete the directory the invoking
shell is currently in before that print happens.

## Fish gets its own branch, not sourced POSIX

`lane --shellenv fish` emits a fish function, not a POSIX script piped through some
compatibility shim — fish's `switch`/`case` and variable syntax are different enough that
sharing one script would mean writing to the lowest common denominator of both. See the
friction log (FR-005) for the two fish-specific gotchas this integration has to get right:
quoting `switch "$argv[1]"` so an empty argv still matches, and quoting `case` patterns so a
leading `-h` can't be parsed as a flag.
