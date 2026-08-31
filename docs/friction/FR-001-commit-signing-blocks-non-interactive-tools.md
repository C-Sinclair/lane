# FR-001: Commit signing blocks non-interactive tool use

- Date: 2026-08-31

## What was hit

`git commit` from an automated or sandboxed context failed outright rather than committing
unsigned or prompting somewhere visible.

## Symptom

```
error: 1Password: failed to fill whole buffer
fatal: failed to write commit object
```

The 1Password SSH signing agent needs to prompt interactively for approval, and cannot
reach the user from a sandboxed or non-interactive shell — the fill request has nowhere to
surface, so it fails after a timeout instead.

It also silently poisoned a Rust unit test: a test built a temporary repository and ran
`git commit` inside it without first disabling signing. The test inherited the author's
global `commit.gpgsign` setting, hit the same agent-fill stall, and hung for roughly 60
seconds before failing — with a failure that looked like a slow or flaky test, not a
signing problem, until traced.

## Resolution

For a real commit: the human runs it themselves, or approves the 1Password prompt and the
operation is retried.

For tests: every test repository must disable signing explicitly —
`git config commit.gpgsign false` — as part of its setup, rather than relying on ambient
environment defaults it has no control over and no reason to assume.

## What it informed

No design or decision doc directly, since this is an environment fact rather than something
lane's own design could route around — a repository's signing configuration is the user's
choice and lane must not override it. Recorded so the test-setup requirement (disable
signing in every test fixture repo) isn't rediscovered the slow way again.
