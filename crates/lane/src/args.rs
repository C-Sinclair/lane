//! Argument parsing. A leading word selects the command; everything after it is
//! read by `pico-args` and what it could not place is checked here.
//!
//! [`parse`] is pure and takes the argument list explicitly, so a test drives it
//! without a process. `-h`/`--help` anywhere in a command's arguments wins over
//! the rest of them, and a bare `lane` prints the root screen. Words after `--`
//! are positional whatever they look like, which is what lets a name start with
//! a dash. A first word that is not a known subcommand and does not start with
//! `-` is read as a lane name to create or enter — a known subcommand always
//! wins over a same-named lane.

use crate::help::Help;
use anyhow::Result;
use std::ffi::OsString;
use std::path::PathBuf;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Every command that appears in help, for the near-miss check on a bare name.
const COMMANDS: &[&str] = &[
    "init",
    "new",
    "enter",
    "switch",
    "exit",
    "ls",
    "prune",
    "rm",
    "shellenv",
    "completions",
];

/// The command a bare name was probably meant to be.
///
/// A bare name creates a lane, so a mistyped subcommand would otherwise leave a branch and
/// a worktree named after the typo. Two edits is the bound: past that the guess is noise
/// rather than a typo.
pub fn nearest_command(typed: &str) -> Option<&'static str> {
    COMMANDS
        .iter()
        .map(|name| (distance(typed, name), *name))
        .filter(|(d, _)| *d <= 2)
        .min_by_key(|(d, _)| *d)
        .map(|(_, name)| name)
}

fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ac) in a.chars().enumerate() {
        cur[0] = i + 1;
        for (j, bc) in b.iter().enumerate() {
            let cost = usize::from(ac != *bc);
            cur[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewArgs {
    pub name: String,
    pub base: Option<String>,
    pub dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenArgs {
    pub name: String,
    pub base: Option<String>,
    pub dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RmArgs {
    pub name: String,
    pub force: bool,
}

/// Which shell a rendered script (`shellenv`, `completions`) targets. `bash`,
/// `zsh`, and `posix` all render the same POSIX `shellenv` form; `completions`
/// has no `Posix` script and rejects the word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Fish,
    Bash,
    Zsh,
    Posix,
}

/// What the argument list asked for. `Help` and `Version` are answers in their
/// own right rather than a flag on a command, because neither reaches the store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parsed {
    Init,
    New(NewArgs),
    Open(OpenArgs),
    Ls { json: bool },
    Enter { name: String },
    Exit,
    Prune { dry_run: bool },
    Rm(RmArgs),
    Shellenv(Shell),
    Completions(Shell),
    Help(Help),
    Version,
}

/// Parse the arguments after the program name.
///
/// # Example
/// ```
/// use lane::args::{Parsed, parse};
/// let parsed = parse(vec!["exit".into()]).unwrap();
/// assert_eq!(parsed, Parsed::Exit);
/// ```
pub fn parse(raw: Vec<OsString>) -> Result<Parsed> {
    let head = raw.first().and_then(|a| a.to_str()).map(str::to_owned);
    match head.as_deref() {
        Some("init") => bare(rest(raw), Help::Init, Parsed::Init),
        Some("new") => parse_new(rest(raw)),
        Some("ls") => parse_ls(rest(raw)),
        Some("enter" | "switch") => parse_one(rest(raw), Help::Enter, "<NAME>", |name| {
            Parsed::Enter { name }
        }),
        Some("exit") => bare(rest(raw), Help::Exit, Parsed::Exit),
        Some("prune") => parse_prune(rest(raw)),
        Some("rm") => parse_rm(rest(raw)),
        Some("shellenv") => parse_shellenv(rest(raw)),
        Some("completions") => parse_completions(rest(raw)),
        Some("-h" | "--help") => Ok(Parsed::Help(Help::Root)),
        Some("-V" | "--version") => Ok(Parsed::Version),
        None => Ok(Parsed::Help(Help::Root)),
        Some(other) if other.starts_with('-') => Err(unexpected(other, Help::Root)),
        // Not a known subcommand: read it as a lane name to create or enter.
        // A subcommand added later always shadows a lane of the same name.
        Some(_) => parse_open(raw),
    }
}

/// Drop the leading command word.
fn rest(raw: Vec<OsString>) -> Vec<OsString> {
    raw.into_iter().skip(1).collect()
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

/// A command whose whole surface is zero or one word.
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

/// A command with no arguments of its own.
fn bare(raw: Vec<OsString>, help: Help, parsed: Parsed) -> Result<Parsed> {
    let (flags, after) = terminated(raw);
    let mut pargs = pico_args::Arguments::from_vec(flags);
    if pargs.contains(["-h", "--help"]) {
        return Ok(Parsed::Help(help));
    }
    none(positionals(pargs, after, help)?, help)?;
    Ok(parsed)
}

/// A command whose whole surface is one required word.
fn parse_one<F>(raw: Vec<OsString>, help: Help, name: &str, build: F) -> Result<Parsed>
where
    F: FnOnce(String) -> Parsed,
{
    let (flags, after) = terminated(raw);
    let mut pargs = pico_args::Arguments::from_vec(flags);
    if pargs.contains(["-h", "--help"]) {
        return Ok(Parsed::Help(help));
    }
    Ok(build(one(positionals(pargs, after, help)?, name, help)?))
}

fn parse_new(raw: Vec<OsString>) -> Result<Parsed> {
    let (flags, after) = terminated(raw);
    let mut pargs = pico_args::Arguments::from_vec(flags);
    if pargs.contains(["-h", "--help"]) {
        return Ok(Parsed::Help(Help::New));
    }
    let base = pargs.opt_value_from_str("--base")?;
    let dirty = pargs.contains("--dirty");
    let name = one(positionals(pargs, after, Help::New)?, "<NAME>", Help::New)?;
    Ok(Parsed::New(NewArgs { name, base, dirty }))
}

/// A bare lane name, with `new`'s creation flags available for the case where
/// it does not exist yet. `raw` here still has the name as its first word,
/// since there is no command word to drop.
fn parse_open(raw: Vec<OsString>) -> Result<Parsed> {
    let (flags, after) = terminated(raw);
    let mut pargs = pico_args::Arguments::from_vec(flags);
    if pargs.contains(["-h", "--help"]) {
        return Ok(Parsed::Help(Help::Open));
    }
    let base = pargs.opt_value_from_str("--base")?;
    let dirty = pargs.contains("--dirty");
    let name = one(positionals(pargs, after, Help::Open)?, "<NAME>", Help::Open)?;
    Ok(Parsed::Open(OpenArgs { name, base, dirty }))
}

fn parse_ls(raw: Vec<OsString>) -> Result<Parsed> {
    let (flags, after) = terminated(raw);
    let mut pargs = pico_args::Arguments::from_vec(flags);
    if pargs.contains(["-h", "--help"]) {
        return Ok(Parsed::Help(Help::Ls));
    }
    let json = pargs.contains("--json");
    none(positionals(pargs, after, Help::Ls)?, Help::Ls)?;
    Ok(Parsed::Ls { json })
}

fn parse_prune(raw: Vec<OsString>) -> Result<Parsed> {
    let (flags, after) = terminated(raw);
    let mut pargs = pico_args::Arguments::from_vec(flags);
    if pargs.contains(["-h", "--help"]) {
        return Ok(Parsed::Help(Help::Prune));
    }
    let dry_run = pargs.contains("--dry-run");
    none(positionals(pargs, after, Help::Prune)?, Help::Prune)?;
    Ok(Parsed::Prune { dry_run })
}

fn parse_rm(raw: Vec<OsString>) -> Result<Parsed> {
    let (flags, after) = terminated(raw);
    let mut pargs = pico_args::Arguments::from_vec(flags);
    if pargs.contains(["-h", "--help"]) {
        return Ok(Parsed::Help(Help::Rm));
    }
    let force = pargs.contains("--force");
    let name = one(positionals(pargs, after, Help::Rm)?, "<NAME>", Help::Rm)?;
    Ok(Parsed::Rm(RmArgs { name, force }))
}

fn parse_shellenv(raw: Vec<OsString>) -> Result<Parsed> {
    let (flags, after) = terminated(raw);
    let mut pargs = pico_args::Arguments::from_vec(flags);
    if pargs.contains(["-h", "--help"]) {
        return Ok(Parsed::Help(Help::Shellenv));
    }
    let word = optional_one(positionals(pargs, after, Help::Shellenv)?, Help::Shellenv)?;
    let shell = match word {
        None => detect_shell(),
        Some(word) => shell_named(&word, &["fish", "bash", "zsh", "posix"], Help::Shellenv)?,
    };
    Ok(Parsed::Shellenv(shell))
}

fn parse_completions(raw: Vec<OsString>) -> Result<Parsed> {
    let (flags, after) = terminated(raw);
    let mut pargs = pico_args::Arguments::from_vec(flags);
    if pargs.contains(["-h", "--help"]) {
        return Ok(Parsed::Help(Help::Completions));
    }
    let word = one(
        positionals(pargs, after, Help::Completions)?,
        "<SHELL>",
        Help::Completions,
    )?;
    let shell = shell_named(&word, &["fish", "bash", "zsh"], Help::Completions)?;
    Ok(Parsed::Completions(shell))
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
