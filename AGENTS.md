# AGENTS

`lane new <name>` gives you an isolated, copy-on-write worktree under `.lane/trees/`, warm
with the repo's ignored build caches. Work there instead of the main checkout when you want
changes isolated from other work in flight.

When you are done with a lane, `lane rm <name>` removes it (`--force` if it holds unlanded
work you want to discard), or `lane prune` sweeps every lane whose branch has already landed
on trunk.
