# ADR-011: Auto-install an agent skill and git hooks (removed)

- Status: superseded by ADR-001
- Date: 2026-08-31 (recording history)

## Context

`lane init` used to append a three-line protocol to `AGENTS.md` — enough to tell an agent
not to hand-edit `.context/`/`.lane/` and to check it before editing a file, but not enough
to explain lanes, the anchor grammar, or the `Why:` trailer (ADR-009). `lane install skill`
was added to give agents the fuller picture as a loadable skill, kept separate from
`AGENTS.md` so the always-in-context file could stay three lines while the skill carried the
detail, loaded only when an agent was actually doing lane work.

Two correctness problems showed up in how lane maintained text it had written into a user's
files:

- **The protocol block in `AGENTS.md` could never be repaired.** `lane init` wrote it once
  and thereafter only checked whether the `## Context memory` heading existed at all — so a
  version with a real bug in it (`lane note -a <anchor> "..."`, missing the required
  `--path`, which exited 2) stayed broken in every repository that had already run `init`,
  with no re-run able to fix it.
- **`lane uninstall hooks` reported success while doing nothing.** The removal logic matched
  against the *current* marker text; a hook installed before a later change to that text
  (as happened when the post-commit hook's content changed) no longer matched, so the
  "removed" message printed over a hook that was still sitting in `.git/hooks/` unchanged.

Both were fixed by giving the protocol block the same replaceable-marker treatment the hooks
already had, so any previously-installed version could be found and swapped for the current
one on a re-run of `init`.

Separately, the post-commit hook that captured `Why:` trailers failed silently when `lane`
wasn't on `PATH` — `command -v lane || true` is correct in that a broken hook must never
fail a commit, but it meant a trailer, and the thought behind it, could vanish with no
warning at all. This was hit for real during development of this repository (see FR-001 for
the closely related signing-agent friction) and fixed by warning on stderr specifically when
there was something to capture and the command wasn't found.

## Decision

None beyond the fixes above — no removal decision recorded here; see ADR-001.

## What removal traded away

- Auto-onboarding: a repository that ran `lane --init` got a working agent skill and hooks
  with no separate install step.
- The replaceable-marker pattern this subsystem converged on (protocol block, hooks) is
  general and worth reusing if lane ever again writes text into a file it doesn't fully own
  — the lesson being that "check whether a marker exists" and "check whether the marker's
  *content* matches what you'd write today" are different questions, and only the second
  one lets you repair what you already installed.
