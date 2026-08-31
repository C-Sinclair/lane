# ADR-006: Anchored, immutable notes as durable memory (removed)

- Status: superseded by ADR-001
- Date: 2026-08-31 (recording history; the design itself dates from the tool's earlier
  development, before the 2026-08 strip)

## Context

Lane used to carry a durable-memory subsystem alongside its worktree mechanics: a `lane
note` command attached a short piece of text to a specific file, or a symbol within it
(an **anchor** — a tree-sitter-resolved declaration, heading, or component block, e.g.
`src/auth.rs#fn verify`), so that the reasoning behind a change could live next to the code
it was about rather than buried in history. `lane why <path>` surfaced whatever was
attached; `lane audit` re-checked every note's anchor against the current tree and flagged
drift when the anchored code changed underneath it.

The storage design went through a real correction before the whole subsystem was removed.
The original note file mixed two kinds of data with opposite merge requirements in one
file: fields written once by the author (`id`, `anchor`, `created`, `branch`, body) and
fields every audit rewrote (`sig`, `body_hash`, `status`, `checked`, `verdict`). A
`.gitattributes` `merge=union` driver was applied to keep both sides of a conflicting edit,
which is right for the append-only fields and wrong for the mutable ones — two branches that
both audited the same note produced a file with a duplicated `checked:` key, which
`serde_yaml_ng` (unlike the earlier Python implementation's looser parser) rejected outright,
making the note invisible to `lane why` and evicting it on the next audit with no path and
no anchor left to identify what was lost.

The fix (plan 013) split a note into an immutable file — written once, never rewritten,
safe under union-merge — plus a per-branch mutable state file holding everything that
changes. Two smaller, related fixes rode alongside it: a renamed file (`git mv`) used to
evict every note attached to it, since the audit only compared against the file's last
known path; and any anchor outside the tool's built-in language grammars (Swift, Ruby,
Kotlin, and others) was silently evicted on the first audit rather than treated as
unverifiable-but-kept, a real regression from the Python implementation's looser
declaration matching.

## Decision

None to record here beyond the fact that the mechanism existed and was made structurally
sound before the tool was stripped down. See ADR-001 for the decision to remove the whole
subsystem.

## What removal traded away

- Memory attached to a specific line of code, discoverable by an agent or person editing
  that file, with no separate index or database to keep in sync.
- Drift detection: a way of knowing a note's claim might no longer be true because the code
  it described changed.
- Survivability across a file rename — the fix for this landed shortly before the whole
  subsystem did.

Nothing survives from this design as live mechanism. `docs/friction/` and `AGENTS.md` are
what a project has now for recording findings — a person or agent writes them down
deliberately, in a place nothing automatically audits for staleness.
