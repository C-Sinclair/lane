# Friction log

Flat and **append-only**. Each entry records something that got in the way of building or
using lane: what was hit, the symptom (exact error text where it's short), how it was
resolved, and what it informed. Entries are never rewritten once filed — if a resolution
turns out to be wrong or incomplete, file a new entry rather than editing the old one, and
link back to it.

Number the next one `FR-NNN-slug.md`, one higher than the highest existing id.

Not every entry resolves into a design or decision doc — some are one-off environment
gotchas worth remembering on their own, with nothing else to link.

| ID | What was hit | Informed |
|---|---|---|
| [FR-001](FR-001-commit-signing-blocks-non-interactive-tools.md) | Commit signing blocks non-interactive tool use | Test-repo setup requirement (disable signing explicitly) |
| [FR-002](FR-002-config-value-is-not-a-reference.md) | A config value naming a commit is not a reference | [ADR-003](../decisions/adr-003-fork-point-as-a-ref.md), [DD-006](../design/dd-006-per-lane-state.md) |
| [FR-003](FR-003-per-lane-config-keys-never-deleted.md) | Deleting per-lane config keys was never wired up | [ADR-003](../decisions/adr-003-fork-point-as-a-ref.md) |
| [FR-004](FR-004-zsh-completions-cannot-be-sourced.md) | zsh completion scripts cannot be sourced | [DD-005](../design/dd-005-cli-surface.md) |
| [FR-005](FR-005-fish-idioms-and-a-false-alarm.md) | Two fish idioms that are easy to get wrong, and one false alarm | [DD-004](../design/dd-004-shell-integration.md) |
| [FR-006](FR-006-git-rejects-dash-prefixed-branch-names.md) | Git refuses branch names beginning with `-` | [DD-002](../design/dd-002-worktree-lifecycle.md) (open gap) |
| [FR-007](FR-007-the-tree-walk-was-not-the-bottleneck.md) | The tree walk looked like `-g`'s bottleneck, and wasn't | [ADR-013](../decisions/adr-013-cache-the-whole-global-row-set.md) |
| [FR-008](FR-008-ignored-entries-collapse-above-the-lanes-directory.md) | Ignored entries collapse above the lanes directory; lanes cloned each other | [DD-001](../design/dd-001-cow-clone.md), [DD-002](../design/dd-002-worktree-lifecycle.md) |
| [FR-009](FR-009-every-worktree-looked-like-a-lane.md) | Every worktree looked like a lane; `--prune` would have deleted them | [ADR-015](../decisions/adr-015-lane-membership-is-by-location.md) |
| [FR-010](FR-010-making-a-json-field-optional-broke-a-consumer.md) | Making a `--json` field optional broke a consumer instantly and silently | Rollout practice; no design change |
| [FR-011](FR-011-a-branch-checked-out-elsewhere-failed-the-lane.md) | A branch checked out in another worktree failed `lane <name>` | [DD-005](../design/dd-005-cli-surface.md) |
