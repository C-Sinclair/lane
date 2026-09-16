//! Command surface. Every command returns an exit code; failures bubble as errors.

use crate::args::{self, Parsed};
use crate::git::{self, try_git};
use crate::worktree as wt;
use crate::worktree::LANE_DIR;
use anyhow::{Result, bail};
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};

/// Takes the stream it will be written to; for `new` that is stderr, not stdout.
fn bold(text: &str, tty: bool) -> String {
    if tty {
        format!("\x1b[1m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

pub fn run() -> Result<i32> {
    // A usage error is the reader's, not the program's: it exits 2, the way a
    // command line has always distinguished "you typed it wrong" from "it failed".
    let parsed = match args::parse(std::env::args_os().skip(1).collect()) {
        Ok(parsed) => parsed,
        Err(err) => {
            eprintln!("error: {err:#}");
            return Ok(2);
        }
    };

    match parsed {
        Parsed::Help(topic) => {
            println!("{}", topic.text());
            Ok(0)
        }
        Parsed::Version => {
            println!("lane {}", args::VERSION);
            Ok(0)
        }
        Parsed::Init => init(),
        Parsed::Open(args) => open(&args.name, args.base.as_deref(), args.dirty),
        Parsed::List {
            json,
            global,
            refresh,
            disk,
        } => list(json, global, refresh, disk),
        Parsed::Delete(args) => delete(&args.names, args.force),
        Parsed::Info { name, json } => crate::info::show(name.as_deref(), json),
        Parsed::Exit => exit(),
        Parsed::Prune { dry_run } => prune(dry_run),
        Parsed::Shellenv(shell) => shellenv(shell),
        Parsed::Completions(shell) => completions(shell),
    }
}

fn init() -> Result<i32> {
    let root = wt::main_root()?;
    let lane = root.join(LANE_DIR);
    std::fs::create_dir_all(&lane)?;
    std::fs::write(lane.join(".gitkeep"), "")?;

    crate::registry::register_best_effort(&root);
    // A newly-registered repository (or one initialized a second time) changes what `-g`
    // ought to show; see cache.rs for why membership is invalidated outright rather than
    // left to the TTL.
    crate::cache::invalidate();
    let (ok, detail) = crate::cow::probe(&root);
    println!("initialized .lane/");
    println!(
        "reflink on this filesystem: {} ({detail})",
        if ok { "yes" } else { "no" }
    );
    if !ok {
        println!("  lanes will still work as plain worktrees; ignored files will not be cloned");
    }
    Ok(0)
}

fn new(name: &str, base: Option<&str>, dirty: bool) -> Result<i32> {
    let created = wt::create(name, base, dirty)?;
    // Progress goes to stderr so stdout carries the path alone, as `enter` does. Bold only
    // for a terminal: a captured path must not carry escapes.
    let info = &mut std::io::stderr();
    for note in &created.notes {
        writeln!(info, "  {note}")?;
    }
    writeln!(info, "  {}", created.stats)?;
    let tty = std::io::stdout().is_terminal();
    println!("{}", bold(&created.path.to_string_lossy(), tty));
    Ok(0)
}

/// Flags that only make sense while a lane is being created, given the reason no lane is
/// being created this time.
fn reject_create_flags(reason: &str, base: Option<&str>, dirty: bool) -> Result<()> {
    let mut conflicts = Vec::new();
    if base.is_some() {
        conflicts.push("--base");
    }
    if dirty {
        conflicts.push("--dirty");
    }
    if conflicts.is_empty() {
        return Ok(());
    }
    let verb = if conflicts.len() == 1 { "applies" } else { "apply" };
    bail!(
        "{reason}; {} only {verb} when creating a lane",
        conflicts.join(" and ")
    )
}

/// `lane <name>`: enter it if it exists, otherwise create it as before.
fn open(name: &str, base: Option<&str>, dirty: bool) -> Result<i32> {
    let root = wt::main_root()?;
    if lane_named(&root, name).is_ok() {
        reject_create_flags(&format!("lane {name} already exists"), base, dirty)?;
        return enter(name);
    }
    // git refuses a second working tree on the same branch, and a branch checked out in
    // the main tree or in another tool's worktree is the common way to hit that. The
    // caller asked to work on this branch, and that checkout is where the branch is, so
    // take them there and say where they landed.
    if let Some(path) = wt::checkout_holding(&root, name) {
        reject_create_flags(&format!("branch {name} is checked out already"), base, dirty)?;
        let canonical = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        let place = if canonical(&path) == canonical(&root) {
            "the main worktree".to_string()
        } else if let Ok(lane) = path.strip_prefix(wt::lanes_dir(&root)) {
            format!("lane {}", lane.display())
        } else {
            format!("the worktree at {}", path.display())
        };
        eprintln!("note: branch {name} is checked out in {place}; moving you there");
        return move_to(&path);
    }
    new(name, base, dirty)
}

#[derive(serde::Serialize)]
struct LaneRow {
    name: String,
    path: String,
    branch: String,
    state: &'static str,
    dirty: bool,
}

fn format_lane_rows(rows: &[LaneRow]) -> Vec<String> {
    let name_width = rows
        .iter()
        .map(|row| row.name.chars().count())
        .max()
        .unwrap_or(20)
        .max(20);
    rows.iter()
        .map(|row| {
            let name = &row.name;
            let state = row.state;
            let dirty = if row.dirty { "dirty" } else { "clean" };
            format!("{name:<name_width$} {state:<7} {dirty}")
        })
        .collect()
}

fn list(json: bool, global: bool, refresh: bool, disk: bool) -> Result<i32> {
    if global {
        return crate::global::list_global(json, refresh, disk);
    }
    let root = wt::main_root()?;
    let lanes = wt::list_lanes(&root);
    let dirty: Vec<bool> = std::thread::scope(|scope| {
        let workers: Vec<_> = lanes
            .iter()
            .map(|lane| scope.spawn(|| wt::is_dirty(&lane.path)))
            .collect();
        workers
            .into_iter()
            .map(|handle| handle.join().expect("status worker panicked"))
            .collect()
    });
    let trunk = wt::trunk_name(&root);
    let rows: Vec<_> = lanes
        .into_iter()
        .zip(dirty)
        .map(|(lane, dirty)| {
            let name = lane.name.clone();
            let state = wt::lane_state(&root, &trunk, &lane);
            LaneRow {
                name,
                path: lane.path.to_string_lossy().to_string(),
                branch: lane.branch,
                state,
                dirty,
            }
        })
        .collect();

    if json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(0);
    }
    if rows.is_empty() {
        println!("no lanes");
        return Ok(0);
    }
    for row in format_lane_rows(&rows) {
        println!("{row}");
    }
    Ok(0)
}

fn prune(dry_run: bool) -> Result<i32> {
    let root = wt::main_root()?;
    // A retired upstream is read from a cache that only empties on a prune. Failing here
    // costs accuracy, never the command: fall through and decide on the refs we have.
    if !try_git(&["remote"], Some(&root)).trim().is_empty()
        && !git::git_ok(&["fetch", "--prune", "--quiet"], Some(&root))
    {
        eprintln!("warning: fetch failed; deciding on cached remote refs");
    }
    let trunk = wt::trunk_name(&root);
    let lanes = wt::list_lanes(&root);

    let mut removed = 0;
    let mut skipped = 0;
    let mut stranded = false;
    for lane in lanes {
        let name = lane.name.clone();
        if !wt::landed(&root, &trunk, &lane.branch) {
            continue;
        }
        let losses = wt::losses(&root, &lane.path, &lane.branch, &trunk);
        if !losses.is_empty() {
            eprintln!("skipped {name}: {}", losses.join(", "));
            skipped += 1;
            continue;
        }
        if dry_run {
            println!("would remove {name}");
            removed += 1;
            continue;
        }
        if wt::remove(&name)? {
            stranded = true;
        }
        println!("removed {name}");
        removed += 1;
    }

    // Unlike `-d`, prune's stdout is its report, so it has nowhere to put a destination
    // for the shell function to read. Say so instead.
    if stranded {
        eprintln!("note: the lane you were standing in is gone; run `lane -e`");
    }

    if removed == 0 && skipped == 0 {
        println!("no landed lanes");
        let behind = try_git(
            &["rev-list", "--count", &format!("{trunk}..origin/{trunk}")],
            Some(&root),
        );
        if behind.parse::<u32>().unwrap_or(0) > 0 {
            println!("  origin/{trunk} is {behind} commit(s) ahead; fetch and merge it first");
        }
    }
    Ok(i32::from(skipped > 0))
}

pub(crate) fn lane_named(root: &Path, name: &str) -> Result<PathBuf> {
    let dest = wt::lanes_dir(root).join(name);
    if !dest.exists() {
        bail!("no lane named {name}");
    }
    Ok(dest)
}

/// Move the shell, or say why it did not.
///
/// A terminal on stdout means nothing captured the destination, so no shell function ran.
fn move_to(dest: &Path) -> Result<i32> {
    println!("{}", dest.display());
    if std::io::stdout().is_terminal() {
        eprintln!(
            "note: no shell integration; add `eval \"$(lane --shellenv)\"` to cd automatically"
        );
    }
    Ok(0)
}

fn enter(name: &str) -> Result<i32> {
    move_to(&lane_named(&wt::main_root()?, name)?)
}

fn exit() -> Result<i32> {
    move_to(&wt::main_root()?)
}

/// The report goes to stderr so stdout is free to carry a destination, the way `new` and
/// `enter` already split them. Returns the exit code and whether the removal moved this
/// process out of the lane the caller was standing in.
fn delete_one(name: &str, force: bool) -> Result<(i32, bool)> {
    let root = wt::main_root()?;
    if !force {
        let trunk = wt::trunk_name(&root);
        let path = wt::lanes_dir(&root).join(name);
        let losses = wt::losses(&root, &path, name, &trunk);
        if !losses.is_empty() {
            eprintln!("kept lane {name}: {}", losses.join(", "));
            eprintln!("  lane -D {name}   to discard it anyway");
            return Ok((1, false));
        }
    }
    let inside = wt::remove(name)?;
    eprintln!("removed lane {name}");
    Ok((0, inside))
}

/// `-d`/`-D`: process each name in order, reporting per name; exit non-zero if
/// any was kept.
///
/// Deleting the lane you are standing in is allowed, and prints the main root on stdout so
/// the shell function moves you there. Every other delete prints nothing on stdout, which
/// is what tells that function to leave the shell where it is.
fn delete(names: &[String], force: bool) -> Result<i32> {
    let mut kept = false;
    let mut stranded = false;
    for name in names {
        let (code, inside) = delete_one(name, force)?;
        if code != 0 {
            kept = true;
        }
        if inside {
            stranded = true;
        }
    }
    if stranded {
        return move_to(&wt::main_root()?).map(|_| i32::from(kept));
    }
    Ok(i32::from(kept))
}

// cd for a bare name, for `--exit`, and for a `-d`/`-D` that removed the lane the shell
// was standing in — that one prints a destination only in that case, so an empty capture
// means stay put. Every other flag, and no args at all, must not cd.
fn shellenv(shell: args::Shell) -> Result<i32> {
    match shell {
        args::Shell::Fish => println!(
            r#"function lane
  switch "$argv[1]"
    case '-e' '--exit'
      set -l p (command lane $argv); or return
      cd $p
    case '-d' '-D' '--delete' '--force-delete'
      set -l p (command lane $argv)
      set -l code $status
      test -n "$p"; and cd $p
      return $code
    case '' '-*'
      command lane $argv
    case '*'
      set -l p (command lane $argv); or return
      cd $p
  end
end"#
        ),
        args::Shell::Bash | args::Shell::Zsh | args::Shell::Posix => println!(
            r#"lane() {{
  local p
  case "$1" in
    -e|--exit) p=$(command lane "$@") || return; cd "$p" ;;
    -d|-D|--delete|--force-delete)
      p=$(command lane "$@"); local code=$?
      [ -n "$p" ] && cd "$p"
      return $code ;;
    ""|-*)  command lane "$@" ;;
    *)      p=$(command lane "$@") || return; cd "$p" ;;
  esac
}}"#
        ),
    }
    Ok(0)
}

fn completions(shell: args::Shell) -> Result<i32> {
    match shell {
        args::Shell::Fish => println!(
            r#"complete -c lane -f
complete -c lane -s h -l help
complete -c lane -s V -l version
complete -c lane -s l -l list
complete -c lane -s g -l global
complete -c lane -l json
complete -c lane -l base -x
complete -c lane -l dirty
complete -c lane -s d -l delete
complete -c lane -s D -l force-delete
complete -c lane -s i -l info
complete -c lane -l prune
complete -c lane -l dry-run
complete -c lane -l refresh
complete -c lane -l disk
complete -c lane -l init
complete -c lane -s e -l exit
complete -c lane -l shellenv -xa 'fish bash zsh posix'
complete -c lane -l completions -xa 'fish bash zsh'
complete -c lane -n '__fish_is_first_arg' -a "(command lane 2>/dev/null | awk '{{print \$1}}')"
complete -c lane -n '__fish_seen_argument -s d -s D -s i' -a "(command lane 2>/dev/null | awk '{{print \$1}}')""#
        ),
        args::Shell::Bash => println!(
            r#"_lane() {{
  local cur prev
  cur="${{COMP_WORDS[COMP_CWORD]}}"
  prev="${{COMP_WORDS[COMP_CWORD-1]}}"
  lanes() {{ command lane 2>/dev/null | awk '{{print $1}}'; }}

  case "$prev" in
    --shellenv) COMPREPLY=($(compgen -W "fish bash zsh posix" -- "$cur")); return ;;
    --completions) COMPREPLY=($(compgen -W "fish bash zsh" -- "$cur")); return ;;
    --base) COMPREPLY=(); return ;;
  esac

  for w in "${{COMP_WORDS[@]}}"; do
    case "$w" in
      -d|--delete|-D|--force-delete|-i|--info)
        COMPREPLY=($(compgen -W "$(lanes)" -- "$cur")); return ;;
    esac
  done

  COMPREPLY=($(compgen -W "$(lanes) -l --list -g --global -i --info --json --base --dirty -d --delete -D --force-delete --prune --dry-run --refresh --disk --init -e --exit --shellenv --completions -h --help -V --version" -- "$cur"))
}}
complete -F _lane lane"#,
        ),
        args::Shell::Zsh => println!(
            r#"#compdef lane

_lane() {{
  local -a lanes flags
  lanes=(${{(f)"$(command lane 2>/dev/null | awk '{{print $1}}')"}})
  flags=(-l --list -g --global -i --info --json --base --dirty -d --delete -D --force-delete --prune --dry-run --refresh --disk --init -e --exit --shellenv --completions -h --help -V --version)

  case "${{words[CURRENT-1]}}" in
    --shellenv) compadd -- fish bash zsh posix; return ;;
    --completions) compadd -- fish bash zsh; return ;;
    --base) return ;;
  esac

  for w in "${{words[@]}}"; do
    case "$w" in
      -d|--delete|-D|--force-delete|-i|--info) compadd -a lanes; return ;;
    esac
  done

  compadd -a lanes
  compadd -a flags
}}

_lane "$@""#,
        ),
        args::Shell::Posix => bail!("no completion script for posix"),
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_lane_names_keep_following_columns_aligned() {
        let row = |name: &str, state| LaneRow {
            name: name.into(),
            path: String::new(),
            branch: name.into(),
            state,
            dirty: false,
        };
        let rows = [
            row("agent-runtime-config", "pushed"),
            row("inline-agent-instructions", "open"),
        ];

        assert_eq!(
            format_lane_rows(&rows),
            [
                "agent-runtime-config      pushed  clean",
                "inline-agent-instructions open    clean",
            ]
        );
    }

    fn repository() -> tempfile::TempDir {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        for args in [
            &["init", "-qb", "main"][..],
            &["config", "user.email", "t@t.t"],
            &["config", "user.name", "t"],
            // The author's own signing config would otherwise reach in and block on a key.
            &["config", "commit.gpgsign", "false"],
        ] {
            git::git(args, Some(root)).unwrap();
        }
        std::fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();
        git::git(&["add", "-A"], Some(root)).unwrap();
        git::git(&["commit", "-qm", "base"], Some(root)).unwrap();
        temp
    }

    #[test]
    fn a_directory_git_does_not_know_is_not_a_lane() {
        let temp = repository();
        let root = temp.path();

        assert!(
            lane_named(root, "ghost")
                .unwrap_err()
                .to_string()
                .contains("no lane named ghost")
        );
    }
}
