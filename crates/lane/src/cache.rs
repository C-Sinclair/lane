//! The `-g` snapshot cache: a repeat lookup's whole row set, so it can skip the git
//! subprocesses that dominate a warm `-g`'s cost (see ADR-013 for the measurements).
//!
//! Freshness has two, deliberately separate, mechanisms:
//!
//! - **Exact invalidation for membership.** Lane performs every mutation that changes which
//!   lanes exist, so it knows precisely when to drop this file: [`invalidate`] is called on
//!   lane creation, on `-d`/`-D` deletion, on `--prune` removing anything, and on `--init`.
//!   A lane just made or removed is therefore never missing from, or stale in, `-g` — no
//!   matter what the TTL below says.
//! - **A TTL for the derived numbers** (age, disk estimate, ahead/behind, state), which
//!   drift for reasons lane does not observe: a commit made inside a lane, a build that grew
//!   the tree, a branch that landed elsewhere. Lane cannot know about those as they happen,
//!   so [`TTL_SECS`] bounds how stale they are allowed to get instead.
//!
//! A cache file that cannot be read, is malformed, or is outside the TTL is a miss, never an
//! error: `-g` falls through to computing fresh rows exactly as it would with no cache at
//! all. The same holds for a cache that cannot be written — `-g` must still print.

use crate::global::{GlobalRow, now};
use crate::registry;
use std::path::{Path, PathBuf};

/// How long the derived columns may lag reality. Membership does not rely on this — see the
/// module doc — so this only ever costs `-g` a stale AGE/DISK/COMMITS/STATE, never a missing
/// or phantom lane.
pub(crate) const TTL_SECS: i64 = 120;

#[derive(serde::Serialize, serde::Deserialize)]
struct CacheFile {
    generated_at: i64,
    rows: Vec<GlobalRow>,
}

fn cache_path() -> PathBuf {
    registry::state_dir().join("cache.json")
}

/// The cached rows, if the file exists, parses as the shape `-g` expects, and is still
/// within the TTL. Anything else — absent, unreadable, malformed, an old shape, expired —
/// comes back `None` rather than an error.
pub(crate) fn read_fresh() -> Option<Vec<GlobalRow>> {
    read_fresh_at(&cache_path(), now())
}

fn read_fresh_at(path: &Path, now: i64) -> Option<Vec<GlobalRow>> {
    let bytes = std::fs::read(path).ok()?;
    let file: CacheFile = serde_json::from_slice(&bytes).ok()?;
    if now - file.generated_at > TTL_SECS {
        return None;
    }
    Some(file.rows)
}

/// Rewrite the cache with `rows`, best-effort. A write failure here is swallowed: it costs
/// the next `-g` a cache hit, not this one its output.
pub(crate) fn write(rows: &[GlobalRow]) {
    let file = CacheFile {
        generated_at: now(),
        rows: rows.to_vec(),
    };
    if let Ok(bytes) = serde_json::to_vec(&file) {
        let _ = registry::write_atomic_bytes(&cache_path(), &bytes);
    }
}

/// Drop the cache so the next `-g` recomputes from scratch. Called everywhere lane changes
/// which lanes exist. Best-effort: a file that fails to delete just outlives its TTL like
/// any other stale cache, and a reader can always delete it by hand.
pub(crate) fn invalidate() {
    let _ = std::fs::remove_file(cache_path());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(lane: &str) -> GlobalRow {
        GlobalRow {
            repo: "repo".into(),
            repo_path: "/repo".into(),
            lane: lane.into(),
            path: format!("/repo/.lane/trees/{lane}"),
            committed_at: 1000,
            disk_estimate_bytes: 4096,
            ahead: 1,
            behind: 0,
            state: "open".into(),
        }
    }

    #[test]
    fn a_write_round_trips_through_a_fresh_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.json");
        let rows = vec![row("a"), row("b")];
        let bytes = serde_json::to_vec(&CacheFile {
            generated_at: 500,
            rows: rows.clone(),
        })
        .unwrap();
        registry::write_atomic_bytes(&path, &bytes).unwrap();

        let read = read_fresh_at(&path, 500 + TTL_SECS).unwrap();
        assert_eq!(
            serde_json::to_string(&read).unwrap(),
            serde_json::to_string(&rows).unwrap()
        );
    }

    #[test]
    fn a_missing_file_is_a_miss() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_fresh_at(&dir.path().join("nope.json"), now()).is_none());
    }

    #[test]
    fn a_truncated_file_is_a_miss() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.json");
        std::fs::write(&path, br#"{"generated_at": 1, "rows": [{"repo": "r""#).unwrap();
        assert!(read_fresh_at(&path, now()).is_none());
    }

    #[test]
    fn a_file_with_the_wrong_shape_is_a_miss() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.json");
        std::fs::write(&path, br#"{"totally": "unexpected"}"#).unwrap();
        assert!(read_fresh_at(&path, now()).is_none());
    }

    #[test]
    fn just_inside_the_ttl_is_fresh_one_second_past_is_not() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.json");
        let bytes = serde_json::to_vec(&CacheFile {
            generated_at: 1000,
            rows: vec![row("a")],
        })
        .unwrap();
        registry::write_atomic_bytes(&path, &bytes).unwrap();

        assert!(read_fresh_at(&path, 1000 + TTL_SECS).is_some());
        assert!(read_fresh_at(&path, 1000 + TTL_SECS + 1).is_none());
    }

    #[test]
    fn writing_leaves_no_temp_file_behind() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lane/cache.json");
        let bytes = serde_json::to_vec(&CacheFile {
            generated_at: 1,
            rows: vec![row("a")],
        })
        .unwrap();
        registry::write_atomic_bytes(&path, &bytes).unwrap();

        let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name() != "cache.json")
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }
}
