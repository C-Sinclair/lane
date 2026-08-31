# FR-004: zsh completion scripts cannot be sourced

- Date: 2026-08-31

## What was hit

`source <(lane --completions zsh)`, the pattern that works for bash and fish, fails for
zsh.

## Symptom

```
compadd: can only be called from completion function
```

The zsh script `lane --completions zsh` prints is an autoloaded `#compdef` function. zsh's
completion system only runs such a function when it's loaded through its autoload mechanism
— found as a file named `_lane` somewhere on `$fpath` — not when its body is executed
directly via `source`. This looked like a broken completion script; it wasn't. The script
was correct for the mechanism zsh actually uses.

## Resolution

Document the difference instead of trying to make the script sourceable: install it as a
file named `_lane` on `$fpath`:

```sh
lane --completions zsh > ~/.zsh/completions/_lane
```

This is now stated directly in the readme, next to the bash and fish install lines, rather
than left implicit.

## What it informed

DD-005 (CLI surface) notes this in passing as the reason the zsh completion script needs
different install instructions from the other two shells.
