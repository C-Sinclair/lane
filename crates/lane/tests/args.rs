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
fn bare_lane_lists_and_the_help_flags_reach_the_root_screen() {
    assert_eq!(ok(&[]), Parsed::List { json: false });
    assert_eq!(ok(&["-h"]), Parsed::Help(Help::Root));
    assert_eq!(ok(&["--help"]), Parsed::Help(Help::Root));
}

#[test]
fn version_has_clap_s_spelling() {
    assert_eq!(ok(&["-V"]), Parsed::Version);
    assert_eq!(ok(&["--version"]), Parsed::Version);
}

#[test]
fn every_flag_answers_help_when_it_is_present() {
    for words in [
        &["--init", "--help"][..],
        &["--prune", "-h"][..],
        &["-d", "spike", "--help"][..],
        &["--shellenv", "-h"][..],
        &["--completions", "-h"][..],
        &["fix-login", "--help"][..],
    ] {
        assert_eq!(ok(words), Parsed::Help(Help::Root), "{words:?}");
    }
}

#[test]
fn help_wins_over_the_arguments_beside_it() {
    assert_eq!(
        ok(&["fix-login", "--base", "main", "-h"]),
        Parsed::Help(Help::Root)
    );
    assert_eq!(ok(&["-D", "spike", "-h"]), Parsed::Help(Help::Root));
}

#[test]
fn no_args_lists() {
    assert_eq!(ok(&[]), Parsed::List { json: false });
}

#[test]
fn list_takes_only_json() {
    assert_eq!(ok(&["--list"]), Parsed::List { json: false });
    assert_eq!(ok(&["-l"]), Parsed::List { json: false });
    assert_eq!(ok(&["--json"]), Parsed::List { json: true });
    assert_eq!(ok(&["--list", "--json"]), Parsed::List { json: true });
    assert!(err(&["--list", "extra"]).contains("unexpected argument 'extra' found"));
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
fn a_lane_may_be_named_ls_or_prune() {
    assert_eq!(
        ok(&["ls"]),
        Parsed::Open(OpenArgs {
            name: "ls".into(),
            base: None,
            dirty: false,
        })
    );
    assert_eq!(
        ok(&["prune", "--dirty"]),
        Parsed::Open(OpenArgs {
            name: "prune".into(),
            base: None,
            dirty: true,
        })
    );
}

#[test]
fn shellenv_takes_an_optional_shell() {
    assert_eq!(ok(&["--shellenv", "fish"]), Parsed::Shellenv(Shell::Fish));
    assert_eq!(ok(&["--shellenv", "bash"]), Parsed::Shellenv(Shell::Bash));
    assert_eq!(ok(&["--shellenv", "zsh"]), Parsed::Shellenv(Shell::Zsh));
    assert_eq!(ok(&["--shellenv", "posix"]), Parsed::Shellenv(Shell::Posix));
    assert!(matches!(ok(&["--shellenv"]), Parsed::Shellenv(_)));
    let message = err(&["--shellenv", "csh"]);
    assert!(message.contains("unknown shell 'csh'"), "{message}");
    assert!(message.contains("fish, bash, zsh, posix"), "{message}");
}

#[test]
fn completions_requires_a_shell() {
    assert_eq!(
        ok(&["--completions", "fish"]),
        Parsed::Completions(Shell::Fish)
    );
    assert_eq!(
        ok(&["--completions", "bash"]),
        Parsed::Completions(Shell::Bash)
    );
    assert_eq!(
        ok(&["--completions", "zsh"]),
        Parsed::Completions(Shell::Zsh)
    );
    let missing = err(&["--completions"]);
    assert!(missing.contains("the following required arguments were not provided"));
    let message = err(&["--completions", "posix"]);
    assert!(message.contains("unknown shell 'posix'"), "{message}");
    assert!(message.contains("fish, bash, zsh"), "{message}");
    assert!(!message.contains("fish, bash, zsh, posix"), "{message}");
}

#[test]
fn open_reads_its_flags_before_or_after_the_name() {
    let expected = Parsed::Open(OpenArgs {
        name: "fix-login".into(),
        base: Some("main".into()),
        dirty: true,
    });
    assert_eq!(ok(&["fix-login", "--base", "main", "--dirty"]), expected);
    assert_eq!(ok(&["--base", "main", "--dirty", "fix-login"]), expected);
}

#[test]
fn open_defaults_the_flags_it_was_not_given() {
    assert_eq!(
        ok(&["spike"]),
        Parsed::Open(OpenArgs {
            name: "spike".into(),
            base: None,
            dirty: false,
        })
    );
}

#[test]
fn a_missing_name_names_itself() {
    let message = err(&["--base", "main"]);
    assert!(message.contains("the following required arguments were not provided"));
    assert!(message.contains("<NAME>"));
    assert!(message.contains("Usage: lane [options] [<name>]"));
    assert!(message.contains("try 'lane --help'"));
}

#[test]
fn a_word_that_only_looks_like_a_flag_is_told_where_to_go() {
    let message = err(&["spike", "--bogus"]);
    assert!(
        message.contains("tip: to pass '--bogus' as a value, use '-- --bogus'"),
        "{message}"
    );
    // A plain word is not a flag, so it gets no advice about `--`.
    assert!(!err(&["--list", "extra"]).contains("tip:"));
}

#[test]
fn a_word_that_is_not_a_flag_is_a_lane_name() {
    assert_eq!(
        ok(&["nope"]),
        Parsed::Open(OpenArgs {
            name: "nope".into(),
            base: None,
            dirty: false,
        })
    );
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
    assert_eq!(ok(&["--json"]), Parsed::List { json: true });
    assert!(err(&["--list", "--jsonn"]).contains("unexpected argument '--jsonn' found"));
}

#[test]
fn a_flag_no_operation_owns_is_refused() {
    assert!(err(&["spike", "--bogus"]).contains("unexpected argument '--bogus' found"));
    assert!(err(&["--bogus"]).contains("unexpected argument '--bogus' found"));
}

#[test]
fn every_screen_quotes_a_usage_line_it_agrees_with() {
    let text = Help::Root.text();
    assert!(text.starts_with('\n'), "opens with a blank line");
    assert!(text.contains("  Usage\n"), "has a usage section");
    assert!(text.contains("-h, --help"), "documents --help");
    assert!(
        Help::Root.usage().starts_with(Help::Root.invocation()),
        "usage and invocation disagree"
    );
}

#[test]
fn prune_takes_only_its_dry_run() {
    assert_eq!(ok(&["--prune"]), Parsed::Prune { dry_run: false });
    assert_eq!(
        ok(&["--prune", "--dry-run"]),
        Parsed::Prune { dry_run: true }
    );
    assert_eq!(ok(&["--prune", "--help"]), Parsed::Help(Help::Root));
    assert!(err(&["--prune", "extra"]).contains("unexpected argument 'extra' found"));
    assert!(err(&["--prune", "--dry"]).contains("unexpected argument '--dry' found"));
    assert_eq!(
        ok(&["sweep"]),
        Parsed::Open(OpenArgs {
            name: "sweep".into(),
            base: None,
            dirty: false,
        })
    );
}

#[test]
fn delete_takes_one_or_more_names() {
    assert_eq!(
        ok(&["-d", "spike"]),
        Parsed::Delete(DeleteArgs {
            names: vec!["spike".into()],
            force: false,
        })
    );
    assert_eq!(
        ok(&["-d", "spike", "second"]),
        Parsed::Delete(DeleteArgs {
            names: vec!["spike".into(), "second".into()],
            force: false,
        })
    );
    assert_eq!(
        ok(&["--delete", "spike"]),
        Parsed::Delete(DeleteArgs {
            names: vec!["spike".into()],
            force: false,
        })
    );
    assert_eq!(
        ok(&["-D", "spike"]),
        Parsed::Delete(DeleteArgs {
            names: vec!["spike".into()],
            force: true,
        })
    );
    assert_eq!(
        ok(&["--force-delete", "spike"]),
        Parsed::Delete(DeleteArgs {
            names: vec!["spike".into()],
            force: true,
        })
    );
    assert!(
        err(&["-d"]).contains("the following required arguments were not provided"),
        "{}",
        err(&["-d"])
    );
}

#[test]
fn conflicting_operation_flags_are_a_usage_error() {
    for words in [&["--prune", "--init"][..], &["-d", "spike", "--prune"][..]] {
        let message = err(words);
        assert!(message.contains("cannot be combined with"), "{message}");
    }
}

#[test]
fn base_and_dirty_only_apply_when_creating_a_lane() {
    assert!(err(&["--prune", "--dirty"]).contains("--dirty does not apply here"));
    assert!(err(&["--init", "--base", "main"]).contains("--base does not apply here"));
    assert!(err(&["-d", "spike", "--dirty"]).contains("--dirty does not apply here"));
}

#[test]
fn json_only_applies_when_listing() {
    assert!(err(&["--prune", "--json"]).contains("--json does not apply here"));
    assert!(err(&["fix-login", "--json"]).contains("--json does not apply here"));
}

#[test]
fn dry_run_only_applies_to_prune() {
    assert!(err(&["--dry-run"]).contains("--dry-run does not apply here"));
    assert!(err(&["--list", "--dry-run"]).contains("--dry-run does not apply here"));
}

#[test]
fn a_dashed_name_is_still_reachable_after_a_terminator() {
    assert_eq!(
        ok(&["--", "-dash-name"]),
        Parsed::Open(OpenArgs {
            name: "-dash-name".into(),
            base: None,
            dirty: false,
        })
    );
    assert_eq!(
        ok(&["-d", "--", "-dash-name"]),
        Parsed::Delete(DeleteArgs {
            names: vec!["-dash-name".into()],
            force: false,
        })
    );
}
