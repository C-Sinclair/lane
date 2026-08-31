# lane

Copy-on-write git worktrees. This glossary fixes the words the tool uses for the things it
moves: worktrees, and the refs they came from.

## Language

**Lane**:
A copy-on-write worktree together with the branch checked out inside it.
_Avoid_: workspace, branch — a branch is not a lane.

**Base**:
The ref a lane branched from. Chosen once when the lane is created.
_Avoid_: trunk, parent, target, upstream — upstream is the remote-tracking ref, a different thing.

**Trunk**:
The repository's default branch. Used to decide whether a lane's branch has landed.
_Avoid_: main, master, mainline — the default branch is frequently none of these.

**Fork point**:
The commit a lane branched from, recorded at `refs/lane/<name>` when the lane is created.
Distinguishes a lane that has committed nothing from one whose work has landed.
_Avoid_: base — the base is a ref that moves, the fork point is a fixed commit.

**Landed**:
A lane whose branch its remote retired, or whose commits trunk already contains — checked
against git refs alone, nothing lane itself records. Collectable by `lane --prune`.
