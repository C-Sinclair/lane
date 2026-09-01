//! Lane lifecycle: create, list, land, remove.

use crate::cow;
use crate::git::{git, git_ok, layout, try_git};
use anyhow::{Context, Result, bail};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

pub const LANE_DIR: &str = ".lane";
const TREES_DIRNAME: &str = "trees";
const TREES_PATH: &str = ".lane/trees";

/// Root of the primary worktree, even when called from inside a lane.
pub fn main_root() -> Result<PathBuf> {
    Ok(layout(&std::env::current_dir()?)?.main_root)
}

pub fn trunk_name(root: &Path) -> String {
    static TRUNKS: OnceLock<Mutex<HashMap<PathBuf, String>>> = OnceLock::new();
    let trunks = TRUNKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut trunks = trunks
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(trunk) = trunks.get(root) {
        return trunk.clone();
    }

    let trunk = trunk_name_uncached(root);
    trunks.insert(root.to_path_buf(), trunk.clone());
    trunk
}

fn trunk_name_uncached(root: &Path) -> String {
    let origin = try_git(
        &[
            "symbolic-ref",
            "--quiet",
            "--short",
            "refs/remotes/origin/HEAD",
        ],
        Some(root),
    );
    let named = origin.strip_prefix("origin/").into_iter();
    for candidate in named.chain(["main", "master", "trunk"]) {
        if git_ok(&["rev-parse", "--verify", "--quiet", candidate], Some(root)) {
            return candidate.into();
        }
    }
    try_git(&["rev-parse", "--abbrev-ref", "HEAD"], Some(root))
}

fn new_base(root: &Path) -> String {
    let branch = try_git(&["rev-parse", "--abbrev-ref", "HEAD"], Some(root));
    if !branch.is_empty() && branch != "HEAD" {
        branch
    } else {
        trunk_name(root)
    }
}

/// Where a lane's fork commit is kept.
///
/// A ref, not a config string, so the commit stays reachable: a config value naming a
/// commit is opaque to git, and a base branch that is later reset or rewritten leaves that
/// commit collectable, taking the marker with it at the next gc.
///
/// Lane names are branch names, so this namespace inherits git's own rule that `a` and
/// `a/b` cannot both be branches — the directory/file collision is already impossible.
fn fork_ref(branch: &str) -> String {
    format!("refs/lane/{branch}")
}

/// The commit a lane forked from, recorded once at creation.
///
/// Refs alone cannot separate a lane that never committed from one whose work has fully
/// merged: both leave the branch tip at its merge-base with trunk. This is what tells them
/// apart, and without it `prune` would collect a lane the moment it was created.
fn record_fork(root: &Path, branch: &str, base: &str) -> Result<()> {
    // Peeled: a tag as base would otherwise record the tag object, which matches no tip.
    let sha = try_git(&["rev-parse", &format!("{base}^{{commit}}")], Some(root));
    if sha.is_empty() {
        return Ok(());
    }
    git(&["update-ref", &fork_ref(branch), &sha], Some(root))?;
    Ok(())
}

fn forget_fork(root: &Path, branch: &str) {
    try_git(&["update-ref", "-d", &fork_ref(branch)], Some(root));
}

pub(crate) fn fork_point(root: &Path, branch: &str) -> Option<String> {
    let sha = try_git(
        &["rev-parse", "--verify", "--quiet", &fork_ref(branch)],
        Some(root),
    );
    (!sha.is_empty()).then_some(sha)
}

pub fn lanes_dir(root: &Path) -> PathBuf {
    root.join(LANE_DIR).join(TREES_DIRNAME)
}

/// Tracked changes only: untracked files do not block a rebase.
pub fn is_dirty(path: &Path) -> bool {
    !try_git(
        &["status", "--porcelain", "--untracked-files=no"],
        Some(path),
    )
    .trim()
    .is_empty()
}

/// Every checkout of this repository, canonicalized so a path can be compared to one.
fn checkouts(root: &Path) -> Vec<PathBuf> {
    try_git(&["worktree", "list", "--porcelain"], Some(root))
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .map(|path| {
            let path = PathBuf::from(path);
            path.canonicalize().unwrap_or(path)
        })
        .collect()
}

/// Whether an ignored entry holds a checkout rather than a build cache.
///
/// git collapses an ignored directory to its shallowest root, so an entry can be an
/// ancestor of the lanes directory instead of equal to it — and a directory holding
/// another tool's worktrees is a checkout too. Neither is a cache worth carrying, and
/// cloning one copies whole sibling working trees into the new lane.
fn holds_a_checkout(base: &Path, entry: &str, checkouts: &[PathBuf]) -> bool {
    let path = base.join(entry);
    if path == lanes_dir(base) || lanes_dir(base).starts_with(&path) {
        return true;
    }
    let canonical = path.canonicalize().unwrap_or(path);
    checkouts
        .iter()
        .any(|checkout| *checkout != canonical && checkout.starts_with(&canonical))
}

/// Entries git will not materialize: exactly what a fresh worktree is missing.
/// Already collapsed to directory roots, at any depth, from the user's own ignore rules.
fn ignored_entries(root: &Path) -> Vec<String> {
    let checkouts = checkouts(root);
    try_git(&["status", "--porcelain", "-z", "--ignored"], Some(root))
        .split('\0')
        .filter_map(|e| e.strip_prefix("!! "))
        .map(|p| p.trim_end_matches('/').to_string())
        .filter(|p| !p.is_empty() && p != ".git")
        .filter(|p| !holds_a_checkout(root, p, &checkouts))
        .collect()
}

/// git 2.48+ supports relative paths. Older versions reject just that option, then use
/// absolute paths, which work in place but not after a move.
fn add_worktree(root: &Path, args: &[&str]) -> Result<()> {
    let mut relative_args = vec!["worktree", "add", "--relative-paths"];
    relative_args.extend(args);
    match git(&relative_args, Some(root)) {
        Ok(_) => Ok(()),
        Err(error) if rejects_relative_paths(&error) => {
            let mut absolute_args = vec!["worktree", "add"];
            absolute_args.extend(args);
            git(&absolute_args, Some(root)).map(|_| ())
        }
        Err(error) => Err(error),
    }
}

fn rejects_relative_paths(error: &anyhow::Error) -> bool {
    let message = error.to_string();
    message.contains("relative-paths")
        && (message.contains("unknown option") || message.contains("unrecognized option"))
}

fn append_line(path: &Path, line: &str) -> Result<()> {
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    if existing.contains(line) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    if !existing.is_empty() && !existing.ends_with('\n') {
        writeln!(file)?;
    }
    writeln!(file, "{line}")?;
    Ok(())
}

fn prepare_lanes_dir(root: &Path) -> Result<()> {
    let dir = lanes_dir(root);
    std::fs::create_dir_all(&dir)?;
    let ignore = dir.join(".gitignore");
    if !ignore.exists() {
        std::fs::write(ignore, "*\n")?;
    }
    // --git-path answers relative to the repository, not to wherever the process stands.
    let exclude = git(&["rev-parse", "--git-path", "info/exclude"], Some(root))?;
    append_line(&root.join(exclude), &format!("{TREES_PATH}/"))
}

fn excluded(root: &Path) -> HashSet<String> {
    try_git(&["config", "--get-all", "lane.exclude"], Some(root))
        .lines()
        .map(|p| p.trim_end_matches('/').to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

pub struct Lane {
    pub path: PathBuf,
    pub branch: String,
    /// The name that addresses this lane on the command line.
    pub name: String,
}

/// A lane's name is its path below `.lane/trees/`, not the last segment of it: a lane on
/// `feat/login` lives two directories deep, and `login` alone addresses nothing.
pub(crate) fn name_of(root: &Path, path: &Path) -> String {
    let dir = lanes_dir(root);
    if let Ok(rel) = path.strip_prefix(&dir) {
        return rel.to_string_lossy().to_string();
    }
    // The two spellings can differ (`/private` on macOS); strip canonically before giving
    // up, or the name falls back to an absolute path that addresses nothing.
    let canonical = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    let (full, dir) = (canonical(path), canonical(&dir));
    match full.strip_prefix(&dir) {
        Ok(rel) => rel.to_string_lossy().to_string(),
        Err(_) => path.to_string_lossy().to_string(),
    }
}

/// Whether a worktree path is a lane, i.e. lives under `.lane/trees/`.
///
/// A repository can hold worktrees lane did not create — `git worktree add` by hand,
/// `git-wt`'s `.wt/`, an agent's `.claude/worktrees/`. Those are someone else's checkouts:
/// listing them misreports them as lanes, and `--prune` would delete them and their
/// branches. Membership is by location, the same rule that decides where `create` puts one.
///
/// Compared canonically: git reports `/private/...` on macOS where the repository root
/// retains the shorter spelling, and a mismatch there would hide every real lane.
fn is_lane(root: &Path, path: &Path) -> bool {
    let dir = lanes_dir(root);
    if path.starts_with(&dir) {
        return true;
    }
    let canonical = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    canonical(path).starts_with(canonical(&dir))
}

pub fn list_lanes(root: &Path) -> Vec<Lane> {
    let out = try_git(&["worktree", "list", "--porcelain"], Some(root));
    let mut lanes = Vec::new();
    let (mut path, mut branch) = (String::new(), String::new());

    let flush = |path: &mut String, branch: &mut String, lanes: &mut Vec<Lane>| {
        let candidate = Path::new(path.as_str());
        if !path.is_empty() && candidate != root && is_lane(root, candidate) {
            let path_buf = PathBuf::from(path.as_str());
            lanes.push(Lane {
                name: name_of(root, &path_buf),
                path: path_buf,
                branch: if branch.is_empty() {
                    "detached".into()
                } else {
                    branch.clone()
                },
            });
        }
        path.clear();
        branch.clear();
    };

    for line in out.lines() {
        if line.trim().is_empty() {
            flush(&mut path, &mut branch, &mut lanes);
        } else if let Some(v) = line.strip_prefix("worktree ") {
            path = v.to_string();
        } else if let Some(v) = line.strip_prefix("branch ") {
            branch = v.trim_start_matches("refs/heads/").to_string();
        }
    }
    flush(&mut path, &mut branch, &mut lanes);
    lanes
}

pub struct Created {
    pub path: PathBuf,
    pub stats: cow::CloneStats,
    pub notes: Vec<String>,
}

#[derive(Debug, PartialEq)]
enum Materialization {
    Plain,
    Ignored,
    Dirty,
    /// Explicit --dirty without reflink: the caches are not worth a byte copy, the
    /// handful of edited files is.
    DirtyPlain,
}

fn materialization(dirty: bool, reflink: bool) -> Materialization {
    match (dirty, reflink) {
        (false, false) => Materialization::Plain,
        (true, false) => Materialization::DirtyPlain,
        (false, true) => Materialization::Ignored,
        (true, true) => Materialization::Dirty,
    }
}

/// Uncommitted work: tracked files that differ from HEAD, plus untracked non-ignored ones.
fn uncommitted(root: &Path) -> Vec<String> {
    let mut paths: Vec<String> = try_git(&["diff", "--name-only", "-z", "HEAD"], Some(root))
        .split('\0')
        .map(str::to_string)
        .collect();
    paths.extend(
        try_git(
            &["ls-files", "--others", "--exclude-standard", "-z"],
            Some(root),
        )
        .split('\0')
        .map(str::to_string),
    );
    paths.retain(|p| !p.is_empty());
    paths
}

fn add_stats(total: &mut cow::CloneStats, next: cow::CloneStats) {
    total.cloned += next.cloned;
    total.copied += next.copied;
    total.links += next.links;
    total.bytes_shared += next.bytes_shared;
    total.bytes_copied += next.bytes_copied;
}

fn clone_entry(root: &Path, dest: &Path, entry: &str) -> Result<cow::CloneStats> {
    let source = root.join(entry);
    let target = dest.join(entry);
    if std::fs::symlink_metadata(&source)?.is_dir() {
        return Ok(cow::clone_dir_tree(&source, &target, root, dest)?);
    }
    let Some(name) = source.file_name().map(|name| name.to_string_lossy()) else {
        bail!("ignored entry has no file name: {entry}");
    };
    let Some(source_parent) = source.parent() else {
        bail!("ignored entry has no parent: {entry}");
    };
    let Some(target_parent) = target.parent() else {
        bail!("ignored entry has no destination parent: {entry}");
    };
    Ok(cow::clone_tree_rooted(
        source_parent,
        target_parent,
        &|rel, _| rel != name,
        root,
        dest,
    )?)
}

fn branch_args<'a>(adopt: bool, name: &'a str, dest: &'a str, base: &'a str) -> Vec<&'a str> {
    if adopt {
        vec![dest, name]
    } else {
        vec!["-b", name, dest, base]
    }
}

/// Remote-tracking branches of the same name, one per remote that publishes it.
///
/// Only refs already fetched: creating a lane is not the moment to reach the network, and
/// a lane whose name happens to match an unfetched branch is an ordinary new branch.
fn upstream_matches(root: &Path, name: &str) -> Vec<String> {
    try_git(&["remote"], Some(root))
        .lines()
        .map(str::trim)
        .filter(|remote| !remote.is_empty())
        .map(|remote| format!("{remote}/{name}"))
        .filter(|candidate| {
            git_ok(
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    &format!("refs/remotes/{candidate}"),
                ],
                Some(root),
            )
        })
        .collect()
}

/// By default git checks out tracked files and ignored entries are cloned by reference.
pub fn create(name: &str, base: Option<&str>, dirty: bool) -> Result<Created> {
    let root = main_root()?;
    // An existing branch is adopted, not recreated: a fetched pull request needs a lane
    // to be reviewed or landed in, and a lane pruned early needs one to come back to.
    let adopt = git_ok(
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{name}"),
        ],
        Some(&root),
    );
    if adopt && base.is_some() {
        bail!("branch {name} already exists; --base applies only to a new branch");
    }
    // A lane named after a branch that already exists on a remote continues that work
    // rather than starting a parallel branch from local HEAD, which would silently leave
    // the published commits behind. An explicit --base is the way to say otherwise.
    let mut track = false;
    let mut upstream = Vec::new();
    if !adopt && base.is_none() {
        upstream = upstream_matches(&root, name);
        if upstream.len() > 1 {
            bail!(
                "{name} exists on more than one remote ({}); name one with --base",
                upstream.join(", ")
            );
        }
        track = upstream.len() == 1;
    }
    let base = base
        .map(str::to_string)
        .or_else(|| upstream.first().cloned())
        .unwrap_or_else(|| new_base(&root));
    let dest = lanes_dir(&root).join(name);
    if dest.exists() {
        bail!("lane {name} already exists at {}", dest.display());
    }
    prepare_lanes_dir(&root)?;

    let (supported, detail) = cow::probe(&root);
    let mut notes = vec![format!(
        "reflink: {} ({detail})",
        if supported { "yes" } else { "no" }
    )];
    if !supported {
        notes.push("no reflink here; leaving a plain worktree".into());
    }
    if track {
        notes.push(format!("branched from {base} and tracking it"));
    }
    if !dirty {
        let carried = try_git(
            &["status", "--porcelain", "--untracked-files=no"],
            Some(&root),
        )
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();
        if carried > 0 {
            notes.push(format!(
                "warning: {carried} uncommitted change(s) were not carried\n    lane -D {name} && lane {name} --dirty   to start over with them"
            ));
        }
    }
    let dest_str = dest.to_string_lossy().to_string();

    let stats = match materialization(dirty, supported) {
        Materialization::Dirty => {
            let mut args = vec!["--no-checkout"];
            if track {
                args.push("--track");
            }
            args.extend(branch_args(adopt, name, &dest_str, &base));
            add_worktree(&root, &args)?;
            // Nested checkouts are pruned here for the same reason `ignored_entries`
            // drops them: they are working trees, not caches this lane should carry.
            let checkouts = checkouts(&root);
            let skip = |rel: &str, is_dir: bool| {
                rel == ".git"
                    || rel.starts_with(".git/")
                    || (is_dir && holds_a_checkout(&root, rel, &checkouts))
            };
            let stats = cow::clone_tree(&root, &dest, &skip)?;
            // Repopulate the index from the checked-out tree without rewriting a single
            // file. HEAD, not base: an adopted branch is already at its own tip.
            git(&["reset", "--mixed", "--quiet", "HEAD"], Some(&dest))?;
            try_git(&["update-index", "--refresh"], Some(&dest));
            let carried = try_git(&["status", "--porcelain"], Some(&dest))
                .lines()
                .filter(|l| !l.trim().is_empty())
                .count();
            if carried > 0 {
                notes.push(format!(
                    "carried {carried} uncommitted change(s) from the parent tree"
                ));
            }
            stats
        }
        mode => {
            let ignored = if mode == Materialization::Ignored {
                ignored_entries(&root)
            } else {
                Vec::new()
            };
            let carry = if mode == Materialization::DirtyPlain {
                uncommitted(&root)
            } else {
                Vec::new()
            };
            let excluded = excluded(&root);
            let mut args = Vec::new();
            if track {
                args.push("--track");
            }
            args.extend(branch_args(adopt, name, &dest_str, &base));
            add_worktree(&root, &args)?;
            let mut stats = cow::CloneStats::default();
            for entry in ignored {
                if excluded.contains(&entry) {
                    continue;
                }
                let next = clone_entry(&root, &dest, &entry)
                    .with_context(|| format!("cloning ignored entry {entry}"))?;
                add_stats(&mut stats, next);
            }
            for path in &carry {
                let (from, to) = (root.join(path), dest.join(path));
                if let Some(parent) = to.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(&from, &to)
                    .with_context(|| format!("carrying uncommitted {path}"))?;
                stats.copied += 1;
            }
            if !carry.is_empty() {
                notes.push(format!(
                    "carried {} uncommitted change(s) by copy; no reflink here",
                    carry.len()
                ));
            }
            stats
        }
    };

    record_fork(&root, name, &base)?;
    crate::registry::register_best_effort(&root);
    // Membership just changed; see cache.rs for why this can't wait on the TTL.
    crate::cache::invalidate();

    Ok(Created {
        path: dest,
        stats,
        notes,
    })
}

fn trash_dir(root: &Path) -> PathBuf {
    lanes_dir(root).join(".trash")
}

/// Move the bulk aside so git's removal only unlinks what it tracks.
///
/// Renaming is constant time where deleting is one syscall per file, and nothing here is
/// worth waiting for: these are the entries git itself declined to materialize.
fn park(root: &Path, dest: &Path) -> Vec<(PathBuf, PathBuf)> {
    let trash = trash_dir(root);
    if std::fs::create_dir_all(&trash).is_err() {
        return Vec::new();
    }
    let mut parked = Vec::new();
    for entry in ignored_entries(dest) {
        let from = dest.join(&entry);
        if std::fs::symlink_metadata(&from).is_err() {
            continue;
        }
        let to = trash.join(format!("{}-{}", std::process::id(), parked.len()));
        if std::fs::rename(&from, &to).is_ok() {
            parked.push((from, to));
        }
    }
    parked
}

/// Unlink what was parked, in a process that outlives this one. Picks up anything an
/// earlier sweep left behind, so a killed child costs disk and not correctness.
fn sweep(root: &Path) {
    let trash = trash_dir(root);
    let Ok(entries) = std::fs::read_dir(&trash) else {
        return;
    };
    let paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    if paths.is_empty() {
        return;
    }
    let _ = std::process::Command::new("rm")
        .arg("-rf")
        .args(paths)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

/// True while git still has a worktree registered at this path, prunable ones included.
pub fn registered(root: &Path, dest: &Path) -> bool {
    let target = dest.canonicalize().ok();
    list_lanes(root).iter().any(|lane| {
        lane.path == dest || (target.is_some() && lane.path.canonicalize().ok() == target)
    })
}

/// What removing this lane destroys for good. Empty means nothing is at stake.
///
/// Every caller of `remove` asks this first. `git branch -d` is the weaker question: it
/// refuses every squash and rebase merge.
pub fn losses(root: &Path, path: &Path, branch: &str, trunk: &str) -> Vec<String> {
    let mut out = Vec::new();
    if path.is_dir() {
        // Untracked counts here where it does not for a rebase: removal deletes the file.
        let changed = try_git(&["status", "--porcelain"], Some(path))
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count();
        if changed > 0 {
            out.push(format!("{changed} uncommitted change(s)"));
        }
    }
    let refname = format!("refs/heads/{branch}");
    if !git_ok(&["rev-parse", "--verify", "--quiet", &refname], Some(root)) {
        return out;
    }

    // A lane that never committed has no commits to lose, landed or not.
    if started(root, branch) && !landed(root, trunk, branch) {
        // No count: a squash merge leaves commits whose patches landed inside one of
        // trunk's, so `rev-list` would name a number larger than what is really at risk.
        out.push(format!("commits {trunk} does not have"));
    }
    out
}

/// Remove a lane's worktree and its branch, and with them everything the lane still held.
///
/// Unconditional by design: `losses` is the guard, and every caller runs it first.
pub fn remove(name: &str) -> Result<()> {
    let root = main_root()?;
    let dest = lanes_dir(&root).join(name);

    // Deleting the directory the caller is standing in leaves their shell in a path that no
    // longer exists, which is the failure plan 006 exists to prevent. `merge` chdirs to the
    // root before it gets here; `rm` and `prune` have no reason to, so refuse instead.
    let inside = std::env::current_dir()
        .ok()
        .and_then(|cwd| cwd.canonicalize().ok())
        .zip(dest.canonicalize().ok())
        .is_some_and(|(cwd, dest)| cwd.starts_with(dest));
    if inside {
        bail!("cannot remove lane {name} from inside it; cd out first");
    }

    let refname = format!("refs/heads/{name}");
    let branch = git_ok(&["rev-parse", "--verify", "--quiet", &refname], Some(&root));
    let worktree = registered(&root, &dest);
    if !branch && !worktree {
        bail!("no lane {name}");
    }

    // A hand-deleted lane leaves a branch and no worktree, and git calls removing an
    // absent one fatal. Skipping is what lets `rm` clean up after that.
    if worktree {
        let parked = park(&root, &dest);
        let dest_str = dest.to_string_lossy().to_string();
        match git(&["worktree", "remove", "--force", &dest_str], Some(&root)) {
            Ok(_) => sweep(&root),
            Err(error) => {
                for (from, to) in parked {
                    let _ = std::fs::rename(to, from);
                }
                return Err(error);
            }
        }
    }
    if branch {
        git(&["branch", "-D", name], Some(&root))?;
    }
    forget_fork(&root, name);
    if dest.exists() {
        let _ = std::fs::remove_dir_all(&dest);
    }
    // Covers `-d`/`-D` and every lane `--prune` actually removes, since both call this.
    crate::cache::invalidate();
    Ok(())
}

fn tracked_changes(status: &str) -> Vec<String> {
    let mut entries = status.split('\0');
    let mut paths = Vec::new();
    while let Some(entry) = entries.next() {
        if entry.starts_with("## ") {
            continue;
        }
        let Some(path) = entry.get(3..) else { continue };
        paths.push(path.to_string());
        if entry.as_bytes()[..2]
            .iter()
            .any(|s| matches!(s, b'R' | b'C'))
            && let Some(source) = entries.next()
        {
            paths.push(source.to_string());
        }
    }
    paths
}

/// Files a fast-forward would overwrite in the main worktree. Empty means `merge` can land.
pub fn blocking_changes(root: &Path, trunk: &str, branch: &str) -> Vec<String> {
    // Only the merge path touches a working tree; update-ref cannot conflict.
    if try_git(&["rev-parse", "--abbrev-ref", "HEAD"], Some(root)) != trunk {
        return Vec::new();
    }
    let incoming: HashSet<String> = try_git(
        &[
            "diff",
            "--name-only",
            "--no-renames",
            "-z",
            &format!("{trunk}..{branch}"),
        ],
        Some(root),
    )
    .split('\0')
    .filter(|p| !p.is_empty())
    .map(str::to_string)
    .collect();
    // The branch header protects the first status column from try_git's trim.
    tracked_changes(&try_git(
        &[
            "status",
            "--porcelain",
            "-z",
            "--branch",
            "--untracked-files=no",
        ],
        Some(root),
    ))
    .into_iter()
    .filter(|p| incoming.contains(p))
    .collect()
}

/// Is everything on `branch` already represented in `trunk`?
///
/// Three answers for GitHub's three merge buttons. A merge commit leaves the branch an
/// ancestor. Anything else rewrites the SHAs, so `git branch -d` refuses even when the
/// trees are identical; comparing the branch's cumulative diff against trunk by patch-id
/// is what sees through a rebase or a squash.
/// Git's own name for a branch its remote retired: upstream configured, ref no longer there.
///
/// Proof of arrival that compares no patches, which is what lets it see through a squash.
/// Reads a cache, so it answers for the last fetch. Not `%(push:track)`, which reports
/// `[gone]` for a branch never pushed, having answered for where a push would land.
pub fn upstream_gone(root: &Path, branch: &str) -> bool {
    let refname = format!("refs/heads/{branch}");
    try_git(
        &["for-each-ref", "--format=%(upstream:track)", &refname],
        Some(root),
    )
    .trim()
        == "[gone]"
}

/// Whether a lane's work has reached trunk.
///
/// No landing markers exist; git refs are the only witness. A retired upstream and
/// containment in trunk are two different proofs of arrival, either one sufficient.
///
/// A lane that has committed nothing is excluded first. Its tip is trunk's, so containment
/// holds vacuously, and calling that "landed" would let `prune` delete a lane the moment it
/// was created.
pub fn landed(root: &Path, trunk: &str, branch: &str) -> bool {
    if !started(root, branch) {
        return false;
    }
    upstream_gone(root, branch) || contained_in(root, trunk, branch)
}

/// Whether a branch holds any commit of its own beyond where it forked.
///
/// An unrecorded fork answers yes: a lane this tool did not create is judged on its refs
/// alone, where leaving it uncollectable forever is the worse of the two failures.
fn started(root: &Path, branch: &str) -> bool {
    let Some(fork) = fork_point(root, branch) else {
        return true;
    };
    try_git(&["rev-parse", branch], Some(root)) != fork
}

pub fn contained_in(root: &Path, trunk: &str, branch: &str) -> bool {
    if git_ok(&["merge-base", "--is-ancestor", branch, trunk], Some(root)) {
        return true;
    }
    // A rebase merge replays the commits one for one, so their patches land separately.
    // The collapsed probe below is the squash answer and matches none of them.
    let replayed = try_git(&["cherry", trunk, branch], Some(root));
    if !replayed.is_empty() && replayed.lines().all(|line| line.starts_with('-')) {
        return true;
    }
    let Ok(base) = git(&["merge-base", trunk, branch], Some(root)) else {
        return false;
    };
    let Ok(tree) = git(&["rev-parse", &format!("{branch}^{{tree}}")], Some(root)) else {
        return false;
    };
    // An empty probe has no patch-id to match, so `cherry` would call it unmerged.
    if git(&["rev-parse", &format!("{base}^{{tree}}")], Some(root)).is_ok_and(|b| b == tree) {
        return true;
    }
    let Ok(probe) = git(
        &[
            "commit-tree",
            &tree,
            "-p",
            &base,
            "-m",
            "lane: containment probe",
        ],
        Some(root),
    ) else {
        return false;
    };
    let cherry = try_git(&["cherry", trunk, &probe], Some(root));
    !cherry.is_empty() && cherry.lines().all(|line| line.starts_with('-'))
}

/// A lane's `open`/`pushed`/`landed` state, shared by the local listing and `-g`.
pub fn lane_state(root: &Path, trunk: &str, lane: &Lane) -> &'static str {
    if landed(root, trunk, &lane.branch) {
        return "landed";
    }
    let upstream = try_git(&["rev-parse", "@{upstream}"], Some(&lane.path));
    if !upstream.is_empty() && try_git(&["rev-parse", "HEAD"], Some(&lane.path)) == upstream {
        return "pushed";
    }
    "open"
}

/// Advance trunk to branch: merge when trunk is checked out, update-ref when it is not.
pub fn fast_forward(root: &Path, trunk: &str, branch: &str) -> Result<()> {
    let head = git(&["rev-parse", "--abbrev-ref", "HEAD"], Some(root))?;
    let target = git(&["rev-parse", branch], Some(root))?;
    if head == trunk {
        git(&["merge", "--ff-only", branch], Some(root))?;
        return Ok(());
    }
    let base = git(&["merge-base", trunk, branch], Some(root))?;
    if base != git(&["rev-parse", trunk], Some(root))? {
        bail!("trunk {trunk} has diverged from {branch}; rebase first");
    }
    git(
        &["update-ref", &format!("refs/heads/{trunk}"), &target],
        Some(root),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lanes_live_inside_the_repository() {
        let root = Path::new("/repo");

        assert_eq!(lanes_dir(root), root.join(".lane/trees"));
    }

    #[test]
    fn ignored_entries_excludes_the_lanes_directory() -> Result<()> {
        let root = tempfile::tempdir()?;
        let r = root.path();
        let run = |args: &[&str]| {
            git(args, Some(r)).ok();
        };
        run(&["init", "-qb", "main"]);
        run(&["config", "user.email", "t@t.t"]);
        run(&["config", "user.name", "t"]);
        // A developer signing every commit makes gpg a dependency of the test suite, and
        // under parallel load it fails to allocate and takes the run with it.
        run(&["config", "commit.gpgsign", "false"]);
        std::fs::write(r.join(".gitignore"), ".lane/\ncache/\n.wt/\n")?;
        run(&["add", ".gitignore"]);
        run(&["commit", "-qm", "base"]);
        std::fs::create_dir_all(r.join(".lane/trees/other"))?;
        std::fs::create_dir_all(r.join("cache"))?;
        std::fs::write(r.join("cache/blob"), "cache")?;
        // Another tool's worktree directory: a checkout, not a cache worth carrying.
        run(&["worktree", "add", "-q", "-b", "side", ".wt/side"]);

        let entries = ignored_entries(r);

        assert!(entries.contains(&"cache".to_string()));
        // git collapses the ignored directory to `.lane`, never to `.lane/trees`, which is
        // why comparing against TREES_PATH alone let every sibling lane be cloned.
        assert!(!entries.contains(&".lane".to_string()));
        assert!(!entries.contains(&TREES_PATH.to_string()));
        assert!(!entries.contains(&".wt".to_string()));
        Ok(())
    }

    #[test]
    fn a_lane_name_matching_a_fetched_remote_branch_finds_its_upstream() -> Result<()> {
        let home = tempfile::tempdir()?;
        let origin = home.path().join("origin");
        let clone = home.path().join("clone");
        let run = |args: &[&str], cwd: &Path| {
            git(args, Some(cwd)).ok();
        };
        std::fs::create_dir_all(&origin)?;
        run(&["init", "-qb", "main"], &origin);
        run(&["config", "user.email", "t@t.t"], &origin);
        run(&["config", "user.name", "t"], &origin);
        run(&["config", "commit.gpgsign", "false"], &origin);
        std::fs::write(origin.join("f"), "f")?;
        run(&["add", "f"], &origin);
        run(&["commit", "-qm", "base"], &origin);
        run(&["branch", "feature"], &origin);
        git(
            &[
                "clone",
                "-q",
                &origin.to_string_lossy(),
                &clone.to_string_lossy(),
            ],
            None,
        )?;

        assert_eq!(upstream_matches(&clone, "feature"), vec!["origin/feature"]);
        // A name nothing publishes is an ordinary new branch, not an error.
        assert!(upstream_matches(&clone, "nothing-upstream").is_empty());
        Ok(())
    }

    #[test]
    fn parking_moves_the_bulk_aside_and_the_sweep_unlinks_it() -> Result<()> {
        let root = tempfile::tempdir()?;
        let r = root.path();
        let run = |args: &[&str]| {
            git(args, Some(r)).ok();
        };
        run(&["init", "-qb", "main"]);
        run(&["config", "user.email", "t@t.t"]);
        run(&["config", "user.name", "t"]);
        // A developer signing every commit makes gpg a dependency of the test suite, and
        // under parallel load it fails to allocate and takes the run with it.
        run(&["config", "commit.gpgsign", "false"]);
        std::fs::write(r.join(".gitignore"), "build/\n")?;
        run(&["add", "-A"]);
        run(&["commit", "-qm", "base"]);
        std::fs::create_dir_all(r.join("build/deep"))?;
        std::fs::write(r.join("build/deep/artifact"), "bulk")?;

        let parked = park(r, r);

        assert_eq!(parked.len(), 1, "the ignored tree is the thing to park");
        assert!(!r.join("build").exists(), "parking leaves the worktree");
        let (from, to) = &parked[0];
        assert_eq!(from, &r.join("build"));
        assert!(to.join("deep/artifact").exists(), "content moved intact");

        sweep(r);

        // The unlinking outlives this process, so wait on it rather than assume it.
        for _ in 0..100 {
            if std::fs::read_dir(trash_dir(r))
                .into_iter()
                .flatten()
                .count()
                == 0
            {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        panic!("sweep left the trash behind");
    }

    #[test]
    fn the_exclude_file_is_found_from_outside_the_repository() -> Result<()> {
        let root = tempfile::tempdir()?;
        let r = root.path();
        git(&["init", "-qb", "main"], Some(r))?;
        std::fs::remove_dir_all(r.join(".git/info"))?;

        // The test process stands in the crate directory, never in `r`: the same relation a
        // lane has to the repository it was made from. A custom init template may also omit
        // info entirely, so setup must recreate the exclude file's parent.
        prepare_lanes_dir(r)?;

        let exclude = std::fs::read_to_string(r.join(".git/info/exclude"))?;
        assert!(exclude.contains(TREES_PATH), "{exclude}");
        Ok(())
    }

    #[test]
    fn no_reflink_skips_the_caches_but_still_honours_dirty() {
        assert_eq!(materialization(false, false), Materialization::Plain);
        assert_eq!(materialization(true, false), Materialization::DirtyPlain);
    }

    #[test]
    fn reflink_selects_the_requested_mode() {
        assert_eq!(materialization(false, true), Materialization::Ignored);
        assert_eq!(materialization(true, true), Materialization::Dirty);
    }

    #[test]
    fn uncommitted_finds_tracked_edits_and_untracked_files() -> Result<()> {
        let root = tempfile::tempdir()?;
        let r = root.path();
        let run = |args: &[&str]| {
            git(args, Some(r)).ok();
        };
        run(&["init", "-qb", "main"]);
        run(&["config", "user.email", "t@t.t"]);
        run(&["config", "user.name", "t"]);
        // A developer signing every commit makes gpg a dependency of the test suite, and
        // under parallel load it fails to allocate and takes the run with it.
        run(&["config", "commit.gpgsign", "false"]);
        std::fs::create_dir_all(r.join("src"))?;
        std::fs::write(r.join("src/a.rs"), "one\n")?;
        std::fs::write(r.join(".gitignore"), "ignored.txt\n")?;
        run(&["add", "-A"]);
        run(&["commit", "-qm", "base"]);

        std::fs::write(r.join("src/a.rs"), "two\n")?; // tracked edit
        std::fs::write(r.join("scratch.txt"), "x")?; // untracked, not ignored
        std::fs::write(r.join("ignored.txt"), "x")?; // ignored, not uncommitted work

        let mut found = uncommitted(r);
        found.sort();
        assert_eq!(
            found,
            vec!["scratch.txt".to_string(), "src/a.rs".to_string()]
        );
        Ok(())
    }

    #[test]
    fn an_ignored_file_is_cloned() -> Result<()> {
        let root = tempfile::tempdir()?;
        let dest = tempfile::tempdir()?;
        std::fs::write(root.path().join(".env"), "SECRET=1")?;

        clone_entry(root.path(), dest.path(), ".env")?;

        assert_eq!(
            std::fs::read_to_string(dest.path().join(".env"))?,
            "SECRET=1"
        );
        Ok(())
    }

    #[test]
    fn tracked_changes_keeps_spaces() {
        assert_eq!(
            tracked_changes("## main\0 M src/auth flow.rs\0"),
            vec!["src/auth flow.rs"]
        );
    }

    #[test]
    fn tracked_changes_includes_both_sides_of_a_rename() {
        assert_eq!(
            tracked_changes("## main\0R  src/new.rs\0src/old.rs\0"),
            vec!["src/new.rs", "src/old.rs"]
        );
    }

    fn repository(branch: &str) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| git(args, Some(root.path())).unwrap();
        run(&["init", "-qb", branch]);
        run(&["config", "user.email", "t@t.t"]);
        run(&["config", "user.name", "t"]);
        // A developer signing every commit makes gpg a dependency of the test suite, and
        // under parallel load it fails to allocate and takes the run with it.
        run(&["config", "commit.gpgsign", "false"]);
        std::fs::write(root.path().join("file"), "one\n").unwrap();
        run(&["add", "file"]);
        run(&["commit", "-qm", "base"]);
        root
    }

    #[test]
    fn new_base_prefers_the_main_worktree_branch() {
        let root = repository("develop");
        git(&["branch", "main"], Some(root.path())).unwrap();

        assert_eq!(new_base(root.path()), "develop");
    }

    #[test]
    fn trunk_name_resolves_origin_head() {
        let root = repository("develop");
        git(
            &["update-ref", "refs/remotes/origin/develop", "HEAD"],
            Some(root.path()),
        )
        .unwrap();
        git(
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/develop",
            ],
            Some(root.path()),
        )
        .unwrap();

        assert_eq!(trunk_name(root.path()), "develop");
    }

    #[test]
    fn trunk_name_probes_when_origin_head_is_absent() {
        let root = repository("develop");
        git(&["branch", "main"], Some(root.path())).unwrap();
        git(&["checkout", "--detach"], Some(root.path())).unwrap();

        assert_eq!(trunk_name(root.path()), "main");
    }

    #[test]
    fn trunk_name_ignores_a_different_checked_out_branch() {
        let root = repository("main");
        git(&["checkout", "-qb", "develop"], Some(root.path())).unwrap();

        assert_eq!(trunk_name(root.path()), "main");
    }
}
