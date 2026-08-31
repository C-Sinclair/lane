//! `-i`/`--info`: everything there is to know about one lane.
//!
//! Unlike `list` and `-g`, which describe many lanes in a few columns, this
//! describes one lane in full. Every value it prints is computed here from
//! git and the filesystem rather than cached anywhere, so it is always
//! current at the cost of a handful of extra git invocations.

use crate::git::try_git;
use crate::global;
use crate::worktree::{self as wt, Lane};
use anyhow::{Result, bail};
use std::path::Path;
use std::time::UNIX_EPOCH;

#[derive(serde::Serialize)]
struct InfoJson {
    lane: String,
    branch: String,
    path: String,
    state: &'static str,
    /// Unix timestamp of the worktree directory's filesystem creation time (see
    /// `created_time` below), not a stored value: lane keeps no record of it.
    created_at: i64,
    fork_point: Option<String>,
    fork_subject: Option<String>,
    fork_committed_at: Option<i64>,
    trunk: String,
    trunk_ahead: u32,
    trunk_behind: u32,
    /// Named to agree with `GlobalRow::committed_at`: the lane branch's last commit.
    committed_at: i64,
    last_commit: String,
    last_commit_subject: String,
    upstream: Option<String>,
    upstream_ahead: Option<u32>,
    upstream_behind: Option<u32>,
    uncommitted: u32,
    disk_estimate_bytes: u64,
}

pub fn show(name: Option<&str>, json: bool) -> Result<i32> {
    let layout = crate::git::layout(&std::env::current_dir()?)?;
    let root = layout.main_root;
    let name = match name {
        Some(name) => name.to_string(),
        None if layout.repo_root == root => {
            bail!("not inside a lane; name one: `lane -i <name>`");
        }
        None => wt::name_of(&root, &layout.repo_root),
    };

    let path = crate::cli::lane_named(&root, &name)?;
    let branch = wt::list_lanes(&root)
        .into_iter()
        .find(|lane| lane.name == name)
        .map_or_else(|| name.clone(), |lane| lane.branch);

    let trunk = wt::trunk_name(&root);
    let lane = Lane {
        path: path.clone(),
        branch: branch.clone(),
        name: name.clone(),
    };
    let state = wt::lane_state(&root, &trunk, &lane);
    let now = global::now();

    let created_at = created_time(&path);

    let fork_sha = wt::fork_point(&root, &branch);
    let fork = fork_sha.as_deref().and_then(|sha| commit_info(&root, sha));

    let counts = try_git(
        &[
            "rev-list",
            "--left-right",
            "--count",
            &format!("{trunk}...{branch}"),
        ],
        Some(&root),
    );
    let (trunk_behind, trunk_ahead) = global::parse_left_right(&counts);

    let last = commit_info(&root, &branch);

    let upstream = try_git(
        &[
            "for-each-ref",
            "--format=%(upstream:short)",
            &format!("refs/heads/{branch}"),
        ],
        Some(&root),
    );
    let upstream = (!upstream.is_empty()).then_some(upstream);
    let upstream_counts = upstream.as_ref().map(|upstream| {
        let counts = try_git(
            &[
                "rev-list",
                "--left-right",
                "--count",
                &format!("{upstream}...{branch}"),
            ],
            Some(&root),
        );
        global::parse_left_right(&counts)
    });

    let uncommitted = try_git(&["status", "--porcelain"], Some(&path))
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count() as u32;

    let disk = global::disk_estimate(&path, &root);

    if json {
        let (upstream_behind, upstream_ahead) = upstream_counts.unwrap_or((0, 0));
        let payload = InfoJson {
            lane: name,
            branch,
            path: path.to_string_lossy().into_owned(),
            state,
            created_at,
            fork_point: fork.as_ref().map(|(sha, ..)| sha.clone()),
            fork_subject: fork.as_ref().map(|(_, subject, _)| subject.clone()),
            fork_committed_at: fork.as_ref().map(|(.., at)| *at),
            trunk,
            trunk_ahead,
            trunk_behind,
            committed_at: last.as_ref().map_or(0, |(.., at)| *at),
            last_commit: last
                .as_ref()
                .map_or_else(String::new, |(sha, ..)| sha.clone()),
            last_commit_subject: last
                .as_ref()
                .map_or_else(String::new, |(_, subject, _)| subject.clone()),
            upstream: upstream.clone(),
            upstream_ahead: upstream_counts.map(|_| upstream_ahead),
            upstream_behind: upstream_counts.map(|_| upstream_behind),
            uncommitted,
            disk_estimate_bytes: disk,
        };
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(0);
    }

    let created = format!("{} ago", global::format_age(now - created_at));
    let forked_from = fork.as_ref().map_or_else(
        || "(not recorded)".to_string(),
        |(sha, subject, at)| commit_summary(sha, subject, *at, now),
    );
    let trunk_row = format_divergence(&trunk, trunk_ahead, trunk_behind);
    let last_commit_row = last
        .as_ref()
        .map_or_else(String::new, |(sha, subject, at)| {
            commit_summary(sha, subject, *at, now)
        });
    let upstream_row = match (&upstream, upstream_counts) {
        (Some(upstream), Some((behind, ahead))) => format_divergence(upstream, ahead, behind),
        _ => "none".to_string(),
    };

    let rows: Vec<(&str, String)> = vec![
        ("lane", name),
        ("branch", branch),
        ("path", path.to_string_lossy().into_owned()),
        ("state", state.to_string()),
        ("created", created),
        ("forked from", forked_from),
        ("trunk", trunk_row),
        ("last commit", last_commit_row),
        ("upstream", upstream_row),
        ("uncommitted", format_uncommitted(uncommitted)),
        ("disk", format_disk(disk)),
    ];
    for line in render_rows(&rows) {
        println!("{line}");
    }
    Ok(0)
}

/// The lane's own age, since lane keeps no record of when it was made: a lane that adopted
/// an existing branch would otherwise report the branch's history rather than its own.
fn created_time(path: &Path) -> i64 {
    let created =
        std::fs::metadata(path).and_then(|meta| meta.created().or_else(|_| meta.modified()));
    created
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

/// Short sha, subject, and commit time for one revision.
fn commit_info(root: &Path, rev: &str) -> Option<(String, String, i64)> {
    let out = try_git(&["log", "-1", "--format=%h%n%s%n%ct", rev], Some(root));
    let mut lines = out.lines();
    let sha = lines.next()?.to_string();
    let subject = lines.next()?.to_string();
    let committed_at: i64 = lines.next()?.parse().ok()?;
    Some((sha, subject, committed_at))
}

fn commit_summary(sha: &str, subject: &str, committed_at: i64, now: i64) -> String {
    format!(
        "{sha} {subject} ({} ago)",
        global::format_age(now - committed_at)
    )
}

/// `main`, or `main +3 -2` when the lane and its counterpart have diverged.
fn format_divergence(label: &str, ahead: u32, behind: u32) -> String {
    let commits = global::format_commits(ahead, behind);
    if commits.is_empty() {
        label.to_string()
    } else {
        format!("{label} {commits}")
    }
}

fn format_uncommitted(count: u32) -> String {
    match count {
        0 => "clean".to_string(),
        1 => "1 file".to_string(),
        n => format!("{n} files"),
    }
}

fn format_disk(bytes: u64) -> String {
    format!(
        "{} (estimate: unshared vs the main checkout)",
        global::format_bytes(bytes)
    )
}

/// Keys left, values right-padded to one column: the key width is computed from the
/// widest key rather than hardcoded, so a row added later cannot silently misalign the
/// rest.
fn render_rows(rows: &[(&str, String)]) -> Vec<String> {
    let key_width = rows
        .iter()
        .map(|(key, _)| key.chars().count())
        .max()
        .unwrap_or(0);
    rows.iter()
        .map(|(key, value)| format!("{key:<key_width$}  {value}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_align_on_the_widest_key() {
        let rows: Vec<(&str, String)> = vec![
            ("lane", "feat/login".to_string()),
            ("forked from", "a1b2c3d Add the parser (3d ago)".to_string()),
        ];

        assert_eq!(
            render_rows(&rows),
            [
                "lane         feat/login",
                "forked from  a1b2c3d Add the parser (3d ago)",
            ]
        );
    }

    #[test]
    fn uncommitted_counts_pluralize() {
        assert_eq!(format_uncommitted(0), "clean");
        assert_eq!(format_uncommitted(1), "1 file");
        assert_eq!(format_uncommitted(2), "2 files");
    }

    #[test]
    fn divergence_omits_the_count_when_in_sync() {
        assert_eq!(format_divergence("main", 0, 0), "main");
        assert_eq!(format_divergence("main", 3, 2), "main +3 -2");
        assert_eq!(
            format_divergence("origin/feat/login", 1, 0),
            "origin/feat/login +1"
        );
    }

    #[test]
    fn commit_summary_reads_sha_subject_and_age() {
        let now = 10_000;
        let hour = 3_600;
        assert_eq!(
            commit_summary("4e5f6a7", "Fix the lexer", now - 2 * hour, now),
            "4e5f6a7 Fix the lexer (2h ago)"
        );
    }
}
