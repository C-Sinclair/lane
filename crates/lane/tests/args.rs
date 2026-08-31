//! The argument parser, driven the way a shell drives it: a list of words in,
//! one `Parsed` out, and no process in between.
//!
//! These live outside `args.rs` because they are longer than the parser they
//! cover, and only reach its public surface anyway.

use anyhow::Result;
use lane::args::*;
use lane::help::Help;
use std::ffi::OsString;

/// The parser is the whole surface now, so the tests drive it the way a
/// shell does: a list of words, with no process in between.
fn parse_words(words: &[&str]) -> Result<Parsed> {
    parse(words.iter().map(OsString::from).collect())
}

fn ok(words: &[&str]) -> Parsed {
    parse_words(words).expect("parses")
}

fn err(words: &[&str]) -> String {
    format!("{:#}", parse_words(words).expect_err("refuses"))
}

#[test]
fn bare_lane_and_the_help_flags_reach_the_root_screen() {
    assert_eq!(ok(&[]), Parsed::Help(Help::Root));
    assert_eq!(ok(&["-h"]), Parsed::Help(Help::Root));
    assert_eq!(ok(&["--help"]), Parsed::Help(Help::Root));
}

#[test]
fn version_has_clap_s_spelling() {
    assert_eq!(ok(&["-V"]), Parsed::Version);
    assert_eq!(ok(&["--version"]), Parsed::Version);
}

#[test]
fn every_command_answers_its_own_help_flag() {
    for (word, screen) in [
        ("init", Help::Init),
        ("new", Help::New),
        ("ls", Help::Ls),
        ("enter", Help::Enter),
        ("switch", Help::Enter),
        ("exit", Help::Exit),
        ("prune", Help::Prune),
        ("rm", Help::Rm),
        ("shellenv", Help::Shellenv),
        ("completions", Help::Completions),
    ] {
        assert_eq!(ok(&[word, "--help"]), Parsed::Help(screen), "{word} --help");
        assert_eq!(ok(&[word, "-h"]), Parsed::Help(screen), "{word} -h");
    }
    assert_eq!(ok(&["fix-login", "--help"]), Parsed::Help(Help::Open));
}

#[test]
fn help_wins_over_the_arguments_beside_it() {
    assert_eq!(ok(&["new", "some-lane", "-h"]), Parsed::Help(Help::New));
    assert_eq!(ok(&["rm", "--force", "-h"]), Parsed::Help(Help::Rm));
}

#[test]
fn commands_without_arguments_take_none() {
    assert_eq!(ok(&["init"]), Parsed::Init);
    assert_eq!(ok(&["ls"]), Parsed::Ls { json: false });
    assert!(err(&["ls", "extra"]).contains("unexpected argument 'extra' found"));
}

#[test]
fn a_bare_name_creates_or_enters_a_lane() {
    assert_eq!(
        ok(&["fix-login"]),
        Parsed::Open(OpenArgs {
            name: "fix-login".into(),
            base: None,
            dirty: false,
        })
    );
    assert_eq!(
        ok(&["spike", "--dirty"]),
        Parsed::Open(OpenArgs {
            name: "spike".into(),
            base: None,
            dirty: true,
        })
    );
    assert_eq!(
        ok(&["hotfix", "--base", "v1.2.0"]),
        Parsed::Open(OpenArgs {
            name: "hotfix".into(),
            base: Some("v1.2.0".into()),
            dirty: false,
        })
    );
    assert!(err(&["fix-login", "extra"]).contains("unexpected argument 'extra' found"));
}

#[test]
fn a_known_subcommand_wins_over_a_same_named_lane() {
    assert_eq!(ok(&["exit"]), Parsed::Exit);
    assert_eq!(ok(&["ls"]), Parsed::Ls { json: false });
}

#[test]
fn shellenv_takes_an_optional_shell() {
    assert_eq!(ok(&["shellenv", "fish"]), Parsed::Shellenv(Shell::Fish));
    assert_eq!(ok(&["shellenv", "bash"]), Parsed::Shellenv(Shell::Bash));
    assert_eq!(ok(&["shellenv", "zsh"]), Parsed::Shellenv(Shell::Zsh));
    assert_eq!(ok(&["shellenv", "posix"]), Parsed::Shellenv(Shell::Posix));
    assert!(matches!(ok(&["shellenv"]), Parsed::Shellenv(_)));
    let message = err(&["shellenv", "csh"]);
    assert!(message.contains("unknown shell 'csh'"), "{message}");
    assert!(message.contains("fish, bash, zsh, posix"), "{message}");
}

#[test]
fn completions_requires_a_shell() {
    assert_eq!(
        ok(&["completions", "fish"]),
        Parsed::Completions(Shell::Fish)
    );
    assert_eq!(
        ok(&["completions", "bash"]),
        Parsed::Completions(Shell::Bash)
    );
    assert_eq!(ok(&["completions", "zsh"]), Parsed::Completions(Shell::Zsh));
    let missing = err(&["completions"]);
    assert!(missing.contains("the following required arguments were not provided"));
    let message = err(&["completions", "posix"]);
    assert!(message.contains("unknown shell 'posix'"), "{message}");
    assert!(message.contains("fish, bash, zsh"), "{message}");
    assert!(!message.contains("fish, bash, zsh, posix"), "{message}");
}

#[test]
fn new_reads_its_flags_before_or_after_the_name() {
    let expected = Parsed::New(NewArgs {
        name: "fix-login".into(),
        base: Some("main".into()),
        dirty: true,
    });
    assert_eq!(
        ok(&["new", "fix-login", "--base", "main", "--dirty"]),
        expected
    );
    assert_eq!(
        ok(&["new", "--base", "main", "--dirty", "fix-login"]),
        expected
    );
}

#[test]
fn new_defaults_the_flags_it_was_not_given() {
    assert_eq!(
        ok(&["new", "spike"]),
        Parsed::New(NewArgs {
            name: "spike".into(),
            base: None,
            dirty: false,
        })
    );
}

#[test]
fn the_shell_function_s_own_invocation_still_parses() {
    // `lane shellenv` writes `command lane "$@"` for each of the verbs it wraps,
    // so every one of them must parse with nothing added.
    assert_eq!(
        ok(&["new", "fix-login"]),
        Parsed::New(NewArgs {
            name: "fix-login".into(),
            base: None,
            dirty: false,
        })
    );
    assert_eq!(
        ok(&["enter", "fix-login"]),
        Parsed::Enter {
            name: "fix-login".into()
        }
    );
    assert_eq!(
        ok(&["switch", "fix-login"]),
        Parsed::Enter {
            name: "fix-login".into()
        }
    );
    assert_eq!(ok(&["exit"]), Parsed::Exit);
}

#[test]
fn a_missing_name_names_itself() {
    let message = err(&["new"]);
    assert!(message.contains("the following required arguments were not provided"));
    assert!(message.contains("<NAME>"));
    assert!(message.contains("Usage: lane new [OPTIONS] <NAME>"));
    assert!(message.contains("try 'lane new --help'"));
}

#[test]
fn a_word_that_only_looks_like_a_flag_is_told_where_to_go() {
    let message = err(&["new", "spike", "--bogus"]);
    assert!(
        message.contains("tip: to pass '--bogus' as a value, use '-- --bogus'"),
        "{message}"
    );
    // A plain word is not a flag, so it gets no advice about `--`.
    assert!(!err(&["ls", "extra"]).contains("tip:"));
}

#[test]
fn the_old_done_command_is_now_read_as_a_lane_name() {
    assert_eq!(
        ok(&["done"]),
        Parsed::Open(OpenArgs {
            name: "done".into(),
            base: None,
            dirty: false,
        })
    );
}

#[test]
fn structured_read_commands_take_json() {
    assert_eq!(ok(&["ls", "--json"]), Parsed::Ls { json: true });
    assert!(err(&["ls", "--jsonn"]).contains("unexpected argument '--jsonn' found"));
}

#[test]
fn rm_reads_the_rest_of_its_flags() {
    assert_eq!(
        ok(&["rm", "spike", "--force"]),
        Parsed::Rm(RmArgs {
            name: "spike".into(),
            force: true,
        })
    );
    assert_eq!(
        ok(&["enter", "spike"]),
        Parsed::Enter {
            name: "spike".into()
        }
    );
    assert_eq!(ok(&["exit"]), Parsed::Exit);
}

#[test]
fn a_flag_no_command_owns_is_refused() {
    assert!(err(&["new", "spike", "--bogus"]).contains("unexpected argument '--bogus' found"));
    assert!(err(&["--bogus"]).contains("unexpected argument '--bogus' found"));
}

#[test]
fn a_word_that_is_not_a_flag_or_a_command_is_a_lane_name() {
    assert_eq!(
        ok(&["nope"]),
        Parsed::Open(OpenArgs {
            name: "nope".into(),
            base: None,
            dirty: false,
        })
    );
    assert_eq!(
        ok(&["nwe"]),
        Parsed::Open(OpenArgs {
            name: "nwe".into(),
            base: None,
            dirty: false,
        })
    );
}

#[test]
fn every_command_the_root_screen_lists_parses() {
    // Read out of the screen rather than out of a second list: a command added
    // to one and not the other is exactly what this is here to catch.
    let listed: Vec<&str> = Help::Root
        .text()
        .split("  Commands\n")
        .nth(1)
        .expect("the root screen lists commands")
        .lines()
        .take_while(|line| !line.trim().is_empty())
        .filter_map(|line| line.split_whitespace().next())
        .collect();
    assert_eq!(listed.len(), 9, "{listed:?}");
    for name in listed {
        assert!(matches!(ok(&[name, "--help"]), Parsed::Help(_)), "{name}");
    }
}

#[test]
fn every_screen_quotes_a_usage_line_it_agrees_with() {
    for screen in [
        Help::Root,
        Help::Init,
        Help::New,
        Help::Open,
        Help::Ls,
        Help::Enter,
        Help::Exit,
        Help::Prune,
        Help::Rm,
        Help::Shellenv,
        Help::Completions,
    ] {
        let text = screen.text();
        assert!(text.starts_with('\n'), "{screen:?} opens with a blank line");
        assert!(text.contains("  Usage\n"), "{screen:?} has a usage section");
        assert!(text.contains("-h, --help"), "{screen:?} documents --help");
        assert!(
            screen.usage().starts_with(screen.invocation()),
            "{screen:?} usage and invocation disagree"
        );
    }
}

#[test]
fn prune_takes_only_its_dry_run() {
    assert_eq!(ok(&["prune"]), Parsed::Prune { dry_run: false });
    assert_eq!(ok(&["prune", "--dry-run"]), Parsed::Prune { dry_run: true });
    assert_eq!(ok(&["prune", "--help"]), Parsed::Help(Help::Prune));
    assert!(err(&["prune", "extra"]).contains("unexpected argument 'extra' found"));
    assert!(err(&["prune", "--dry"]).contains("unexpected argument '--dry' found"));
    assert_eq!(
        ok(&["sweep"]),
        Parsed::Open(OpenArgs {
            name: "sweep".into(),
            base: None,
            dirty: false,
        })
    );
}
