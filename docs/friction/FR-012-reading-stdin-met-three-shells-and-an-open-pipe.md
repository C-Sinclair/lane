# FR-012: Reading stdin met three shells and an open pipe

- Date: 2026-09-26

## What was hit

Adding names read from stdin ([ADR-018](../decisions/adr-018-names-read-from-stdin.md)) turned up three problems, each outside lane's own code.

1. The end-to-end suite hung when run as a background job. `scripts/test.sh` calls a bare `"$LANE"` many times, and the job runner left stdin as a pipe that was open and never written to. `read_to_string` on it waits forever.
2. In fish, `echo two | lane` did not open `two`. Fish gives a command substitution the shell's stdin, not the stdin of the function it runs in, so `set -l p (command lane $argv)` never saw the pipe.
3. In bash, `echo two | lane` created the lane but left the shell where it was. Bash runs every element of a pipeline in a subshell, the last one included, so the wrapper's `cd` happened in a process that exited straight away.

## Symptom

```
$ ps -o pid,stat,etime,command
 1967 S      02:11 …/target/debug/lane
 1968 S      02:11 awk $1 == "fix-login" && … { print n + 0 }
```

## Resolution

1. `cli::piped_names` polls stdin for `PIPE_WAIT` before reading, and reads nothing if it stays silent. `scripts/test.sh` also starts with `exec < /dev/null`, so its result does not depend on how it was launched.
2. The fish wrapper captures with `command lane $argv | read -lz out`. The first element of a pipeline inside a function does get the function's stdin, and `read` runs in the current shell, so the variable survives.
3. No fix in lane. `shopt -s lastpipe` only applies when job control is off, which excludes interactive bash. DD-004 and the readme tell bash users to write `lane "$(…)"`.

## What it informed

[ADR-018](../decisions/adr-018-names-read-from-stdin.md), [DD-004](../design/dd-004-shell-integration.md).
