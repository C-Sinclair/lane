# Design docs

Living documents describing the shape of a thing lane does today: how a feature or
subsystem works, and is meant to work. A design doc is **rewritten in place** as the design
changes — it describes the current mechanism, not its history. When a design changes
enough that the old description is actively wrong, edit the doc; don't leave stale
paragraphs beside new ones.

Number the next one `dd-NNN-slug.md`, one higher than the highest existing id, regardless of
which doc it's related to.

If what you're writing describes a specific choice made at a point in time — with
alternatives considered and consequences accepted — it belongs in `docs/decisions/` instead.
If it's mechanism that no longer exists, it belongs in `docs/decisions/` as a superseded
record, not here.

| ID | Title | Status | Summary |
|---|---|---|---|
| [DD-001](dd-001-cow-clone.md) | Copy-on-write clone layer | current | How a new lane gets warm build caches by reference instead of copying them. |
| [DD-002](dd-002-worktree-lifecycle.md) | Worktree lifecycle | current | How a lane is created, entered, listed, and removed. |
| [DD-003](dd-003-landing-detection.md) | Landing detection | current | How lane decides a branch has landed, from git refs alone. |
| [DD-004](dd-004-shell-integration.md) | Shell integration | current | How `lane <name>`/`lane --exit` change the calling shell's directory. |
| [DD-005](dd-005-cli-surface.md) | CLI surface | current | The full flag-only command grammar, why it has no subcommands, and the snapshot cache behind `-g`. |
| [DD-006](dd-006-per-lane-state.md) | Per-lane state | current | The one thing lane persists per lane, and where. |
