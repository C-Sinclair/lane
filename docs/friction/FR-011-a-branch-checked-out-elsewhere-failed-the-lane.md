# FR-011: A branch checked out in another worktree failed `lane <name>`

- Date: 2026-09-16

## What was hit

`lane <name>` on a branch that some other working tree of the repository already has
checked out failed with git's own error and left the caller where they stood. The branch
was most often checked out in the main worktree, or in a worktree another tool created
(`.wt/`, `.claude/worktrees/`). Reaching the branch meant reading the error, finding the
path in it, and `cd`-ing there by hand.

## Symptom

```
$ lane sidebranch
fatal: 'sidebranch' is already used by worktree at '/Users/conor/Repos/example/side-wt'
```

git allows a branch in one working tree at a time, so `git worktree add` on a checked-out
branch can only fail. `worktree::create` adopts an existing local branch, and that adoption
hits the rule directly.

## Resolution

`worktree::checkout_holding` reads `git worktree list --porcelain` and returns the checkout
holding a branch. `cli::open` consults it after the `.lane/trees/<name>` lookup and before
creating anything: a hit prints a note naming that checkout and prints its path on stdout,
which is what the shell function reads to move the caller there. The error path is gone,
not reworded.

`--base` and `--dirty` are rejected here the same way they are when the lane already
exists, since nothing is being created either way.

## What it informed

[DD-005](../design/dd-005-cli-surface.md), which now states the three destinations
`lane <name>` resolves to and their order.
