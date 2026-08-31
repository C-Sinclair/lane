# FR-005: Two fish idioms that are easy to get wrong, and one false alarm

- Date: 2026-08-31

## What was hit

Writing and debugging the fish branch of the shell integration (DD-004) surfaced two real
quoting gotchas, plus one reported bug that turned out not to be a bug at all.

## Symptom

**`switch` needs its argument quoted.** `switch $argv[1]` fails to match anything when
`$argv[1]` is empty — fish's word-splitting means an unquoted empty variable disappears from
the argument list entirely rather than becoming an empty string `switch` can compare
against. `switch "$argv[1]"` is required so an empty argv still reaches a `case ''` branch
correctly.

**`case` patterns need quoting too.** An unquoted `case -h` risks fish parsing `-h` as a flag
to `case` itself rather than as a literal pattern to match, depending on what else is on the
line. Quoting the pattern (`case '-h'`) avoids the ambiguity.

**False alarm: "the fish wrapper swallows exit codes."** Reported as a bug — it wasn't. A
test had put `(basename $PWD)` in the same `echo` command as `$status`:

```fish
echo (basename $PWD) $status
```

The command substitution `(basename $PWD)` runs and completes *before* `$status` is read,
which resets `$status` to reflect that substitution's own exit code, not the exit code the
test meant to check. fish's bare `return` propagates the last command's status exactly the
way bash's does — nothing in the wrapper was actually dropping it.

## Resolution

The two quoting issues are fixed in the shipped fish integration. The false alarm's lesson:
capture `$status` into a variable immediately after the command you care about, before doing
anything else — including an apparently side-effect-free command substitution in the same
line — that could reset it.

## What it informed

DD-004 (shell integration) references this entry for the fish-specific quoting rules the
`--shellenv fish` branch has to get right.
