//! The help screens, and the usage lines the parse errors quote.
//!
//! Every screen is a static string rather than something rendered from the
//! parser's tables: nothing here can drift at run time, and the wording is
//! written for a reader instead of derived from field names. [`Help::usage`] is
//! the same line the screen opens with, so an error and the screen it points at
//! cannot disagree.

/// Which screen to print. Reached from a `-h`/`--help` anywhere in a command's
/// arguments, and from a bare `lane`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Help {
    Root,
    Init,
    New,
    Ls,
    Enter,
    Exit,
    Prune,
    Rm,
    Shellenv,
}

impl Help {
    /// The whole screen, as printed to stdout.
    pub fn text(self) -> &'static str {
        match self {
            Help::Root => ROOT,
            Help::Init => INIT,
            Help::New => NEW,
            Help::Ls => LS,
            Help::Enter => ENTER,
            Help::Exit => EXIT,
            Help::Prune => PRUNE,
            Help::Rm => RM,
            Help::Shellenv => SHELLENV,
        }
    }

    /// The one-line grammar, quoted by a parse error.
    pub fn usage(self) -> &'static str {
        match self {
            Help::Root => "lane <COMMAND>",
            Help::Init => "lane init",
            Help::New => "lane new [OPTIONS] <NAME>",
            Help::Ls => "lane ls [--json]",
            Help::Enter => "lane enter <NAME>",
            Help::Exit => "lane exit",
            Help::Prune => "lane prune [--dry-run]",
            Help::Rm => "lane rm [--force] <NAME>",
            Help::Shellenv => "lane shellenv",
        }
    }

    /// A line saying what a command's choices mean, where the names do not say
    /// it themselves. Shown under any error about them, so the reader who typed
    /// the wrong one and the reader who typed neither are told the same thing.
    pub fn tip(self) -> &'static str {
        ""
    }

    /// What to tell the reader to run for more, without the flag.
    pub fn invocation(self) -> &'static str {
        match self {
            Help::Root => "lane",
            Help::Init => "lane init",
            Help::New => "lane new",
            Help::Ls => "lane ls",
            Help::Enter => "lane enter",
            Help::Exit => "lane exit",
            Help::Prune => "lane prune",
            Help::Rm => "lane rm",
            Help::Shellenv => "lane shellenv",
        }
    }
}

// Each screen opens with a blank line and indents two spaces; `run` prints them
// with `println!`, which supplies the matching trailing one, so a screen is
// padded top and bottom.

const ROOT: &str = "
  Usage
    $ lane <command> [options]

  Commands
    init         Initialize lane in a repository
    new          Create a copy-on-write worktree
    enter        Enter a lane
    exit         Return to the main worktree
    ls           List lanes
    prune        Remove landed lanes
    rm           Discard a lane
    shellenv     Print shell integration

  Options
    -h, --help       Display this message
    -V, --version    Display current version

  Examples
    $ lane new fix-login
    $ lane enter fix-login
    $ lane prune
";

const INIT: &str = "
  Description
    Create .lane/ and check reflink support. Safe to re-run.

  Usage
    $ lane init

  Options
    -h, --help    Display this message
";

const NEW: &str = "
  Description
    Create a branch and worktree under .lane/trees/. Ignored files are cloned
    by reference when the filesystem supports reflinks.

  Usage
    $ lane new <name> [options]

  Options
    --base <rev>    Branch from <rev> instead of the default base
    --dirty         Carry uncommitted work into the lane
    -h, --help      Display this message

  Examples
    $ lane new fix-login
    $ lane new spike --dirty
    $ lane new hotfix --base v1.2.0
";

const LS: &str = "
  Description
    List each lane's state and worktree status.

  Usage
    $ lane ls [--json]

  Options
    --json        Emit machine-readable JSON
    -h, --help    Display this message
";

const ENTER: &str = "
  Description
    Change directory into a lane. `switch` is an alias.

  Usage
    $ lane enter <name>

  Options
    -h, --help    Display this message

  Examples
    $ lane enter fix-login
    $ lane switch fix-login
";

const EXIT: &str = "
  Description
    Change directory back to the main worktree.

  Usage
    $ lane exit

  Options
    -h, --help    Display this message
";

const PRUNE: &str = "
  Description
    Remove lanes whose branches have landed. Uncommitted work and commits made
    after landing are never discarded.

  Usage
    $ lane prune [options]

  Options
    --dry-run     List what would go, remove nothing
    -h, --help    Display this message
";

const RM: &str = "
  Description
    Remove a lane and its branch and worktree. Refuse if anything would be
    lost unless --force is given.

  Usage
    $ lane rm <name> [options]

  Options
    --force       Discard it anyway
    -h, --help    Display this message
";

const SHELLENV: &str = "
  Description
    Print the shell function that makes `lane enter`, `lane exit`, and
    `lane new` leave the shell in the right directory.

  Usage
    $ eval \"$(lane shellenv)\"

  Options
    -h, --help    Display this message
";
