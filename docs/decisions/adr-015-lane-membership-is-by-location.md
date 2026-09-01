# ADR-015: Lane membership is by location

- Status: accepted
- Date: 2026-09-01

## Context

`list_lanes` reads `git worktree list --porcelain` and filtered out only the primary
worktree, so every other worktree in the repository was reported as a lane. A repository
routinely holds worktrees lane did not create: `git-wt` keeps them in `.wt/`, agent tooling
in `.claude/worktrees/`, and a plain `git worktree add` puts one wherever it was asked to.

This was not a display problem. `--prune` iterates the same list and removes every lane it
judges landed and clean, and a finished `git-wt` worktree is precisely landed and clean.
Measured in one real repository, `lane --prune --dry-run` proposed deleting twelve foreign
worktrees and their branches ([FR-009](../friction/FR-009-every-worktree-looked-like-a-lane.md)).
`-g` also paid to walk all of them for its DISK column, which is how the bug was found.

The information to tell them apart was already present: lane creates every lane at
`.lane/trees/<name>`, and nothing else does.

## Decision

**A lane is a worktree under `.lane/trees/`.** `list_lanes` filters on that, and every
command reading it — `--list`, `-g`, `-i`, `-d`/`-D`, `--prune` — inherits the rule without
its own check.

Paths are compared canonically. On macOS git reports worktree paths under `/private/...`
where the repository root retains the shorter spelling, so a literal prefix comparison would
have filtered out every real lane — a failure that turns a listing bug into an empty
listing. The canonical comparison is attempted only after the cheap literal one, which
succeeds in the ordinary case.

`name_of` strips the lanes directory canonically for the same reason, and no longer falls
back to returning the absolute path. Nothing reaching it can fail that strip now that
`list_lanes` filters, and a lane name is the string that addresses a lane on the command
line — an absolute path in that position is never a usable answer.

## Alternatives considered

**Identify lanes by their fork ref** (`refs/lane/<name>`), which lane writes at creation.
Rejected: the ref records a lane's fork point, not its existence, and `create` deliberately
tolerates its absence — a lane adopted from an existing branch, or one whose ref was
collected, is still a lane. Making the ref load-bearing for membership would make a missing
ref mean "not a lane" rather than "fork point unknown", and DD-006 keeps those separate on
purpose.

**Keep listing foreign worktrees but refuse to prune them.** Rejected: it fixes only the
sharpest edge and leaves `-d` able to delete one by name, `-g` paying to walk them, and
`--list` claiming lanes that no `lane` command can properly act on. If they are not lanes
for the purpose of removal, they are not lanes.

**Match on a marker file written into each lane.** Rejected as state where a convention
suffices: the location *is* the marker, it costs nothing to check, and it cannot drift out
of sync with where `create` actually puts things.

## Consequences

- `--prune` can no longer delete another tool's worktree. This is the change that mattered;
  the e2e suite now asserts a foreign worktree survives a prune with its branch intact, and
  that assertion fails against the previous implementation.
- A worktree the user created by hand outside `.lane/trees/` is invisible to lane, including
  to `lane -d`. That is the intended reading of "lane manages lanes" — `git worktree` remains
  the tool for worktrees lane did not make — but it does mean lane cannot be used to tidy up
  after other tooling.
- Moving a lane directory out from under `.lane/trees/` stops it being a lane. Nothing
  supported does that, and `git worktree` has never promised a movable worktree beyond the
  `--relative-paths` support DD-002 already depends on.
