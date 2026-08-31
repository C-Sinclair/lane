# AGENTS

`lane <name>` gives you an isolated, copy-on-write worktree under `.lane/trees/`, warm with
the repo's ignored build caches. It creates the lane, or enters it if it already exists.
Work there instead of the main checkout when you want changes isolated from other work in
flight. `lane --exit` returns you to the main worktree.

When you are done with a lane, `lane -d <name>` removes it (`-D` if it holds unlanded work
you want to discard), or `lane --prune` sweeps every lane whose branch has already landed on
trunk.

Every bare argument is a lane name; the operation is always a flag. `lane` on its own lists.
