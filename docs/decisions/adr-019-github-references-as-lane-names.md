# ADR-019: GitHub references as lane names

- Status: accepted
- Date: 2026-09-26

## Context

Work on a pull request or an issue starts from its number or its URL, not its branch name. Reaching the branch meant asking `gh` for it first, `lane "$(gh pr view 103 --json headRefName -q .headRefName)"`, or copying it from the web page. [ADR-018](adr-018-names-read-from-stdin.md) shortened that to a pipeline, but the reader still has to know the `gh` incantation.

A lane name is a branch name ([ADR-004](adr-004-flag-based-cli.md)), and `lane <name>` adopts a matching upstream branch ([ADR-014](adr-014-a-lane-name-adopts-a-matching-upstream-branch.md)). ADR-014 only consults refs already fetched, so a pull request branch nobody fetched yet gets a fresh branch off HEAD under the right name. That is the silent failure ADR-014 set out to prevent.

## Decision

`github::parse_reference` recognises three forms of name. `cli::branch_for` resolves each through `gh` before the name reaches open, `-d`/`-D` or `-i`, whether it came from the command line or from stdin:

- `#103` is pull request 103 in this repository, resolved to its `headRefName`.
- `https://github.com/<owner>/<repo>/pull/<n>` is that pull request, resolved the same way.
- `https://github.com/<owner>/<repo>/issues/<n>` is the first branch `gh issue develop --list` reports as linked to the issue. With none linked, it is the name GitHub would give one, `<n>-<title slug>`, derived locally by `github::issue_branch_name`. Nothing is created on GitHub.

Every lookup passes `-R <owner>/<repo>` taken from origin's configured URL, so `#103` and a URL mean the same repository. A URL naming any other repository is an error that names both. A pull request from a fork is an error too, because its branch is not on origin.

A pull request, or an issue's linked branch, is published on origin. `cli::open` fetches that one branch into `refs/remotes/origin/<branch>` before creating a lane, unless a local branch of that name exists or `--base` was given. ADR-014's adoption then finds the remote-tracking ref and the lane tracks it.

Every `gh` call goes through `github::gh`. A missing `gh` reports `resolving #103 needs the GitHub CLI (gh), which is not on PATH`, and a failed lookup reports `gh could not resolve #103:` followed by `gh`'s own error.

## Alternatives considered

**Leave fetching to ADR-014's rule, which never reaches the network.** Rejected for references only. Resolving a reference already calls GitHub, so one more fetch changes neither the offline story nor the latency class. Without it the most common case, a colleague's pull request, produces a lane that silently lacks their commits.

**Fetch a fork's pull request through `refs/pull/<n>/head`.** Rejected for now: the lane would have no upstream to push to, and `gh pr checkout` already handles forks, including adding the fork as a remote.

**Create the issue's branch with `gh issue develop`.** Rejected: opening a lane should not write to GitHub. The locally derived name matches what `gh issue develop` would create, so creating the branch later from the lane links it.

**Parse references in `args::parse`.** Rejected: resolving needs the repository and a subprocess, and the parser is pure. `args` still treats a reference as a plain name, and `cli` resolves it after parsing.

## Consequences

- `#103` has to be quoted in bash, zsh and fish, as `lane '#103'`, because an unquoted `#` at the start of a word starts a comment. The shell then runs a bare `lane`, which lists. The URL forms need no quoting.
- A plain branch literally named `#103`, or named like a GitHub URL, can no longer be opened by that name. Both are unusual, and `git` would need them quoted anyway.
- Only `github.com` is recognised. A GitHub Enterprise host is a plain name.
- `github::issue_branch_name` keeps ASCII letters and digits only. A title in another script loses those characters, where GitHub's own naming may keep them.
