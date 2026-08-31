# DD-001: Copy-on-write clone layer

How a new lane gets its warm build caches without byte-copying them.

- Status: current
- Date: 2026-08-31

## Summary

A new lane is a git worktree plus every file git ignores in the source tree, cloned by
reference rather than copied. On a filesystem that supports it, cloning costs no extra
storage until either copy's blocks diverge. Where reflinks are unavailable, lane creates a
plain worktree and skips the ignored files rather than byte-copying them.

## The warm set comes from git

The set of files to bring into a new lane is `git status --porcelain -z --ignored`, not a
hardcoded list of directory names. A hardcoded top-level list (`node_modules`, `target`, …)
cannot express a monorepo layout where ignored directories nest — `packages/a/node_modules`
sits three levels down and `.env` can live anywhere. Asking git is exact and free of
maintenance: whatever the repository's own `.gitignore` says is warm, is warm.

`--dirty` extends the same clone to tracked, uncommitted edits and untracked-but-not-ignored
files, so a lane can carry in-progress work as well as caches.

## Two clone mechanisms, one interface

`crates/lane/src/cow.rs` wraps the two reflink syscalls behind one `clone_file`/`clone_dir`
pair:

- macOS: `clonefile(2)` on APFS. Cloning a directory clones the whole tree beneath it in one
  call.
- Linux: `FICLONE` ioctl, on btrfs, XFS with `reflink=1`, bcachefs, and some ZFS
  configurations. No directory primitive — the tree is walked and each file cloned
  individually.

`probe()` gates the whole mechanism: it attempts one real clone against the destination
filesystem and returns whether it succeeded. This is a hard gate, not a hint — if the probe
fails, lane creates a normal git worktree and does not fall back to copying ignored files.
Byte-copying `node_modules` and `target` is the exact expense the tool exists to avoid, so a
filesystem that cannot reflink gets a lane with none of that content rather than a slow one
with all of it.

`clone_file` classifies the syscall's error to tell "this filesystem cannot reflink"
(`ENOTSUP`/`EOPNOTSUPP`, handled as equivalent — Darwin's `clonefile` returns `ENOTSUP`, not
the `EOPNOTSUPP` a naive port would expect) from a real I/O failure, so probing never masks
a genuine problem as a missing capability.

## Symlinks: preserved, except back into the source

A clone preserves symlinks byte for byte using `CLONE_NOFOLLOW` (macOS) / a
`symlink(read_link(src))` fallback walk. That is correct for a relative link, or an absolute
link pointing outside the repository. It is wrong for an absolute link that points back
*inside* the source worktree: the lane would hold a link into the parent's checkout, silently
reading and executing the parent's bytes while believing it owns them. That breaks the
property lane exists to provide — that two lanes cannot see each other's in-progress work.

`retarget()` (`cow.rs`) checks every symlink's target against the known "spellings" of the
source root (the worktree path plus any path it's reachable through, e.g. a symlinked
`/tmp`) and rewrites an absolute in-root target to point at the equivalent path inside the
destination instead of copying it verbatim. A relative link, or an absolute link outside the
source root, is left untouched.

## What's deliberately out of scope

Ignored files are the only warm-cache carrier. There is no configurable include/exclude
list — the earlier attempt at one (a hardcoded, user-editable set of top-level names) could
not express the nested-`node_modules` case that matters most and was abandoned; see
ADR-003 (fork point as a ref) for the general preference this reflects: derive state from
git rather than duplicate it in lane's own config.
