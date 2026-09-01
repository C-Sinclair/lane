# Decision records

Each ADR captures one choice: the context that forced it, the decision, the alternatives
considered, and its consequences. Decisions are **historical and accrete** — a decision
that gets reversed is never edited away. A new ADR supersedes it, records why, and the old
one gains a `Status: superseded by ADR-NNN` line. Never rewrite an accepted ADR's decision
or context to match a later reality.

Number the next one `adr-NNN-slug.md`, one higher than the highest existing id.

Some of these record a subsystem that existed and was later removed rather than a design
choice still in effect — marked `superseded by ADR-001` below. They're kept because the
mechanism and the reasoning behind it are the most useful record of what this tool used to
try, and why it stopped.

| ID | Title | Status | Summary |
|---|---|---|---|
| [ADR-001](adr-001-strip-to-cow-worktrees-only.md) | Strip to copy-on-write worktrees only | accepted | Delete the notes/memory subsystem; reduce lane to worktree mechanics. |
| [ADR-002](adr-002-landing-from-git-refs.md) | Decide landing from git refs | accepted | No landing marker; `ls`/`prune` read git refs at the moment of the question. |
| [ADR-003](adr-003-fork-point-as-a-ref.md) | Fork point as a ref | accepted | `refs/lane/<name>`, not a git-config value. |
| [ADR-004](adr-004-flag-based-cli.md) | Flag-based CLI | accepted | Every bare word is a lane name; operations are flags, not subcommands. |
| [ADR-005](adr-005-no-collaborative-object-storage.md) | No collaborative-object storage | accepted | Rejected a Radicle-style shared-object model; a ref is already minimal for a single-writer state. |
| [ADR-006](adr-006-anchored-immutable-notes.md) | Anchored, immutable notes | superseded by ADR-001 | Durable per-file memory, corrected to an immutable-note + per-writer-state split, then removed. |
| [ADR-007](adr-007-structured-reads-and-note-lifecycle.md) | Structured reads and note lifecycle | superseded by ADR-001 | JSON reads, anchor discovery, and one command family for the note lifecycle. |
| [ADR-008](adr-008-derived-audit-state-out-of-git.md) | Derived audit state out of git | superseded by ADR-001 | A sequence of fixes chasing committed, multi-writer derived state; ended with per-note baselines and no shared log. |
| [ADR-009](adr-009-capture-decisions-from-commit-trailers.md) | Capture decisions from commit trailers | superseded by ADR-001 | A `Why:` trailer as the low-friction path to a note; the pending queue that carried it, twice relocated, then removed. |
| [ADR-010](adr-010-llm-review-of-drifted-notes.md) | LLM review of drifted notes | superseded by ADR-001 | Optional model-assisted triage of stale notes, corrected to mark authorship and expose verdicts as ordinary verbs. |
| [ADR-011](adr-011-auto-install-skill-and-hooks.md) | Auto-install skill and hooks | superseded by ADR-001 | `lane install skill`/hooks, and the replaceable-marker fix for repairing previously-installed text. |
| [ADR-012](adr-012-interactive-tour-as-separate-binary.md) | Interactive tour as a separate binary | superseded by ADR-001 | A teaching aid shipped as its own crate, with a hard boundary against affecting the real binary. |
| [ADR-013](adr-013-cache-the-whole-global-row-set.md) | Cache the whole `-g` row set, not just the disk estimate | accepted | Measured the tree walk as nearly free warm; git subprocess spawns dominate, so the cache covers the whole row. |
| [ADR-014](adr-014-a-lane-name-adopts-a-matching-upstream-branch.md) | A lane name adopts a matching upstream branch | accepted | A name matching exactly one remote-tracking branch branches from it and tracks it; two matches is an error. |
| [ADR-015](adr-015-lane-membership-is-by-location.md) | Lane membership is by location | accepted | A lane is a worktree under `.lane/trees/`; foreign worktrees are not lanes and `--prune` cannot touch them. |
| [ADR-016](adr-016-the-disk-estimate-is-opt-in.md) | The DISK estimate is opt-in | accepted | The tree walk is ~all of a cold `-g`; `-g` skips it and `--disk` opts in, absent rather than zero in JSON. |
