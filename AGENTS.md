# AGENTS

`lane <name>` gives you an isolated, copy-on-write worktree under `.lane/trees/`, warm with
the repo's ignored build caches. It creates the lane, or enters it if it already exists.
Work there instead of the main checkout when you want changes isolated from other work in
flight. `lane --exit` returns you to the main worktree.

When you are done with a lane, `lane -d <name>` removes it (`-D` if it holds unlanded work
you want to discard), or `lane --prune` sweeps every lane whose branch has already landed on
trunk.

Every bare argument is a lane name; the operation is always a flag. `lane` on its own lists.

## Working in this repo

Documentation lives in `docs/`, split three ways:

- `docs/design/` — how a subsystem works today (the COW clone layer, worktree lifecycle,
  landing detection, shell integration, the CLI surface, per-lane state). Rewrite these in
  place as the design changes.
- `docs/decisions/` — one ADR per choice: context, decision, alternatives, consequences.
  Never edit an accepted decision to match a later reality — a reversed decision gets a new
  ADR that supersedes it, and the old one is marked `Superseded by`.
- `docs/friction/` — flat, append-only log of what got in the way, its resolution, and what
  it informed. Never rewritten, only added to.

Each directory's `README.md` explains numbering and has the full index. If you hit real
friction — a confusing error, a false assumption, an environment quirk — add an `FR-` entry
rather than letting it evaporate once you've worked around it.
