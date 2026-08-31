//! The help screen, and the usage line the parse errors quote.
//!
//! The screen is a static string rather than something rendered from the
//! parser's tables: nothing here can drift at run time, and the wording is
//! written for a reader instead of derived from field names. [`Help::usage`] is
//! the same line the screen opens with, so an error and the screen it points at
//! cannot disagree.

/// Reached from `-h`/`--help` anywhere in the arguments, and from a bare `lane`
/// with an unrecognised flag. There is only one screen now: the whole surface
/// is flags on the one command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Help {
    Root,
}

impl Help {
    /// The whole screen, as printed to stdout.
    pub fn text(self) -> &'static str {
        match self {
            Help::Root => ROOT,
        }
    }

    /// The one-line grammar, quoted by a parse error.
    pub fn usage(self) -> &'static str {
        match self {
            Help::Root => "lane [options] [<name>]",
        }
    }

    /// A line saying what a choice of arguments means, where the names do not
    /// say it themselves. Shown under any error about them, so the reader who
    /// typed the wrong one and the reader who typed none are told the same
    /// thing.
    pub fn tip(self) -> &'static str {
        ""
    }

    /// What to tell the reader to run for more, without the flag.
    pub fn invocation(self) -> &'static str {
        match self {
            Help::Root => "lane",
        }
    }
}

// The screen opens with a blank line and indents two spaces; `run` prints it
// with `println!`, which supplies the matching trailing one, so the screen is
// padded top and bottom.

const ROOT: &str = "
  Usage
    $ lane
    $ lane <name> [--base <rev>] [--dirty]
    $ lane -d|-D <name>...
    $ lane [options]

  Options
    -l, --list                 List lanes
    -g, --global               List lanes across every repository (slower: walks working trees)
        --json                 Emit machine-readable JSON (with or without --list)
    -b, --base <rev>           Branch from <rev> instead of the default base (create only)
        --dirty                Carry uncommitted work into the lane (create only)
    -d, --delete <name>...     Delete lane(s) and branch(es), refusing on loss
    -D, --force-delete         Delete anyway, discarding whatever it holds
        --prune                Remove landed lanes
        --dry-run              With --prune, list what would go, remove nothing
        --init                 Initialize lane in the repository
        --exit                 Return to the main worktree
        --shellenv [shell]     Print shell integration (fish, bash, zsh, posix)
        --completions <shell>  Print completions (fish, bash, zsh)
    -h, --help                 Display this message
    -V, --version              Display current version

  Examples
    $ lane
    $ lane fix-login              Creates it, or enters it if it already exists
    $ lane hotfix --base v1.2.0
    $ lane -d fix-login old-spike
    $ lane -g                     Every lane across every repository lane knows about
    $ lane --prune
    $ eval \"$(lane --shellenv)\"

  A lane may be named anything, including ls or prune.
";
