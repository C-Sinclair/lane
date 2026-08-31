# FR-006: Git refuses branch names beginning with `-`

- Date: 2026-08-31

## What was hit

`lane -- -dash-name` — using `--` to tell lane's own parser that `-dash-name` is a lane
name, not a flag — parses correctly on lane's side, but the underlying `git worktree add`
call still fails.

## Symptom

```
$ lane -- -dash-name
```

`git worktree add -b -dash-name ...` fails and dumps git's own usage text at the user,
because git refuses a branch name beginning with `-` (it would be ambiguous with a flag to
whichever git subcommand is asked to operate on it). Lane's parser did its job correctly;
the name itself is illegal as far as git is concerned, and the resulting error is git's, not
lane's, and reads confusingly out of context.

## Resolution

Not fixed. Recorded as a known rough edge: the fix would be a `git check-ref-format`
preflight before attempting to create the worktree, so lane can reject the name itself with
a clear message instead of forwarding git's raw usage dump.

## What it informed

No design or decision doc yet — this is an open, acknowledged gap in DD-002 (worktree
lifecycle)'s creation path rather than a resolved one. Recorded so the known fix
(`check-ref-format` preflight) isn't rediscovered from scratch if someone picks it up.
