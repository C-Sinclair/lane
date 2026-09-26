//! GitHub references as lane names: `#103`, a pull request URL, or an issue URL, each
//! resolved to the branch it names before the name reaches the rest of lane.
//!
//! Recognising a reference and deriving a branch name from an issue title are pure and
//! unit-tested. Everything that reaches GitHub goes through [`gh`], so a missing CLI or a
//! failed lookup is reported in one place, naming the reference the reader typed.

use anyhow::{Context, Result, bail};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    pub owner: String,
    pub name: String,
}

impl Repo {
    fn slug(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }

    fn same_as(&self, other: &Repo) -> bool {
        self.owner.eq_ignore_ascii_case(&other.owner) && self.name.eq_ignore_ascii_case(&other.name)
    }
}

/// A pull request's `repo` is `None` for `#103`, which always means this repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reference {
    Pull { repo: Option<Repo>, number: u64 },
    Issue { repo: Repo, number: u64 },
}

/// The branch a reference names, and whether that branch is published on origin, so the
/// caller knows fetching it will find something.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub branch: String,
    pub published: bool,
}

/// `#<n>`, `https://github.com/<owner>/<repo>/pull/<n>`, or
/// `https://github.com/<owner>/<repo>/issues/<n>`. The scheme and `www.` are optional, and
/// anything after the number (`/files`, `?query`, `#fragment`) is ignored. Any other text
/// is a plain lane name.
pub fn parse_reference(text: &str) -> Option<Reference> {
    if let Some(digits) = text.strip_prefix('#') {
        return number(digits).map(|number| Reference::Pull { repo: None, number });
    }
    let rest = text
        .strip_prefix("https://")
        .or_else(|| text.strip_prefix("http://"))
        .unwrap_or(text);
    let rest = rest.strip_prefix("www.").unwrap_or(rest);
    let path = rest.strip_prefix("github.com/")?;
    let path = path.split(['?', '#']).next().unwrap_or_default();
    let mut segments = path.split('/');
    let owner = segments.next().filter(|s| !s.is_empty())?;
    let name = segments.next().filter(|s| !s.is_empty())?;
    let kind = segments.next()?;
    let number = number(segments.next()?)?;
    let repo = Repo {
        owner: owner.to_string(),
        name: name.to_string(),
    };
    match kind {
        "pull" => Some(Reference::Pull {
            repo: Some(repo),
            number,
        }),
        "issues" => Some(Reference::Issue { repo, number }),
        _ => None,
    }
}

fn number(digits: &str) -> Option<u64> {
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok().filter(|&n| n > 0)
}

/// The GitHub repository a remote URL points at, in any of the forms git accepts:
/// `git@github.com:o/r.git`, `ssh://git@github.com/o/r.git`, `https://github.com/o/r`.
pub fn repo_of_remote(url: &str) -> Option<Repo> {
    let at = url.find("github.com")?;
    let rest = &url[at + "github.com".len()..];
    let rest = rest.strip_prefix([':', '/'])?;
    let rest = rest.trim_end_matches('/');
    let rest = rest.strip_suffix(".git").unwrap_or(rest);
    let (owner, name) = rest.split_once('/')?;
    if owner.is_empty() || name.is_empty() || name.contains('/') {
        return None;
    }
    Some(Repo {
        owner: owner.to_string(),
        name: name.to_string(),
    })
}

/// The branch GitHub names for an issue when `gh issue develop` is given no name:
/// `<n>-<title>`, lowercased, with every run of other characters collapsed to one `-`.
pub fn issue_branch_name(number: u64, title: &str) -> String {
    let mut slug = String::new();
    for c in title.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-');
    match slug.is_empty() {
        true => number.to_string(),
        false => format!("{number}-{slug}"),
    }
}

/// The first branch in `gh issue develop --list` output, which prints one
/// `<branch>\t<url>` line per linked branch when stdout is not a terminal.
pub fn first_linked_branch(listing: &str) -> Option<String> {
    listing
        .lines()
        .filter_map(|line| line.split('\t').next())
        .map(str::trim)
        .find(|branch| !branch.is_empty())
        .map(String::from)
}

/// Resolve `text` to a branch if it is a GitHub reference, or `None` if it is a plain name.
pub fn resolve(text: &str, root: &Path) -> Result<Option<Resolved>> {
    let Some(reference) = parse_reference(text) else {
        return Ok(None);
    };
    let origin = origin_repo(text, root)?;
    let resolved = match reference {
        Reference::Pull { repo, number } => {
            ensure_origin(text, repo.as_ref(), &origin)?;
            pull_branch(text, &origin, number, root)?
        }
        Reference::Issue { repo, number } => {
            ensure_origin(text, Some(&repo), &origin)?;
            issue_branch(text, &origin, number, root)?
        }
    };
    Ok(Some(resolved))
}

fn origin_repo(reference: &str, root: &Path) -> Result<Repo> {
    // The configured value, not `git remote get-url`: that applies `insteadOf` rewrites,
    // and the rewritten URL may no longer say which GitHub repository origin is.
    let url = crate::git::try_git(&["config", "--get", "remote.origin.url"], Some(root));
    if url.is_empty() {
        bail!("cannot resolve {reference}: this repository has no origin remote");
    }
    repo_of_remote(&url).with_context(|| {
        format!("cannot resolve {reference}: origin ({url}) is not a GitHub repository")
    })
}

fn ensure_origin(reference: &str, named: Option<&Repo>, origin: &Repo) -> Result<()> {
    match named {
        Some(repo) if !repo.same_as(origin) => bail!(
            "{reference} belongs to {}, but this repository's origin is {}",
            repo.slug(),
            origin.slug()
        ),
        _ => Ok(()),
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct PullFields {
    head_ref_name: String,
    #[serde(default)]
    is_cross_repository: bool,
}

fn pull_branch(reference: &str, origin: &Repo, number: u64, root: &Path) -> Result<Resolved> {
    let out = gh(
        reference,
        &[
            "pr",
            "view",
            &number.to_string(),
            "-R",
            &origin.slug(),
            "--json",
            "headRefName,isCrossRepository",
        ],
        root,
    )?;
    let fields: PullFields = serde_json::from_str(&out)
        .with_context(|| format!("gh returned unexpected output for {reference}"))?;
    // A fork's branch is not on origin, so fetching origin would find nothing and the lane
    // would silently start from HEAD under the fork's branch name.
    if fields.is_cross_repository {
        bail!(
            "{reference} comes from a fork; its branch {} is not on origin\n  \
             fetch it yourself (`gh pr checkout {number}`), then run `lane {}`",
            fields.head_ref_name,
            fields.head_ref_name
        );
    }
    Ok(Resolved {
        branch: fields.head_ref_name,
        published: true,
    })
}

fn issue_branch(reference: &str, origin: &Repo, number: u64, root: &Path) -> Result<Resolved> {
    let n = number.to_string();
    let slug = origin.slug();
    let listing = gh(
        reference,
        &["issue", "develop", "--list", &n, "-R", &slug],
        root,
    )?;
    if let Some(branch) = first_linked_branch(&listing) {
        return Ok(Resolved {
            branch,
            published: true,
        });
    }
    let title = gh(
        reference,
        &[
            "issue", "view", &n, "-R", &slug, "--json", "title", "-q", ".title",
        ],
        root,
    )?;
    Ok(Resolved {
        branch: issue_branch_name(number, title.trim()),
        published: false,
    })
}

/// Run `gh`, naming `reference` in any failure.
fn gh(reference: &str, args: &[&str], root: &Path) -> Result<String> {
    let out = match Command::new("gh").args(args).current_dir(root).output() {
        Ok(out) => out,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            bail!("resolving {reference} needs the GitHub CLI (gh), which is not on PATH")
        }
        Err(err) => bail!("resolving {reference}: could not run gh: {err}"),
    };
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        bail!("gh could not resolve {reference}: {}", stderr.trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(owner: &str, name: &str) -> Repo {
        Repo {
            owner: owner.into(),
            name: name.into(),
        }
    }

    #[test]
    fn a_hash_number_is_a_pull_request_in_this_repository() {
        assert_eq!(
            parse_reference("#103"),
            Some(Reference::Pull {
                repo: None,
                number: 103
            })
        );
        for plain in ["#", "#0", "#12a", "#-1", "103", "fix-login", "#fix"] {
            assert_eq!(parse_reference(plain), None, "{plain}");
        }
    }

    #[test]
    fn pull_request_urls_name_their_repository() {
        let want = Some(Reference::Pull {
            repo: Some(repo("acme", "widgets")),
            number: 7,
        });
        for url in [
            "https://github.com/acme/widgets/pull/7",
            "http://github.com/acme/widgets/pull/7",
            "https://www.github.com/acme/widgets/pull/7",
            "github.com/acme/widgets/pull/7",
            "https://github.com/acme/widgets/pull/7/files",
            "https://github.com/acme/widgets/pull/7#issuecomment-1",
            "https://github.com/acme/widgets/pull/7?w=1",
        ] {
            assert_eq!(parse_reference(url), want, "{url}");
        }
    }

    #[test]
    fn issue_urls_are_issues() {
        assert_eq!(
            parse_reference("https://github.com/acme/widgets/issues/42"),
            Some(Reference::Issue {
                repo: repo("acme", "widgets"),
                number: 42
            })
        );
    }

    #[test]
    fn other_github_urls_are_plain_names() {
        for url in [
            "https://github.com/acme/widgets",
            "https://github.com/acme/widgets/pull",
            "https://github.com/acme/widgets/pull/x",
            "https://github.com/acme/widgets/discussions/3",
            "https://gitlab.com/acme/widgets/pull/7",
            "https://github.com//widgets/pull/7",
        ] {
            assert_eq!(parse_reference(url), None, "{url}");
        }
    }

    #[test]
    fn remotes_in_every_form_git_accepts_name_their_repository() {
        for url in [
            "git@github.com:acme/widgets.git",
            "git@github.com:acme/widgets",
            "ssh://git@github.com/acme/widgets.git",
            "https://github.com/acme/widgets.git",
            "https://github.com/acme/widgets/",
            "https://token@github.com/acme/widgets",
        ] {
            assert_eq!(repo_of_remote(url), Some(repo("acme", "widgets")), "{url}");
        }
        for url in ["/srv/origin.git", "https://gitlab.com/acme/widgets.git"] {
            assert_eq!(repo_of_remote(url), None, "{url}");
        }
    }

    #[test]
    fn repositories_compare_without_case() {
        assert!(repo("Acme", "Widgets").same_as(&repo("acme", "widgets")));
        assert!(!repo("acme", "widgets").same_as(&repo("acme", "gadgets")));
    }

    #[test]
    fn an_issue_title_becomes_a_numbered_slug() {
        assert_eq!(
            issue_branch_name(42, "Fix the login bug"),
            "42-fix-the-login-bug"
        );
        assert_eq!(
            issue_branch_name(7, "  [UI] Crash: when   saving (again!) "),
            "7-ui-crash-when-saving-again"
        );
        assert_eq!(issue_branch_name(9, "Use v2.0 API"), "9-use-v2-0-api");
        assert_eq!(issue_branch_name(3, "!!!"), "3");
    }

    #[test]
    fn the_first_linked_branch_is_taken_from_the_listing() {
        assert_eq!(
            first_linked_branch(
                "42-fix-login\thttps://github.com/acme/widgets/tree/42-fix-login\nother\turl\n"
            ),
            Some("42-fix-login".into())
        );
        assert_eq!(first_linked_branch(""), None);
        assert_eq!(first_linked_branch("\n\n"), None);
    }
}
