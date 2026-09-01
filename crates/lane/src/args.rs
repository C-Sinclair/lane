//! Argument parsing. Every bare positional argument is unambiguously a lane
//! name; the operation is chosen entirely by flags, mirroring `git-wt`.
//!
//! [`parse`] is pure and takes the argument list explicitly, so a test drives it
//! without a process. `-h`/`--help` and `-V`/`--version` anywhere win over the
//! rest of the arguments. Words after `--` are positional whatever they look
//! like, which is what lets a lane be named `-x`. With no operation flag and no
//! name, `lane` lists. With no operation flag and one name, `lane <name>`
//! enters it, creating it first if it does not exist.

use crate::help::Help;
use anyhow::Result;
use std::ffi::OsString;
use std::path::PathBuf;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenArgs {
    pub name: String,
    pub base: Option<String>,
    pub dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteArgs {
    pub names: Vec<String>,
    pub force: bool,
}

/// Which shell a rendered script (`--shellenv`, `--completions`) targets.
/// `bash`, `zsh`, and `posix` all render the same POSIX `shellenv` form;
/// `--completions` has no `Posix` script and rejects the word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Fish,
    Bash,
    Zsh,
    Posix,
}

/// What the argument list asked for. `Help` and `Version` are answers in their
/// own right rather than a flag on an operation, because neither reaches the
/// store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parsed {
    List {
        json: bool,
        global: bool,
        refresh: bool,
    },
    Open(OpenArgs),
    Delete(DeleteArgs),
    Info {
        name: Option<String>,
        json: bool,
    },
    Prune {
        dry_run: bool,
    },
    Init,
    Exit,
    Shellenv(Shell),
    Completions(Shell),
    Help(Help),
    Version,
}

/// One of the mutually exclusive operation flags. A bare name (`Open`) and no
/// flags at all (`List`) are not in this set: they are what parsing falls back
/// to once none of these are present.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    List,
    Delete,
    ForceDelete,
    Info,
    Prune,
    Init,
    Exit,
    Shellenv,
    Completions,
}

impl Op {
    fn flag(self) -> &'static str {
        match self {
            Op::List => "-l/--list",
            Op::Delete => "-d/--delete",
            Op::ForceDelete => "-D/--force-delete",
            Op::Info => "-i/--info",
            Op::Prune => "--prune",
            Op::Init => "--init",
            Op::Exit => "-e/--exit",
            Op::Shellenv => "--shellenv",
            Op::Completions => "--completions",
        }
    }
}

/// What operation the arguments settled on, once a bare name and no name at
/// all have been folded in alongside the explicit flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Selected {
    List,
    Open,
    Delete,
    ForceDelete,
    Info,
    Prune,
    Init,
    Exit,
    Shellenv,
    Completions,
}

/// Parse the arguments after the program name.
///
/// # Example
/// ```
/// use lane::args::{Parsed, parse};
/// let parsed = parse(vec!["--exit".into()]).unwrap();
/// assert_eq!(parsed, Parsed::Exit);
/// ```
pub fn parse(raw: Vec<OsString>) -> Result<Parsed> {
    let (flags, after) = terminated(raw);
    let mut pargs = pico_args::Arguments::from_vec(flags);

    if pargs.contains(["-h", "--help"]) {
        return Ok(Parsed::Help(Help::Root));
    }
    if pargs.contains(["-V", "--version"]) {
        return Ok(Parsed::Version);
    }

    let list = pargs.contains(["-l", "--list"]);
    let json = pargs.contains("--json");
    let global = pargs.contains(["-g", "--global"]);
    let delete = pargs.contains(["-d", "--delete"]);
    let force_delete = pargs.contains(["-D", "--force-delete"]);
    let info = pargs.contains(["-i", "--info"]);
    let prune = pargs.contains("--prune");
    let dry_run = pargs.contains("--dry-run");
    let init = pargs.contains("--init");
    let exit = pargs.contains(["-e", "--exit"]);
    let shellenv = pargs.contains("--shellenv");
    let completions = pargs.contains("--completions");
    let base: Option<String> = pargs.opt_value_from_str(["-b", "--base"])?;
    let dirty = pargs.contains("--dirty");
    let refresh = pargs.contains("--refresh");

    let positionals = positionals(pargs, after, Help::Root)?;

    let mut ops = Vec::new();
    if list {
        ops.push(Op::List);
    }
    if delete {
        ops.push(Op::Delete);
    }
    if force_delete {
        ops.push(Op::ForceDelete);
    }
    if info {
        ops.push(Op::Info);
    }
    if prune {
        ops.push(Op::Prune);
    }
    if init {
        ops.push(Op::Init);
    }
    if exit {
        ops.push(Op::Exit);
    }
    if shellenv {
        ops.push(Op::Shellenv);
    }
    if completions {
        ops.push(Op::Completions);
    }
    if let [a, b, ..] = ops[..] {
        return Err(conflict(a.flag(), b.flag(), Help::Root));
    }

    let selected = match ops.first() {
        Some(Op::List) => Selected::List,
        Some(Op::Delete) => Selected::Delete,
        Some(Op::ForceDelete) => Selected::ForceDelete,
        Some(Op::Info) => Selected::Info,
        Some(Op::Prune) => Selected::Prune,
        Some(Op::Init) => Selected::Init,
        Some(Op::Exit) => Selected::Exit,
        Some(Op::Shellenv) => Selected::Shellenv,
        Some(Op::Completions) => Selected::Completions,
        None if positionals.is_empty() && base.is_none() && !dirty => Selected::List,
        None => Selected::Open,
    };

    // Each modifier belongs to exactly one operation; anywhere else it is a
    // usage error rather than something silently ignored.
    let (base_ok, dirty_ok, json_ok, dry_run_ok, global_ok) = match selected {
        Selected::Open => (true, true, false, false, false),
        Selected::List => (false, false, true, false, true),
        Selected::Info => (false, false, true, false, false),
        Selected::Prune => (false, false, false, true, false),
        _ => (false, false, false, false, false),
    };
    if base.is_some() && !base_ok {
        return Err(misplaced("--base", Help::Root));
    }
    if dirty && !dirty_ok {
        return Err(misplaced("--dirty", Help::Root));
    }
    if json && !json_ok {
        return Err(misplaced("--json", Help::Root));
    }
    if dry_run && !dry_run_ok {
        return Err(misplaced("--dry-run", Help::Root));
    }
    if global && !global_ok {
        return Err(misplaced("-g/--global", Help::Root));
    }
    // `--refresh` only means something next to `-g`: it names which cache entry to skip,
    // and `-g` is the only thing here that reads one.
    if refresh && !global {
        return Err(misplaced("--refresh", Help::Root));
    }

    let parsed = match selected {
        Selected::Delete | Selected::ForceDelete => {
            if positionals.is_empty() {
                return Err(missing(&["<NAME>..."], Help::Root));
            }
            Parsed::Delete(DeleteArgs {
                names: positionals,
                force: selected == Selected::ForceDelete,
            })
        }
        Selected::Shellenv => {
            let word = optional_one(positionals, Help::Root)?;
            let shell = match word {
                None => detect_shell(),
                Some(word) => shell_named(&word, &["fish", "bash", "zsh", "posix"], Help::Root)?,
            };
            Parsed::Shellenv(shell)
        }
        Selected::Completions => {
            let word = one(positionals, "<SHELL>", Help::Root)?;
            let shell = shell_named(&word, &["fish", "bash", "zsh"], Help::Root)?;
            Parsed::Completions(shell)
        }
        Selected::Info => {
            let name = optional_one(positionals, Help::Root)?;
            Parsed::Info { name, json }
        }
        Selected::Prune => {
            none(positionals, Help::Root)?;
            Parsed::Prune { dry_run }
        }
        Selected::Init => {
            none(positionals, Help::Root)?;
            Parsed::Init
        }
        Selected::Exit => {
            none(positionals, Help::Root)?;
            Parsed::Exit
        }
        Selected::List => {
            none(positionals, Help::Root)?;
            Parsed::List {
                json,
                global,
                refresh,
            }
        }
        Selected::Open => {
            let name = one(positionals, "<NAME>", Help::Root)?;
            Parsed::Open(OpenArgs { name, base, dirty })
        }
    };
    Ok(parsed)
}

/// Split at a bare `--`. Nothing after it is read as a flag, by pico-args or by
/// the leftover check below.
fn terminated(raw: Vec<OsString>) -> (Vec<OsString>, Vec<OsString>) {
    match raw.iter().position(|a| a.as_os_str() == "--") {
        Some(at) => {
            let mut flags = raw;
            let tail = flags.split_off(at);
            (flags, tail.into_iter().skip(1).collect())
        }
        None => (raw, Vec::new()),
    }
}

/// What pico-args could not place, refusing anything that still looks like a
/// flag. Words held back by `--` join the positionals without that check.
fn positionals(
    pargs: pico_args::Arguments,
    after: Vec<OsString>,
    help: Help,
) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for arg in pargs.finish() {
        let text = arg.to_string_lossy().into_owned();
        if text.starts_with('-') {
            return Err(unexpected(&text, help));
        }
        out.push(text);
    }
    out.extend(after.into_iter().map(|a| a.to_string_lossy().into_owned()));
    Ok(out)
}

fn one(got: Vec<String>, name: &str, help: Help) -> Result<String> {
    let mut got = got.into_iter();
    let Some(first) = got.next() else {
        return Err(missing(&[name], help));
    };
    match got.next() {
        Some(extra) => Err(unexpected(&extra, help)),
        None => Ok(first),
    }
}

fn none(got: Vec<String>, help: Help) -> Result<()> {
    match got.into_iter().next() {
        Some(extra) => Err(unexpected(&extra, help)),
        None => Ok(()),
    }
}

/// `--shellenv`'s whole surface is zero or one word.
fn optional_one(got: Vec<String>, help: Help) -> Result<Option<String>> {
    let mut got = got.into_iter();
    let Some(first) = got.next() else {
        return Ok(None);
    };
    match got.next() {
        Some(extra) => Err(unexpected(&extra, help)),
        None => Ok(Some(first)),
    }
}

fn shell_named(word: &str, accepted: &[&str], help: Help) -> Result<Shell> {
    match word {
        "fish" if accepted.contains(&"fish") => Ok(Shell::Fish),
        "bash" if accepted.contains(&"bash") => Ok(Shell::Bash),
        "zsh" if accepted.contains(&"zsh") => Ok(Shell::Zsh),
        "posix" if accepted.contains(&"posix") => Ok(Shell::Posix),
        other => Err(unknown_shell(other, accepted, help)),
    }
}

/// The basename of `$SHELL`, falling back to `posix` when it is unset or is not
/// one lane has a dedicated script for.
fn detect_shell() -> Shell {
    let name = std::env::var_os("SHELL")
        .map(PathBuf::from)
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));
    match name.as_deref() {
        Some("fish") => Shell::Fish,
        Some("bash") => Shell::Bash,
        Some("zsh") => Shell::Zsh,
        _ => Shell::Posix,
    }
}

fn unexpected(token: &str, help: Help) -> anyhow::Error {
    // A word that only looks like a flag — a branch named `-x` — has somewhere
    // to go, and the reader is told where rather than left to guess.
    let tip = match token.starts_with('-') {
        true => format!("\n\n  tip: to pass '{token}' as a value, use '-- {token}'"),
        false => String::new(),
    };
    anyhow::anyhow!(
        "unexpected argument '{token}' found{tip}\n\nUsage: {}\n\nFor more information, try '{} --help'.",
        help.usage(),
        help.invocation()
    )
}

fn missing(absent: &[&str], help: Help) -> anyhow::Error {
    let list = absent
        .iter()
        .map(|name| format!("  {name}"))
        .collect::<Vec<_>>()
        .join("\n");
    anyhow::anyhow!(
        "the following required arguments were not provided:\n{list}{}\n\nUsage: {}\n\nFor more information, try '{} --help'.",
        help.tip(),
        help.usage(),
        help.invocation()
    )
}

fn unknown_shell(got: &str, accepted: &[&str], help: Help) -> anyhow::Error {
    anyhow::anyhow!(
        "unknown shell '{got}', expected one of: {}\n\nUsage: {}\n\nFor more information, try '{} --help'.",
        accepted.join(", "),
        help.usage(),
        help.invocation()
    )
}

fn conflict(a: &str, b: &str, help: Help) -> anyhow::Error {
    anyhow::anyhow!(
        "{a} cannot be combined with {b}\n\nUsage: {}\n\nFor more information, try '{} --help'.",
        help.usage(),
        help.invocation()
    )
}

fn misplaced(flag: &str, help: Help) -> anyhow::Error {
    anyhow::anyhow!(
        "{flag} does not apply here\n\nUsage: {}\n\nFor more information, try '{} --help'.",
        help.usage(),
        help.invocation()
    )
}
