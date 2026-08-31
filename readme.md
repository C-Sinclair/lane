# lane [![CI](https://github.com/lukeed/lane/actions/workflows/ci.yml/badge.svg)](https://github.com/lukeed/lane/actions/workflows/ci.yml)

> Copy-on-write Git worktrees.

Lane creates isolated worktrees without leaving behind the ignored build caches that make a
checkout fast. When you run `git worktree add`, it creates a clean checkout but leaves
behind everything Git ignores: `target/`, `node_modules/`, virtual environments, generated
files, local `.env` files. That means reinstalling and rebuilding from scratch, despite
already having warm caches on disk.

On a reflink filesystem, lane clones those entries by reference instead. The new lane starts
warm, and new storage is only allocated for the blocks it changes.

Lane uses `clonefile(2)` on APFS and `FICLONE` on Linux filesystems that support it,
including btrfs and XFS with reflink enabled. If reflinks are unavailable, lane creates a
normal worktree and does not byte-copy ignored caches. When desired, `lane new --dirty` also
carries tracked edits and untracked, non-ignored files; without reflinks, those changed files
are copied normally.

Lanes live under `.lane/trees/` and are excluded through `.git/info/exclude`, so the
worktrees themselves are never committed.

The only state lane keeps is the commit each lane forked from, held as a ref at
`refs/lane/<name>` and deleted with the lane. It is what separates a lane that has not
committed yet from one whose work has fully merged — both leave the branch tip at its
merge-base with trunk — and keeping it as a ref rather than a config value keeps that
commit reachable, so a rewritten base cannot let gc collect it. Nothing is written to
`.git/config`, and the refs are local: they are never pushed or fetched.

## Install

```sh
$ cargo binstall --git https://github.com/lukeed/lane lane
# or
$ cargo install --git https://github.com/lukeed/lane
```

Lane requires Rust 1.85 or newer.

## Setup

For each new shell, install the `lane shellenv` wrapper so `lane new`, `lane enter`/`switch`,
and `lane exit` `cd` for you. Add it to `.zshrc` or `.bashrc`:

```sh
eval "$(lane shellenv)"
```

> Without it, those commands still print the destination path; you just have to `cd` there
> yourself.

Then initialize each repository:

```sh
$ cd yourrepo
$ lane init
```

This creates `.lane/` and reports whether the filesystem supports reflinks.

## Usage

```sh
$ lane new fix-login
$ lane enter fix-login

# edit and commit as usual

$ lane exit
$ lane prune
```

`lane new <name>` creates a branch and worktree under `.lane/trees/`. On APFS, btrfs, and
reflink-enabled XFS, ignored files are cloned by reference; otherwise lane creates a normal
Git worktree and skips them. `--base <rev>` branches from a specific ref instead of the
default base, and `--dirty` carries uncommitted work into the new lane.

`lane ls` lists each lane's state (`open`, `pushed`, or `landed`) and worktree status; add
`--json` for machine-readable output.

`lane rm <name>` removes a lane's branch and worktree, refusing if it would discard
uncommitted work or commits trunk does not have. `--force` discards it anyway.

`lane prune` removes every lane whose branch has landed — its remote retired, or its commits
already contained in trunk — leaving open lanes and anything committed after landing alone.
Add `--dry-run` to see what it would remove without removing anything.

## Development

```sh
$ ./scripts/build.sh          # release-build and install the local lane binary
$ cargo fmt --all --check
$ cargo clippy --workspace --all-targets -- -D warnings
$ cargo test --workspace
$ ./scripts/test.sh           # end to end against temporary Git repositories
$ ./scripts/check-linux.sh    # run the same gates without reflink support
$ ./scripts/bench.sh          # time `lane new`/`lane rm` against a synthetic repository
```

## License

MIT © [Luke Edwards](https://lukeed.com)
