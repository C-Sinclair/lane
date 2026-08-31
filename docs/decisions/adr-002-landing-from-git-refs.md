# ADR-002: Decide landing from git refs, not a marker

- Status: accepted
- Date: 2026-08-31

## Context

Before the strip (ADR-001), landing a lane was a command — `lane done` rebased the branch,
audited and committed memory, merged, and left a marker behind. `lane ls` and `lane --prune`
could then trust that marker to know a lane had landed. That combined command is gone; there
is no `lane done`, `lane merge`, or `lane push` in the current CLI. Landing itself — merging
or pushing a branch — is now something the user does with ordinary git or their normal PR
workflow, entirely outside lane.

That leaves the question a marker used to answer: how does `lane ls` and `lane --prune` know
a given lane's branch already landed?

## Decision

Answer it by reading git refs at the moment the question is asked, never by consulting
anything lane itself wrote down. Two proofs, either sufficient: the branch's upstream was
retired (`[gone]` in `for-each-ref`), or trunk already contains the branch's commits —
checked via ancestry, a squash-merge patch-id probe, and a `git cherry` check for rebased
commits (the full mechanism is DD-003).

## Alternatives considered

**A marker written by a landing command.** This was the previous design and it required a
command that did the landing, which is exactly the `lane done` surface ADR-001 removed.
Reintroducing a marker would mean reintroducing something to write it, and that something
would need to run at exactly the right moment relative to an external merge (on GitHub, in
a different lane, via `git push` directly) that lane has no visibility into and no hook for.

**Trust `git merge-base --is-ancestor` alone.** Correct for a fast-forward or an ordinary
merge commit, wrong for a squash merge, where the merged commit's sha and parent differ
from anything on the source branch. This was plan 037's finding, reproduced against two
real lanes in this repository that neither `ls` nor `prune` could see had landed.

## Consequences

- No landing state to keep in sync, corrupt, or lose — there is nothing to write, so there
  is nothing that can go stale relative to what actually happened on the remote.
- Detection is exactly as accurate as git's own refs are. If a branch lands somewhere lane
  never observes and neither proof applies — for instance a merge strategy that rewrites
  history in a way no patch-id or ancestry check can recognize — lane will not see it as
  landed. This is judged an acceptable gap: the two proofs cover fast-forward, merge, squash,
  and rebase merges, which is the overwhelming majority of real workflows.
- Detection cost moved from "read a marker" (cheap, `O(1)`) to "run several git queries per
  lane" (more expensive, but computed in parallel across lanes at list time — see DD-002).
