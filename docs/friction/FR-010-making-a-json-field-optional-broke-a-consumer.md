# FR-010: Making a `--json` field optional broke a consumer, silently and instantly

- Date: 2026-09-01

## What was hit

[ADR-016](../decisions/adr-016-the-disk-estimate-is-opt-in.md) made `disk_estimate_bytes`
absent from `-g --json` unless `--disk` is passed. Within minutes the Herdr lane picker
(`herdr-lanes`, in the user's dotfiles) stopped working: the popup opened and closed
instantly, with no visible error, because a popup's stderr disappears with the pane.

## Symptom

```
$ herdr-lanes
jq: error (at <stdin>:32): null (null) and number (1024) cannot be divided
```

The picker formats the column itself:

```jq
def disk($b):
    if   $b < 1048576 then "\(($b / 1024) | floor) KB"
    ...
```

jq sorts `null` below every number, so `null < 1048576` is **true** and the first branch runs
on a null. The script is `set -euo pipefail`, so jq's failure took the whole script down
before fzf drew anything.

## Resolution

The picker now decides for itself, on both sides: `HERDR_LANES_DISK=1` passes `--disk` and
renders the column; unset (the default) does neither, keeping the popup at lane's ~0.1 s.
`disk($b)` returns `"-"` for a null rather than formatting it.

The first attempt returned `""` for the absent value, which introduced a *second* bug: the
row is assembled as TSV and read with `IFS=$'\t'`, and a tab is IFS **whitespace**, so an
empty field between two tabs is swallowed and every later column shifts left by one. The
COMMITS column silently vanished. The script already carried a comment warning about exactly
this for its `$tab` field — the fix ignored a note left by the person who had already been
bitten. `"-"` keeps the field non-empty.

The dotfiles' generated `lane.fish` completions were regenerated while there; they had also
been missing `--refresh`, `-i/--info` and `-e`'s short form since before this change.

## What it informed

Nothing in lane's design changed — ADR-016's decision stands, and absence is still the right
encoding for "not measured". What this cost was entirely in the rollout:

- **A field going from always-present to conditional is a breaking change to every consumer**,
  even when the new shape is more honest. `-g --json` is documented as a contract for
  external tools; lane's own e2e suite asserts the field's absence, but nothing warned the
  consumer that already existed on this machine. Grepping for a field name across known
  callers costs a minute and would have caught it.
- **`null` is not a safe default in jq.** It compares below every number, so a guard written
  as a range check silently selects the wrong branch instead of failing at the comparison.
  A consumer's `// 0` or an explicit `== null` test is the only protection, and neither is
  the default thing to write.
- **A Herdr popup swallows stderr.** A script bound to a popup key that dies has no channel
  to say why; the failure presents as the popup flickering shut. The existing script pattern
  of `{ echo "..."; sleep 1; exit 0; }` for handled errors exists for that reason, and
  unhandled ones under `set -e` bypass it entirely.
