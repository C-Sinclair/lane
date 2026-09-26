# ADR-018: Names read from stdin

- Status: accepted
- Date: 2026-09-26

## Context

A branch name usually comes out of another tool: `gh pr view 103 --json headRefName`, a picker, a script. Getting it into `lane` meant a command substitution, `lane "$(gh pr view 103 --json headRefName -q .headRefName)"`, which is harder to type and to build up interactively than a pipeline. `lane -d` over several names from a query had the same problem, with `xargs` as the usual answer.

Every bare word on the command line is a lane name ([ADR-004](adr-004-flag-based-cli.md)), and a bare `lane` lists. Reading stdin has to keep both true.

## Decision

`args::parse_with` takes the argument list and a closure that returns names piped on stdin. The parser calls the closure only when an operation that takes a name was given none:

- a bare `lane`, which opens the piped name, or still lists when stdin yields no names;
- `--base`/`--dirty` without a name, which open the piped name;
- `-d`/`-D` without names, which delete every piped name;
- `-i` without a name, which describes the piped name, or the current lane when stdin yields none.

Opening and `-i` take one name. More than one piped name is a usage error that points at `-d`. Listing flags such as `-l`, `--json` and `-g` never read stdin, and a name on the command line always wins.

`args::names_from` turns the piped text into names: one per line, whitespace trimmed, blank lines dropped, and a surrounding pair of double quotes removed so `jq` output without `-r` works.

`cli::piped_names` owns the read. It never reads a terminal. On anything else it waits up to `PIPE_WAIT` (5 seconds) for stdin to become readable, then reads to end of file. `/dev/null` and a regular file are readable at once, so `lane < /dev/null` lists without delay.

The `--shellenv` wrappers capture a bare `lane` when stdin is not a terminal. They `cd` when lane printed exactly one existing directory, and print the captured output otherwise, so a listing still reaches the reader.

## Alternatives considered

**Read stdin only after an explicit `-`**, as in `gh … | lane -`. This form never waits on stdin, because nothing reads it unless asked. It was rejected because the bare pipeline is the form people reach for, and `-` would be the only positional word that is not a lane name, which breaks ADR-004's rule.

**Block on a piped stdin with no timeout.** This is what `cat` does. It hung the end-to-end suite under a background job runner, which leaves an open pipe on stdin and never writes to it ([FR-012](../friction/FR-012-reading-stdin-met-three-shells-and-an-open-pipe.md)). Agents run `lane` that way routinely, and a hang on a bare `lane` is worse than any delay.

**Read stdin inside `args::parse`.** Rejected: the parser is pure so `tests/args.rs` can drive it with a list of words. The closure keeps it pure and lets a test supply names, or panic if stdin is read when it should not be.

## Consequences

- A bare `lane` or `lane -i` run under an open, silent pipe waits `PIPE_WAIT` before answering. A producer slower than `PIPE_WAIT` loses its race: `lane` lists instead of opening, and the producer gets `SIGPIPE`.
- Bash runs the last element of a pipeline in a subshell, so `… | lane` in bash creates or finds the lane and prints its path, but the shell does not move. `lane "$(…)"` still moves it. zsh and fish run the wrapper in the current shell and move.
- The fish wrapper captures through `| read -z` instead of a command substitution, because fish gives a command substitution the shell's stdin, not the function's.
