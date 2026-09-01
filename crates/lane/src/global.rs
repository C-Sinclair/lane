//! `-g`/`--global`: every lane across every repository the registry knows about.

use crate::git::try_git;
use crate::registry;
use crate::worktree::{self as wt, Lane};
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Shared with `cache.rs`, which stores exactly this shape: `--json`'s output must be
/// byte-identical whether a row came from the cache or was just computed, so the cache
/// stores the struct itself rather than re-deriving it from something coarser.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub(crate) struct GlobalRow {
    pub(crate) repo: String,
    /// Absolute, because `repo` is only a directory name: without these a reader of the
    /// JSON can name a lane but cannot act on it.
    pub(crate) repo_path: String,
    pub(crate) lane: String,
    pub(crate) path: String,
    /// Unix timestamp of the lane branch's last commit; AGE in the text table is derived
    /// from this rather than the other way around, so JSON gets the exact value.
    pub(crate) committed_at: i64,
    /// An estimate, not a filesystem extent query: see `disk_estimate` below. Named to say
    /// so, rather than claiming a precision this proxy cannot deliver.
    pub(crate) disk_estimate_bytes: u64,
    pub(crate) ahead: u32,
    pub(crate) behind: u32,
    // Owned, not `&'static str` like `wt::lane_state` returns: a value round-tripped through
    // the cache has no `'static` data to borrow from, only bytes read back off disk.
    pub(crate) state: String,
}

struct Job<'a> {
    repo_root: &'a Path,
    repo_name: &'a str,
    trunk: &'a str,
    lane: &'a Lane,
}

pub fn list_global(json: bool, refresh: bool) -> Result<i32> {
    // `registry::read()` self-heals: it drops (and rewrites out) any repository that has
    // moved or been deleted since the last read. That has to happen on every `-g`, cache hit
    // or not — a repo vanishing from disk is not one of the mutations `cache::invalidate` is
    // wired to, so a warm cache never sees it on its own. It costs nothing worth caching
    // itself: `git::layout` underneath is filesystem-only, no subprocess.
    let repo_roots = registry::read();

    let cached = if refresh {
        None
    } else {
        crate::cache::read_fresh().map(|rows| {
            let live: std::collections::HashSet<String> = repo_roots
                .iter()
                .map(|root| root.to_string_lossy().into_owned())
                .collect();
            rows.into_iter()
                .filter(|row| live.contains(&row.repo_path))
                .collect::<Vec<_>>()
        })
    };
    let rows = match cached {
        Some(rows) => rows,
        None => {
            let rows = compute_rows(&repo_roots);
            crate::cache::write(&rows);
            rows
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(0);
    }
    // An empty registry and registered repositories that simply hold no lanes are different
    // situations, and only the first one is the reader's to act on.
    if rows.is_empty() {
        match repo_roots.len() {
            0 => {
                println!("no repositories registered");
                println!("  lane registers one when you run `lane --init` or create a lane in it");
            }
            1 => println!("no lanes in the 1 registered repository"),
            n => println!("no lanes in any of the {n} registered repositories"),
        }
        return Ok(0);
    }
    for line in format_global_rows(&rows) {
        println!("{line}");
    }
    Ok(0)
}

fn compute_rows(repo_roots: &[PathBuf]) -> Vec<GlobalRow> {
    struct Repo {
        root: PathBuf,
        name: String,
        trunk: String,
        lanes: Vec<Lane>,
    }

    let repos: Vec<Repo> = repo_roots
        .iter()
        .map(|root| {
            let trunk = wt::trunk_name(root);
            let lanes = wt::list_lanes(root);
            let name = root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| root.to_string_lossy().into_owned());
            Repo {
                root: root.clone(),
                name,
                trunk,
                lanes,
            }
        })
        .collect();

    let jobs: Vec<Job> = repos
        .iter()
        .flat_map(|repo| {
            repo.lanes.iter().map(move |lane| Job {
                repo_root: &repo.root,
                repo_name: &repo.name,
                trunk: &repo.trunk,
                lane,
            })
        })
        .collect();

    // The tree walk behind the DISK estimate is what makes a cold `-g` slow, since it
    // crosses build caches; one thread per lane is what `list` already does for dirty
    // status. Warm, this whole pass is dominated by the git subprocesses `build_row` spawns
    // rather than the walk — which is exactly what the cache above exists to skip.
    let mut rows: Vec<GlobalRow> = std::thread::scope(|scope| {
        let workers: Vec<_> = jobs
            .iter()
            .map(|job| scope.spawn(|| build_row(job)))
            .collect();
        workers
            .into_iter()
            .map(|handle| handle.join().expect("global worker panicked"))
            .collect()
    });
    rows.sort_by(|a, b| b.committed_at.cmp(&a.committed_at));
    rows
}

fn build_row(job: &Job) -> GlobalRow {
    let branch = &job.lane.branch;
    let committed_at: i64 = try_git(&["log", "-1", "--format=%ct", branch], Some(job.repo_root))
        .parse()
        .unwrap_or(0);
    let counts = try_git(
        &[
            "rev-list",
            "--left-right",
            "--count",
            &format!("{}...{}", job.trunk, branch),
        ],
        Some(job.repo_root),
    );
    let (behind, ahead) = parse_left_right(&counts);
    let state = wt::lane_state(job.repo_root, job.trunk, job.lane).to_string();
    let disk_estimate_bytes = disk_estimate(&job.lane.path, job.repo_root);

    GlobalRow {
        repo: job.repo_name.to_string(),
        repo_path: job.repo_root.to_string_lossy().into_owned(),
        lane: job.lane.name.clone(),
        path: job.lane.path.to_string_lossy().into_owned(),
        committed_at,
        disk_estimate_bytes,
        ahead,
        behind,
        state,
    }
}

/// An estimate of the storage a lane has stopped sharing with the checkout it was cloned
/// from.
///
/// Neither APFS nor Linux exposes a cheap "bytes unique to this file" query, and `stat`'s
/// `st_blocks` reports a reflinked file's full allocation whether or not its extents are
/// still shared — so `du` cannot answer this either. This instead sums the apparent size of
/// every file in the lane that is not byte-for-byte identical to its counterpart in the main
/// checkout (by size or mtime, not content), plus every file the lane has that the main
/// checkout does not. That approximates unshared storage without ever reading file content.
pub(crate) fn disk_estimate(lane: &Path, main_root: &Path) -> u64 {
    // An empty relative path is the walk's own root, which `filter_entry` also visits:
    // skipping it there prunes the entire walk rather than one entry.
    let skip = |rel: &Path| rel.starts_with(".git") || rel.starts_with(".lane/trees");
    walkdir::WalkDir::new(lane)
        .into_iter()
        .filter_entry(|entry| {
            let rel = entry.path().strip_prefix(lane).unwrap_or(entry.path());
            !skip(rel)
        })
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| {
            let rel = entry.path().strip_prefix(lane).ok()?;
            let meta = entry.metadata().ok()?;
            let size = meta.len();
            let differs = match std::fs::symlink_metadata(main_root.join(rel)) {
                Ok(other) => other.len() != size || other.modified().ok() != meta.modified().ok(),
                Err(_) => true,
            };
            differs.then_some(size)
        })
        .sum()
}

/// `git rev-list --left-right --count trunk...branch` prints "<trunk-only> <branch-only>",
/// i.e. how far the lane is behind trunk, then how far it is ahead.
pub(crate) fn parse_left_right(out: &str) -> (u32, u32) {
    let mut parts = out.split_whitespace();
    let behind = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let ahead = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    (behind, ahead)
}

/// Compact "time since" for AGE: `3h`, `2d`, `3w`, `4mo`, `2y`.
pub(crate) fn format_age(seconds: i64) -> String {
    let seconds = seconds.max(0);
    let minutes = seconds / 60;
    let hours = seconds / 3600;
    let days = seconds / 86400;
    let weeks = days / 7;
    let months = days / 30;
    let years = days / 365;
    if hours < 1 {
        format!("{minutes}m")
    } else if days < 1 {
        format!("{hours}h")
    } else if weeks < 1 {
        format!("{days}d")
    } else if months < 1 {
        format!("{weeks}w")
    } else if years < 1 {
        format!("{months}mo")
    } else {
        format!("{years}y")
    }
}

/// `+3`, `+1 -12`, or empty when the lane and trunk are in sync.
pub(crate) fn format_commits(ahead: u32, behind: u32) -> String {
    match (ahead, behind) {
        (0, 0) => String::new(),
        (ahead, 0) => format!("+{ahead}"),
        (0, behind) => format!("-{behind}"),
        (ahead, behind) => format!("+{ahead} -{behind}"),
    }
}

pub(crate) fn format_bytes(bytes: u64) -> String {
    let mb = bytes as f64 / (1024.0 * 1024.0);
    if mb < 1.0 {
        format!("{} KB", bytes / 1024)
    } else if mb < 1024.0 {
        format!("{mb:.0} MB")
    } else {
        format!("{:.1} GB", mb / 1024.0)
    }
}

pub(crate) fn now() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn format_global_rows(rows: &[GlobalRow]) -> Vec<String> {
    let header = ["REPO", "LANE", "AGE", "DISK", "COMMITS", "STATE"];
    let cells: Vec<[String; 6]> = rows
        .iter()
        .map(|row| {
            [
                row.repo.clone(),
                row.lane.clone(),
                format_age(now() - row.committed_at),
                format_bytes(row.disk_estimate_bytes),
                format_commits(row.ahead, row.behind),
                row.state.to_string(),
            ]
        })
        .collect();

    // Same technique `format_lane_rows` uses for the plain listing: each column widens to
    // its longest entry, header included, and only the last column is left ragged.
    let widths: Vec<usize> = (0..header.len())
        .map(|i| {
            cells
                .iter()
                .map(|row| row[i].chars().count())
                .chain(std::iter::once(header[i].chars().count()))
                .max()
                .unwrap_or(0)
        })
        .collect();

    let render = |cols: &[String; 6]| -> String {
        cols.iter()
            .enumerate()
            .map(|(i, cell)| {
                if i + 1 == cols.len() {
                    cell.clone()
                } else {
                    format!("{cell:<width$}", width = widths[i])
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
            .trim_end()
            .to_string()
    };

    let mut out = vec![render(&header.map(str::to_string))];
    out.extend(cells.iter().map(render));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn age_formats_across_every_boundary() {
        assert_eq!(format_age(0), "0m");
        assert_eq!(format_age(59 * 60), "59m");
        assert_eq!(format_age(60 * 60), "1h");
        assert_eq!(format_age(23 * 3600 + 59 * 60), "23h");
        assert_eq!(format_age(24 * 3600), "1d");
        assert_eq!(format_age(6 * 86400), "6d");
        assert_eq!(format_age(7 * 86400), "1w");
        assert_eq!(format_age(29 * 86400), "4w");
        assert_eq!(format_age(30 * 86400), "1mo");
        assert_eq!(format_age(364 * 86400), "12mo");
        assert_eq!(format_age(365 * 86400), "1y");
        assert_eq!(format_age(2 * 365 * 86400), "2y");
    }

    #[test]
    fn commits_formats_ahead_behind_and_in_sync() {
        assert_eq!(format_commits(3, 0), "+3");
        assert_eq!(format_commits(1, 12), "+1 -12");
        assert_eq!(format_commits(0, 5), "-5");
        assert_eq!(format_commits(0, 0), "");
    }

    /// The proxy returned zero for every lane once, because `filter_entry` visits the
    /// walk's own root and rejecting it there prunes everything below it.
    #[test]
    fn disk_estimate_counts_only_what_diverges_from_the_main_tree() {
        let temp = tempfile::tempdir().unwrap();
        let main_root = temp.path().join("main");
        let lane = temp.path().join("lane");
        std::fs::create_dir_all(main_root.join("target")).unwrap();
        std::fs::create_dir_all(lane.join("target")).unwrap();

        // Byte-identical in both, and copied so the mtime matches: shared, so not counted.
        std::fs::write(main_root.join("target/shared.bin"), vec![7u8; 4096]).unwrap();
        std::fs::copy(
            main_root.join("target/shared.bin"),
            lane.join("target/shared.bin"),
        )
        .unwrap();
        assert_eq!(disk_estimate(&lane, &main_root), 0);

        // Present only in the lane: counted in full.
        std::fs::write(lane.join("target/built.bin"), vec![1u8; 2048]).unwrap();
        assert_eq!(disk_estimate(&lane, &main_root), 2048);

        // Differing size at the same path: counted at the lane's size.
        std::fs::write(lane.join("target/shared.bin"), vec![7u8; 5000]).unwrap();
        assert_eq!(disk_estimate(&lane, &main_root), 2048 + 5000);

        // A nested lane's own tree is never walked into.
        std::fs::create_dir_all(lane.join(".lane/trees/inner")).unwrap();
        std::fs::write(lane.join(".lane/trees/inner/huge.bin"), vec![0u8; 9999]).unwrap();
        assert_eq!(disk_estimate(&lane, &main_root), 2048 + 5000);
    }

    #[test]
    fn left_right_counts_are_read_as_behind_then_ahead() {
        assert_eq!(parse_left_right("12\t1\n"), (12, 1));
        assert_eq!(parse_left_right(""), (0, 0));
    }
}
