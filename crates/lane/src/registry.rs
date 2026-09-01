//! The cross-repository registry `-g`/`--global` reads from.
//!
//! Git has no notion of "every repository on this machine"; this is lane's own cache of
//! it, kept as a plain-text file so a human can read or edit it directly. It is state, not
//! config: nothing here is authored by hand in the normal case, and losing it costs nothing
//! but having to visit each repository once more to rebuild it.

use anyhow::{Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Once;

/// `$XDG_STATE_HOME/lane`, falling back to `~/.local/state/lane`. Shared with `cache.rs`,
/// which keeps its snapshot alongside the registry rather than inventing its own rule for
/// where lane's state lives.
pub(crate) fn state_dir() -> PathBuf {
    if let Some(xdg) = std::env::var_os("XDG_STATE_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("lane");
        }
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".local/state/lane")
}

fn registry_path() -> PathBuf {
    state_dir().join("repos")
}

/// Whether `path` is still there and still the main worktree of a repository, rather than a
/// lane, a deleted directory, or something that was never a repository at all.
fn is_live_main_worktree(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }
    let Ok(layout) = crate::git::layout(path) else {
        return false;
    };
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    layout.main_root == canonical
}

/// Read the registry at `path`, dropping and rewriting anything stale.
///
/// Self-healing here, rather than at write time, is what keeps a moved or deleted repository
/// from ever being a hard error: the next `-g` just sees one fewer repository.
pub fn read_at(path: &Path) -> Vec<PathBuf> {
    let contents = std::fs::read_to_string(path).unwrap_or_default();
    let mut kept = Vec::new();
    let mut dropped = false;
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let candidate = PathBuf::from(line);
        if is_live_main_worktree(&candidate) {
            kept.push(candidate);
        } else {
            dropped = true;
        }
    }
    kept.sort();
    kept.dedup();
    if dropped {
        let _ = write_atomic(path, &kept);
    }
    kept
}

pub fn read() -> Vec<PathBuf> {
    read_at(&registry_path())
}

/// Write `entries` to `path` atomically: a temp file in the same directory, renamed over the
/// target. A crash or a second writer racing this one can then never observe a truncated
/// registry, only the old file or the new one.
fn write_atomic(path: &Path, entries: &[PathBuf]) -> Result<()> {
    let mut buf = Vec::new();
    for entry in entries {
        writeln!(buf, "{}", entry.display())?;
    }
    write_atomic_bytes(path, &buf)
}

/// The mechanism behind [`write_atomic`], generic over the bytes written: a temp file in
/// `path`'s own directory, renamed over the target. `cache.rs` reuses this rather than
/// duplicating it for JSON instead of lines.
pub(crate) fn write_atomic_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path.parent().context("path has no parent")?;
    std::fs::create_dir_all(dir)?;
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    tmp.write_all(bytes)?;
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

/// Register `repo`, idempotently: an already-registered repository rewrites nothing.
pub fn register_at(path: &Path, repo: &Path) -> Result<()> {
    let repo = repo.canonicalize().unwrap_or_else(|_| repo.to_path_buf());
    let mut current = read_at(path);
    if current.contains(&repo) {
        return Ok(());
    }
    current.push(repo);
    current.sort();
    current.dedup();
    write_atomic(path, &current)
}

pub fn register(repo: &Path) -> Result<()> {
    register_at(&registry_path(), repo)
}

static WARNED: Once = Once::new();

/// Register a repository without letting a registry failure fail the command that
/// triggered it — a read-only or unwritable state directory means `-g` is incomplete, not
/// that lane creation breaks. Warns to stderr, at most once per process.
pub fn register_best_effort(repo: &Path) {
    if let Err(error) = register(repo) {
        WARNED.call_once(|| {
            eprintln!("warning: could not update the lane registry: {error:#}");
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::git;

    fn repository() -> tempfile::TempDir {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        for args in [
            &["init", "-qb", "main"][..],
            &["config", "user.email", "t@t.t"],
            &["config", "user.name", "t"],
            &["config", "commit.gpgsign", "false"],
        ] {
            git(args, Some(root)).unwrap();
        }
        temp
    }

    #[test]
    fn registering_twice_writes_once() {
        let repo = repository();
        let state = tempfile::tempdir().unwrap();
        let path = state.path().join("repos");

        register_at(&path, repo.path()).unwrap();
        let first = std::fs::read_to_string(&path).unwrap();
        register_at(&path, repo.path()).unwrap();
        let second = std::fs::read_to_string(&path).unwrap();

        assert_eq!(first, second);
        assert_eq!(read_at(&path).len(), 1);
    }

    #[test]
    fn registering_the_same_repo_from_two_spellings_deduplicates() {
        let repo = repository();
        let state = tempfile::tempdir().unwrap();
        let path = state.path().join("repos");
        let canonical = repo.path().canonicalize().unwrap();

        register_at(&path, repo.path()).unwrap();
        register_at(&path, &canonical).unwrap();

        assert_eq!(read_at(&path), vec![canonical]);
    }

    #[test]
    fn a_write_is_atomic_no_partial_file_is_ever_visible() {
        let repo = repository();
        let state = tempfile::tempdir().unwrap();
        let path = state.path().join("repos");

        register_at(&path, repo.path()).unwrap();

        // The temp file used to write it must not remain in the directory afterwards.
        let leftovers: Vec<_> = std::fs::read_dir(state.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name() != "repos")
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    #[test]
    fn a_repository_deleted_from_disk_is_dropped_on_read() {
        let repo = repository();
        let state = tempfile::tempdir().unwrap();
        let path = state.path().join("repos");
        register_at(&path, repo.path()).unwrap();
        let canonical = repo.path().canonicalize().unwrap();
        drop(repo);
        std::fs::remove_dir_all(&canonical).ok();

        let entries = read_at(&path);

        assert!(entries.is_empty());
        assert!(
            std::fs::read_to_string(&path).unwrap().trim().is_empty(),
            "the stale entry is rewritten out, not just filtered on read"
        );
    }

    #[test]
    fn a_path_that_is_now_a_lane_not_a_main_worktree_is_dropped() {
        let repo = repository();
        let lane = repo.path().join(".lane/trees/spike");
        std::fs::create_dir_all(&lane).unwrap();
        // Not a real linked worktree, just something that is a directory without being the
        // main worktree of a repository: is_live_main_worktree must say no either way.
        let state = tempfile::tempdir().unwrap();
        let path = state.path().join("repos");
        std::fs::write(&path, format!("{}\n", lane.display())).unwrap();

        assert!(read_at(&path).is_empty());
    }
}
