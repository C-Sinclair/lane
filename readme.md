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

For each new shell, install the `lane --shellenv` wrapper so a bare `lane <name>` and
`lane --exit` `cd` for you. Add it to `.zshrc` or `.bashrc`:

```sh
eval "$(lane --shellenv)"
```

For fish, add this to `config.fish` instead:

```sh
lane --shellenv fish | source
```

> Without it, those commands still print the destination path; you just have to `cd` there
> yourself.

Then initialize each repository:

```sh
$ cd yourrepo
$ lane --init
```

This creates `.lane/` and reports whether the filesystem supports reflinks.

## Usage

```sh
$ lane fix-login

# edit and commit as usual

$ lane --exit
$ lane fix-login   # back into the same lane
$ lane --prune
```

`lane <name>` is the everyday form: it enters the lane if it already exists, or creates it
first if it does not — mirroring `git wt <branch>`. Every bare positional argument is a lane
name; there is no separate command surface to collide with, so a lane may be named anything,
including `ls` or `prune`.

`lane <name>` creates a branch and worktree under `.lane/trees/` when the lane does not exist
yet. On APFS, btrfs, and reflink-enabled XFS, ignored files are cloned by reference; otherwise
lane creates a normal Git worktree and skips them. `--base <rev>` branches from a specific ref
instead of the default base, and `--dirty` carries uncommitted work into the new lane. Both
apply only on creation.

`lane`, or `lane --list`/`-l`, lists each lane's state (`open`, `pushed`, or `landed`) and
worktree status; add `--json` for machine-readable output (with or without `--list`).

`lane -g`/`--global` lists lanes across every repository lane knows about, not just the one
you are standing in: `REPO`, `LANE`, `AGE` (time since the lane branch's last commit),
`DISK`, `COMMITS` (divergence from that repository's trunk, as `+ahead -behind`), and
`STATE`, sorted most-recently-active first. `--json` works with it too, and carries the
commit timestamp as a raw unix time rather than the rendered `AGE` string, and absolute
`path` and `repo_path` values, since `REPO` in the table is only a directory name and a
reader that cannot locate a lane cannot act on one. `-g` walks every
lane's working tree to compute `DISK`, so it is slower than a plain `lane --list` — expect it
to take longer the more lanes you have and the larger their build caches are.

`DISK` is an estimate, not a filesystem measurement: neither APFS nor Linux exposes a cheap
"bytes unique to this file" query, and `stat`'s block count reports a reflinked file's full
allocation whether or not its extents are still shared, so tools like `du` cannot answer this
either. Lane instead sums the apparent size of every file in the lane that differs from its
counterpart in the main checkout by size or modification time, plus every file the lane has
that the main checkout does not — an approximation of the storage a lane has stopped sharing
with the checkout it was cloned from, not a precise accounting of disk blocks.

`-g` reads from a small registry of repository paths at `$XDG_STATE_HOME/lane/repos` (or
`~/.local/state/lane/repos`), one absolute path per line. Lane writes to it on `--init` and
on every successful lane creation, and heals it on every read: an entry whose repository has
moved or been deleted is dropped rather than reported as an error. It is a cache, not
configuration — safe to delete, and lane rebuilds it as you use it again.

`lane -i`/`--info` describes one lane in detail: the lane you are standing in with no name,
or `lane -i <name>` from anywhere in the repo. It reports the lane's branch and path, its
state, when it was created, the commit it forked from, how it has diverged from trunk and
from its upstream, its last commit, uncommitted work, and disk use. `--json` works with it
too, as a single object of raw values — unix timestamps rather than rendered ages, and
separate `ahead`/`behind` counts.

`created` is not something lane stores: it comes from the worktree directory's own
filesystem creation time, which is when the lane was actually made — not the branch's
reflog, which would date the branch instead for a lane that adopted an existing one. `disk`
is the same estimate `-g`'s `DISK` column reports, described above.

`lane -d <name>...` removes one or more lanes' branches and worktrees, refusing on a lane
where it would discard uncommitted work or commits trunk does not have; `-D`/`--force-delete`
discards them anyway.

`lane --prune` removes every lane whose branch has landed — its remote retired, or its commits
already contained in trunk — leaving open lanes and anything committed after landing alone.
Add `--dry-run` to see what it would remove without removing anything.

`lane --completions fish|bash|zsh` prints a completion script for that shell. It completes
lane names as bare arguments and after `-d`/`-D`/`-i`. Install it where your shell looks:

```sh
$ lane --completions fish > ~/.config/fish/completions/lane.fish
$ lane --completions bash > ~/.local/share/bash-completion/completions/lane
$ lane --completions zsh  > ~/.zsh/completions/_lane   # a directory on your $fpath
```

The zsh script is an autoloaded `#compdef` function, so it must be a file named `_lane` on
`$fpath` — sourcing it directly will not work.

## Development

```sh
$ ./scripts/build.sh          # release-build and install the local lane binary
$ cargo fmt --all --check
$ cargo clippy --workspace --all-targets -- -D warnings
$ cargo test --workspace
$ ./scripts/test.sh           # end to end against temporary Git repositories
$ ./scripts/check-linux.sh    # run the same gates without reflink support
$ ./scripts/bench.sh          # time `lane <name>`/`lane -D` against a synthetic repository
```

## License

MIT © [Luke Edwards](https://lukeed.com)
