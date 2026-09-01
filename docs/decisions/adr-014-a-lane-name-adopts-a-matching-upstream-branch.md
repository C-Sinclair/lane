# ADR-014: A lane name adopts a matching upstream branch

- Status: accepted
- Date: 2026-09-01

## Context

Every bare word on lane's command line is a lane name, and a lane name is a branch name
([ADR-004](adr-004-flag-based-cli.md)). `create` already adopts an existing **local**
branch rather than recreating it, so `lane review-963` on a fetched pull request opens a
lane on that work.

A branch that exists only as a remote-tracking ref got no such treatment. `lane
ARC-245-frontend`, with `origin/ARC-245-frontend` fetched but no local branch, created a
*new* branch off local HEAD. The lane looked right — correct name, correct directory — and
silently contained none of the published commits. The failure is quiet in the worst way: the
next push either fails as non-fast-forward long after the work has diverged, or succeeds
against a different branch than the user thought they were on. `git worktree add <path>
<branch>` and the `git-wt` wrapper both resolve a bare name through remote-tracking refs
for exactly this reason.

## Decision

When a lane name matches no local branch and no `--base` was given, look for
remote-tracking branches of that name across every configured remote:

- **Exactly one match** — branch from it and set it as upstream, by passing `--track` to
  `git worktree add` alongside the existing `-b <name> <dest> <remote>/<name>`. Report it
  in the creation notes (`branched from origin/<name> and tracking it`) so the choice is
  never invisible.
- **No match** — unchanged behaviour: an ordinary new branch from the current HEAD.
- **More than one match** — fail, listing the remotes, and point at `--base`. Two remotes
  publishing the same branch name is precisely the case where guessing is unrecoverable,
  and lane has a way to say which one.

An explicit `--base` always wins and suppresses the lookup: it is the existing way to say
"start this from somewhere else", and a name colliding with an upstream branch must not
take that away.

**Only refs already fetched are consulted.** Creating a lane does not reach the network.

## Alternatives considered

**Fetch before resolving**, so a name matching an unfetched upstream branch is also
adopted. Rejected: it puts a network round-trip on the hot path of every lane creation,
including the overwhelmingly common case of a brand-new name where the fetch can only ever
come back empty. Latency here is felt directly — lane creation is otherwise ~2 s on a large
repository (see [FR-008](../friction/FR-008-ignored-entries-collapse-above-the-lanes-directory.md))
— and it makes creation fail when offline. The user who wants an upstream branch adopted
fetches, exactly as they would before `git checkout` of a colleague's branch.

**Prompt when a name matches an upstream branch.** Rejected: lane is used
non-interactively, from shell wrappers and from agents, and a prompt on the creation path
has no good non-interactive answer. Adopting is what the user meant in nearly every case,
and the note in the output plus `lane -d` makes the wrong guess cheap to undo.

**Adopt but do not set upstream.** Rejected as half the feature: the commits would be
there, but `lane -i` and the `pushed`/`open` state in `-g` both read `@{upstream}`, and
`git push` with no upstream is its own papercut. Tracking is what makes the adopted lane
behave like the branch it claims to be.

## Consequences

- `lane <name>` is now three operations behind one word: enter an existing lane, adopt a
  local or upstream branch, or create a new one. That is more behaviour per keystroke than
  before, which is why the upstream case announces itself in the notes rather than being
  inferred from output the user has to compare against expectation.
- A lane name that collides with a stale remote-tracking ref — a branch long since merged
  and deleted on the remote, whose ref survives locally until `fetch --prune` — will be
  adopted rather than created fresh. This is the same trap `git checkout <name>` has, and
  the note names the ref it used, so it is visible rather than merely correct.
- Multiple remotes publishing one name is now a hard error where it used to silently work
  (by ignoring both). That is a deliberate regression in permissiveness: the old success
  was the wrong branch.
