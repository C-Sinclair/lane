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
        Parsed::New(args) => new(&args.name, args.base.as_deref(), args.dirty),
        Parsed::Open(args) => open(&args.name, args.base.as_deref(), args.dirty),
        Parsed::Ls { json } => ls(json),
        Parsed::Enter { name } => enter(&name),
        Parsed::Exit => exit(),
        Parsed::Prune { dry_run } => prune(dry_run),
        Parsed::Rm(args) => rm(&args.name, args.force),
        Parsed::Shellenv(shell) => shellenv(shell),
        Parsed::Completions(shell) => completions(shell),
    }
}

fn init() -> Result<i32> {
    let root = wt::main_root()?;
    let lane = root.join(LANE_DIR);
    std::fs::create_dir_all(&lane)?;
    std::fs::write(lane.join(".gitkeep"), "")?;

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

/// `lane <name>`: enter it if it exists, otherwise create it as `lane new` would.
fn open(name: &str, base: Option<&str>, dirty: bool) -> Result<i32> {
    let root = wt::main_root()?;
    if lane_named(&root, name).is_ok() {
        let mut conflicts = Vec::new();
        if base.is_some() {
            conflicts.push("--base");
        }
        if dirty {
            conflicts.push("--dirty");
        }
        if !conflicts.is_empty() {
            bail!(
                "lane {name} already exists; {} only apply when creating a lane",
                conflicts.join(" and ")
            );
        }
        return enter(name);
    }
    // A typo would otherwise become a branch and a worktree. An existing branch of that
    // name is the author's answer either way, and `lane new` says it outright.
    let refname = format!("refs/heads/{name}");
    let branch = git::git_ok(&["rev-parse", "--verify", "--quiet", &refname], Some(&root));
    if !branch {
        if let Some(similar) = args::nearest_command(name) {
            eprintln!("error: no lane {name}; did you mean `lane {similar}`?");
            eprintln!("  lane new {name}   to create a lane by that name anyway");
            return Ok(2);
        }
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

fn ls(json: bool) -> Result<i32> {
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
    let rows: Vec<_> = lanes
        .into_iter()
        .zip(dirty)
        .map(|(lane, dirty)| {
            let name = lane
                .path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let upstream = try_git(&["rev-parse", "@{upstream}"], Some(&lane.path));
            let state = if wt::landed(&root, &wt::trunk_name(&root), &lane.branch) {
                "landed"
            } else if !upstream.is_empty()
                && try_git(&["rev-parse", "HEAD"], Some(&lane.path)) == upstream
            {
                "pushed"
            } else {
                "open"
            };
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
    for lane in lanes {
        let name = lane
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
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
        wt::remove(&name)?;
        println!("removed {name}");
        removed += 1;
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

fn lane_named(root: &Path, name: &str) -> Result<PathBuf> {
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
            "note: no shell integration; add `eval \"$(lane shellenv)\"` to cd automatically"
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

fn rm(name: &str, force: bool) -> Result<i32> {
    let root = wt::main_root()?;
    if !force {
        let trunk = wt::trunk_name(&root);
        let path = wt::lanes_dir(&root).join(name);
        let losses = wt::losses(&root, &path, name, &trunk);
        if !losses.is_empty() {
            eprintln!("kept lane {name}: {}", losses.join(", "));
            eprintln!("  lane rm {name} --force   to discard it anyway");
            return Ok(1);
        }
    }
    wt::remove(name)?;
    println!("removed lane {name}");
    Ok(0)
}

// The wrapper lists what must NOT cd, not what must: a bare lane name now needs
// one too, so "commands that cd" is no longer a fixed, enumerable set.
fn shellenv(shell: args::Shell) -> Result<i32> {
    match shell {
        args::Shell::Fish => println!(
            r#"function lane
  switch "$argv[1]"
    case '' '-h' '--help' '-V' '--version' 'init' 'ls' 'rm' 'prune' 'shellenv' 'completions'
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
    ""|-h|--help|-V|--version|init|ls|rm|prune|shellenv|completions) command lane "$@" ;;
    *) p=$(command lane "$@") || return; cd "$p" ;;
  esac
}}"#
        ),
    }
    Ok(0)
}

const COMPLETION_COMMANDS: &str = "init new enter switch exit ls prune rm shellenv completions";

fn completions(shell: args::Shell) -> Result<i32> {
    match shell {
        args::Shell::Fish => println!(
            r#"complete -c lane -f
complete -c lane -n '__fish_use_subcommand' -a '{COMPLETION_COMMANDS}'
complete -c lane -n '__fish_use_subcommand' -a "(command lane ls 2>/dev/null | awk '{{print \$1}}')"
complete -c lane -n '__fish_use_subcommand' -s h -l help
complete -c lane -n '__fish_use_subcommand' -s V -l version

complete -c lane -n '__fish_seen_subcommand_from enter switch rm' -a "(command lane ls 2>/dev/null | awk '{{print \$1}}')"
complete -c lane -n '__fish_seen_subcommand_from new' -l base
complete -c lane -n '__fish_seen_subcommand_from new' -l dirty
complete -c lane -n '__fish_seen_subcommand_from ls' -l json
complete -c lane -n '__fish_seen_subcommand_from rm' -l force
complete -c lane -n '__fish_seen_subcommand_from prune' -l dry-run"#
        ),
        args::Shell::Bash => println!(
            r#"_lane() {{
  local cur
  cur="${{COMP_WORDS[COMP_CWORD]}}"
  lanes() {{ command lane ls 2>/dev/null | awk '{{print $1}}'; }}

  if [ "$COMP_CWORD" -eq 1 ]; then
    COMPREPLY=($(compgen -W "{COMPLETION_COMMANDS} $(lanes) -h --help -V --version" -- "$cur"))
    return
  fi

  case "${{COMP_WORDS[1]}}" in
    enter|switch) COMPREPLY=($(compgen -W "$(lanes)" -- "$cur")) ;;
    rm) COMPREPLY=($(compgen -W "$(lanes) --force -h --help" -- "$cur")) ;;
    new) COMPREPLY=($(compgen -W "--base --dirty -h --help" -- "$cur")) ;;
    ls) COMPREPLY=($(compgen -W "--json -h --help" -- "$cur")) ;;
    prune) COMPREPLY=($(compgen -W "--dry-run -h --help" -- "$cur")) ;;
    *) COMPREPLY=() ;;
  esac
}}
complete -F _lane lane"#,
        ),
        args::Shell::Zsh => println!(
            r#"#compdef lane

_lane() {{
  local -a cmds lanes
  cmds=({COMPLETION_COMMANDS})
  lanes=(${{(f)"$(command lane ls 2>/dev/null | awk '{{print $1}}')"}})

  if (( CURRENT == 2 )); then
    compadd -a cmds
    compadd -a lanes
    compadd -- -h --help -V --version
    return
  fi

  case "${{words[2]}}" in
    enter|switch) compadd -a lanes ;;
    rm) compadd -a lanes; compadd -- --force -h --help ;;
    new) compadd -- --base --dirty -h --help ;;
    ls) compadd -- --json -h --help ;;
    prune) compadd -- --dry-run -h --help ;;
  esac
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
