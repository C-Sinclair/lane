# ADR-004: Choose the operation by flag, so every bare word is a lane name

- Status: accepted
- Date: 2026-08-31

## Context

Lane's older CLI mixed a subcommand grammar (`lane new`, `lane ls`, `lane rm`, `lane prune`,
…) with the everyday form of typing a bare word to enter or create a lane by that name.
Those two things collide whenever a lane's name matches a command word: the parser has to
pick a winner, and it wasn't the lane. The grammar also needed a near-miss typo guard —
`lane sl` meaning `lane ls` — which is machinery any subcommand grammar mixed with
free-form user input eventually needs.

## Decision

Remove the subcommand grammar. Every operation other than "enter or create a lane" is
spelled as a flag (`-d`/`-D`, `--prune`, `--init`, `--exit`, `--shellenv`, `--completions`).
A bare positional word is always, unconditionally, a lane name.

## Alternatives considered

**Keep subcommands, add a precedence rule.** ("A command outranks a same-named lane" was
the rule in place before this decision.) Workable, but it means a lane can be unnamable —
choose a name that happens to be a future subcommand and the parser reinterprets your lane
as a command the next time one is added. It also leaves the typo-guard problem unsolved.

**Keep subcommands, reserve the namespace.** Disallow lane names matching any command word.
This constrains what a lane can be called for reasons that have nothing to do with the lane
itself, and the constraint grows every time a new subcommand is added.

## Consequences

- No precedence rule and no typo guard are needed, because there is no word a lane name
  could collide with — flags and lane names occupy disjoint syntactic positions.
- A lane may be named `ls`, `prune`, `delete`, or anything else that used to be reserved.
- The flag surface is necessarily flatter than a subcommand tree could be — there's no room
  for a subcommand's own sub-flags to live in a separate namespace. This has been fine so
  far because the whole surface is small (DD-005); it would need revisiting if the flag
  count grew large enough to make the flat namespace itself hard to scan.
- This shipped alongside ADR-001's removal of most of the subcommands it used to
  disambiguate — fewer surviving subcommands made the grammar switch cheap to do at the same
  time rather than a separate migration later.
