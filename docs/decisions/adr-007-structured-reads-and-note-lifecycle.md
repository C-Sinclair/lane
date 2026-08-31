# ADR-007: Machine-readable reads and an explicit note-lifecycle command family (removed)

- Status: superseded by ADR-001
- Date: 2026-08-31 (recording history)

## Context

Once notes (ADR-006) were a real feature, two gaps showed up in how agents — the subsystem's
primary intended audience — actually used it. First, the two read surfaces agents needed
most, `lane ls` and `lane why`, had no structured output; an agent had to scrape aligned
text columns and parse markdown headings to get at data `lane check` already emitted as
JSON. Second, the note lifecycle was split across inconsistent surfaces: `lane note` could
create or supersede a note, `lane holds` confirmed drift had resolved, `lane audit` could
retire a note as a side effect of a review, and pinning or restoring a retired note required
editing `.lane/` by hand — there was no one place that owned "everything you can deliberately
do to a note."

Anchors compounded this: attaching a note required guessing the exact anchor string, and a
misspelled one was silently accepted with only a warning, evicted on the next audit with no
trace of what it had meant to point at.

## Decision

Three plans, landed as one connected sequence:

1. Add `--json` to `lane ls` and `lane why`, fixing field names and empty-result behavior as
   a compatibility contract up front rather than letting an executor invent the schema
   ad hoc.
2. Add `lane anchors <path>`, exposing the same tree-sitter declaration/heading/component
   resolution the audit already used internally, so a note's anchor could be discovered and
   validated before being typed rather than guessed and silently accepted.
3. Consolidate every intentional note mutation under `lane note` (`add`, `replace`,
   `confirm`, `retire`, `restore`, plus pin/unpin), renaming the opaque `lane holds`
   judgment to the clearer `confirm`, and removing the old `lane note -p ...
   [--supersedes]` / `lane holds <id>` spellings outright rather than keeping aliases —
   justified because the tool was not yet released and doubling the parser, help, and docs
   to preserve two spellings indefinitely wasn't worth it.

## What removal traded away

- A stable JSON contract an orchestrating agent could depend on for lane and note state,
  without scraping human-formatted output.
- Deterministic anchor discovery — `lane anchors` as the answer to "what can I attach a note
  to here" — and the qualification step that turned ambiguous shorthand into one canonical
  stored anchor.
- One coherent verb family for the note lifecycle, in place of state mutations scattered
  across `note`, `holds`, and manual edits to `.lane/`.

All three depended on the notes subsystem existing at all (ADR-006) and left with it under
ADR-001. Nothing here is revived by the flag-only CLI redesign (ADR-004) — that redesign
only reshaped how *lane's own* operations are spelled, not a note lifecycle that no longer
exists.
