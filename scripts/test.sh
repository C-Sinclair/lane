#!/usr/bin/env bash
# End-to-end suite for `lane`: isolated, copy-on-write git worktrees. Covers the
# flag surface (--init, a bare name, --list, --exit, -d/-D, -i/--info, --prune,
# --shellenv, --completions). The clone layer and anchor-free worktree logic are
# covered in depth by `cargo test`.
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cargo build --quiet --manifest-path "$ROOT/crates/lane/Cargo.toml" || exit 1
TARGET=$(cargo metadata --no-deps --format-version 1 --manifest-path "$ROOT/crates/lane/Cargo.toml" \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
LANE="$TARGET/debug/lane"
[ -x "$LANE" ] || { echo "no binary at $LANE"; exit 1; }

TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
# Every lane invocation below registers its repo; keep that registry out of the real
# ~/.local/state/lane/repos so the suite never touches state that outlives it.
export XDG_STATE_HOME="$TMP/state"
pass=0; fail=0
ok()  { pass=$((pass+1)); echo "  ok   - $1"; }
bad() { fail=$((fail+1)); echo "  FAIL - $1"; }
is()  { if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (want '$3', got '$2')"; fi; }
# BSD sed wants an argument to -i, GNU sed refuses one; do the rename ourselves.
sedi() { local expr="$1"; shift; for f in "$@"; do sed "$expr" "$f" > "$f.sedi" && mv "$f.sedi" "$f"; done; }

setup() {
  cd "$TMP" && rm -rf repo && mkdir repo && cd repo
  git init -qb main . && git config user.email t@t.t && git config user.name t
  git config commit.gpgsign false
  mkdir -p src node_modules/pkg
  cat > src/auth.rs <<'EOF'
pub fn verify(token: &str) -> bool {
    parse(token).is_valid()
}
EOF
  head -c 2000000 /dev/urandom > node_modules/pkg/blob.bin
  printf 'node_modules/\n' > .gitignore
  git add -A && git commit -qm init
  "$LANE" --init > /dev/null
  git add -A && git commit -qm "lane init"
}

remote_setup() {
  rm -rf "$TMP/origin.git"
  git init --bare -q "$TMP/origin.git"
  git remote add origin "$TMP/origin.git"
}

echo "== 1. --init creates .lane/ and reports the reflink verdict =="
setup
is ".lane/ exists" "$([ -d .lane ] && echo yes)" "yes"
is "--init reports a reflink verdict" \
   "$("$LANE" --init | grep -c 'reflink')" "1"
is "--init writes no AGENTS.md" "$([ -f AGENTS.md ] && echo yes || echo no)" "no"

echo "== 1b. -g lists lanes across repositories, self-heals, and rejects conflicting flags =="
setup
# An empty registry, registered repos holding no lanes, and lanes to show are three
# different answers, and none of them may read as the local listing's "no lanes".
rm -rf "$XDG_STATE_HOME/lane"
is "-g says so when no repository is registered" \
   "$("$LANE" -g 2>&1 | grep -c '^no repositories registered')" "1"
is "and names how one gets registered" \
   "$("$LANE" -g 2>&1 | grep -c 'lane --init')" "1"
"$LANE" --init > /dev/null
is "-g distinguishes a registered repo with no lanes" \
   "$("$LANE" -g 2>&1 | grep -c 'no lanes in the 1 registered repository')" "1"
is "which is not the local listing's wording" \
   "$("$LANE" -g 2>&1 | grep -cx 'no lanes')" "0"
"$LANE" alpha-lane > /dev/null 2>&1

mkdir -p "$TMP/repo2" && cd "$TMP/repo2"
git init -qb main . > /dev/null && git config user.email t@t.t && git config user.name t
git config commit.gpgsign false
echo x > f && git add -A && git commit -qm base
"$LANE" --init > /dev/null
"$LANE" beta-lane > /dev/null 2>&1

is "-g lists a lane from the first repo" \
   "$("$LANE" -g | awk '$1 == "repo" && $2 == "alpha-lane" { n++ } END { print n + 0 }')" "1"
is "-g lists a lane from the second repo" \
   "$("$LANE" -g | awk '$1 == "repo2" && $2 == "beta-lane" { n++ } END { print n + 0 }')" "1"

REGISTRY="$XDG_STATE_HOME/lane/repos"
is "the registry recorded both repos" "$(wc -l < "$REGISTRY" | tr -d ' ')" "2"

rm -rf "$TMP/repo2"
is "-g drops a repo deleted from disk rather than erroring" \
   "$("$LANE" -g > /dev/null 2>&1; echo $?)" "0"
is "and rewrites the registry to drop it" "$(wc -l < "$REGISTRY" | tr -d ' ')" "1"

is "-g combined with --prune exits 2" \
   "$("$LANE" -g --prune > /dev/null 2>&1; echo $?)" "2"
is "and reports a usage error" "$("$LANE" -g --init 2>&1 | grep -c '^error:')" "1"

# An external tool acts on these rows, and `repo` is only a directory name: absolute
# paths are part of the contract, not a convenience.
is "-g --json carries absolute paths for both repo and lane" \
   "$("$LANE" -g --json | python3 -c 'import json,sys
d = json.load(sys.stdin)[0]
print(int(d["path"].startswith("/") and d["repo_path"].startswith("/") and d["path"].startswith(d["repo_path"])))')" "1"
is "-g --json is valid JSON with the expected fields" \
   "$("$LANE" -g --json | python3 -c 'import json,sys
d = json.load(sys.stdin)[0]
fields = ("repo", "repo_path", "lane", "path", "committed_at", "disk_estimate_bytes", "ahead", "behind", "state")
print(int(all(f in d for f in fields)))')" "1"

echo "== 1c. -g's own snapshot cache serves repeats, invalidates exactly, and --refresh bypasses it =="
setup
CACHE="$XDG_STATE_HOME/lane/cache.json"
"$LANE" cache-lane > /dev/null 2>&1
"$LANE" -g --json > /dev/null
is "-g writes its snapshot cache" "$([ -f "$CACHE" ] && echo yes || echo no)" "yes"

GEN1=$(python3 -c 'import json; print(json.load(open("'"$CACHE"'"))["generated_at"])')
"$LANE" -g --json > /dev/null
GEN2=$(python3 -c 'import json; print(json.load(open("'"$CACHE"'"))["generated_at"])')
is "a second -g is served from cache, not rewritten" "$GEN2" "$GEN1"

"$LANE" second-lane > /dev/null 2>&1
is "a lane created under a warm cache appears in -g immediately" \
   "$("$LANE" -g | awk '$2 == "second-lane" { n++ } END { print n + 0 }')" "1"

"$LANE" -d second-lane > /dev/null 2>&1
is "a lane deleted under a warm cache disappears from -g immediately" \
   "$("$LANE" -g | awk '$2 == "second-lane" { n++ } END { print n + 0 }')" "0"

"$LANE" landed-lane > /dev/null 2>&1
( cd "$TMP/repo/.lane/trees/landed-lane" && echo "fn m() {}" > src/m.rs \
  && git add -A && git commit -qm "landed work" > /dev/null )
git merge -q --no-edit landed-lane > /dev/null 2>&1
"$LANE" -g --json > /dev/null
"$LANE" --prune > /dev/null 2>&1
is "a --prune that removes a lane invalidates a warm cache" \
   "$("$LANE" -g | awk '$2 == "landed-lane" { n++ } END { print n + 0 }')" "0"

"$LANE" -g --json > /dev/null
GEN3=$(python3 -c 'import json; print(json.load(open("'"$CACHE"'"))["generated_at"])')
sleep 1
"$LANE" -g --refresh --json > /dev/null
GEN4=$(python3 -c 'import json; print(json.load(open("'"$CACHE"'"))["generated_at"])')
is "--refresh rewrites generated_at rather than serving the cache" \
   "$([ "$GEN4" -gt "$GEN3" ] && echo yes || echo no)" "yes"

is "--refresh without -g exits 2" "$("$LANE" --refresh > /dev/null 2>&1; echo $?)" "2"
is "and reports a usage error" "$("$LANE" --refresh 2>&1 | grep -c '^error:')" "1"

rm -f "$CACHE"
FRESH=$("$LANE" -g --json)
CACHED=$("$LANE" -g --json)
is "cached and freshly computed -g --json are byte-identical" \
   "$([ "$FRESH" = "$CACHED" ] && echo yes || echo no)" "yes"

echo "== 2. a bare name: warm cache arrives, tracked files from git, status clean =="
setup
is "--list json is an empty array before a lane exists" \
   "$("$LANE" --list --json | python3 -c 'import json,sys; print(json.load(sys.stdin) == [])')" "True"
"$LANE" fix-login > /tmp/new.out 2>&1
LP="$TMP/repo/.lane/trees/fix-login"
LP_REAL=$(cd "$LP" && pwd -P)
REFLINK=$(grep -c 'reflink: yes' /tmp/new.out)
want() { [ "$REFLINK" = "1" ] && echo yes || echo no; }
is "lane exists" "$([ -d "$LP" ] && echo yes)" "yes"
is "warm dir present in lane iff reflink" \
   "$([ -f "$LP/node_modules/pkg/blob.bin" ] && echo yes || echo no)" "$(want)"
is "tracked file present" "$([ -f "$LP/src/auth.rs" ] && echo yes)" "yes"
is "lane status clean" "$(git -C "$LP" status --porcelain | wc -l | tr -d ' ')" "0"
is "the list prints the lane name once" \
   "$("$LANE" | awk '$1 == "fix-login" && $2 == "open" && $3 == "clean" { n++ } END { print n + 0 }')" "1"
is "--list json reports the exact clean open lane row" \
   "$("$LANE" --json | python3 -c 'import json,sys; d=json.load(sys.stdin)[0]; print(int(d["name"]=="fix-login" and d["path"]==sys.argv[1] and d["branch"]=="fix-login" and d["state"]=="open" and d["dirty"] is False))' "$LP_REAL")" "1"
is "reflink verdict reported" "$(grep -c 'reflink:' /tmp/new.out)" "1"
is "tracked files not re-cloned" \
   "$(grep -o 'cloned' /tmp/new.out | head -1)" "cloned"

echo "== 3. dirty mode carries dirty state without rewriting files =="
setup
echo "// scratch work" >> src/auth.rs
"$LANE" spike --dirty > /tmp/dirty.out 2>&1
LP="$TMP/repo/.lane/trees/spike"
REFLINK=$(grep -c 'reflink: yes' /tmp/dirty.out)
want() { [ "$REFLINK" = "1" ] && echo yes || echo no; }
# --dirty honours the flag on any filesystem: reflinked whole-tree with it, copied without.
is "dirty change carried" \
   "$(grep -q 'scratch work' "$LP/src/auth.rs" && echo yes || echo no)" "yes"
is "dirty mode reports what it carried" \
   "$(grep -q 'carried' /tmp/dirty.out && echo yes || echo no)" "yes"
is "exactly one modified file, not the whole tree" \
   "$(git -C "$LP" status --porcelain | grep -c '^ M')" "1"
is "--json reports the dirty spike row" \
   "$("$LANE" --json | python3 -c 'import json,sys; print(sum(r["name"] == "spike" and r["dirty"] is True for r in json.load(sys.stdin)))')" "1"
is "warm dir also carried iff reflink" \
   "$([ -f "$LP/node_modules/pkg/blob.bin" ] && echo yes || echo no)" "$(want)"

echo "== 4. a lane can adopt a branch that already exists =="
setup
git branch review-me
"$LANE" review-me > /dev/null 2>&1
is "the lane is on the existing branch" \
   "$(git -C .lane/trees/review-me rev-parse --abbrev-ref HEAD)" "review-me"
is "no second branch was made" "$(git branch --list 'review-me*' | wc -l | tr -d ' ')" "1"
"$LANE" -D review-me > /dev/null 2>&1
git branch -D review-me > /dev/null 2>&1
git branch taken
is "--base is refused for an existing branch" \
   "$("$LANE" taken --base main 2>&1 | grep -c '^error:')" "1"

echo "== 5. a lane lives inside the repo and survives a move =="
setup
RELATIVE_PATHS=$(git worktree add -h 2>&1 | grep -c 'relative-paths')
"$LANE" moved > /dev/null 2>&1
is "the lane is inside the repo" \
   "$([ -d .lane/trees/moved ] && echo yes || echo no)" "yes"
is "its gitdir pointer is relative when git supports it" \
   "$(grep -c '^gitdir: \.\.' .lane/trees/moved/.git)" "$RELATIVE_PATHS"
is "the main worktree stays clean" "$(git status --porcelain)" ""
( cd "$TMP" && mv repo moved-repo )
MOVED_BRANCH=$(cd "$TMP/moved-repo/.lane/trees/moved" \
  && git rev-parse --abbrev-ref HEAD 2>/dev/null || true)
is "git still works after a move when relative paths are supported" \
   "$MOVED_BRANCH" "$([ "$RELATIVE_PATHS" = "1" ] && echo moved || true)"
( cd "$TMP" && mv moved-repo repo )

echo "== 6. dirty lanes do not carry sibling lanes =="
setup
"$LANE" first > /dev/null 2>&1
"$LANE" second > /dev/null 2>&1
echo "// scratch" >> src/auth.rs
"$LANE" dirty-third --dirty > /dev/null 2>&1
is "a dirty lane contains no other lanes" \
   "$([ -e .lane/trees/dirty-third/.lane/trees ] && echo yes || echo no)" "no"

echo "== 7. a bare name and --exit move the shell =="
setup
"$LANE" here > /dev/null 2>&1
LP_REAL=$(cd .lane/trees/here && pwd -P)
is "a bare name against an existing lane prints its path" "$("$LANE" here)" "$LP_REAL"
ROOT_REAL=$(pwd -P)
is "--exit prints the repo root" "$("$LANE" --exit)" "$ROOT_REAL"
is "-e is --exit, from inside the lane" \
   "$(cd "$LP_REAL" && "$LANE" -e)" "$ROOT_REAL"
is "entering without shell integration warns on a terminal only" \
   "$("$LANE" here 2>&1 >/dev/null | grep -c 'shell integration')" "0"

echo "== 8. --shellenv prints the shell wrapper =="
is "--shellenv defines a lane() function" "$("$LANE" --shellenv | grep -c '^lane() {')" "1"
is "-e is an alias for --exit in both wrappers" \
   "$("$LANE" --shellenv posix | grep -c -- '-e|--exit')" "1"
is "and in the fish wrapper's cd branch" \
   "$("$LANE" --shellenv fish | grep -c -- "case '-e' '--exit'")" "1"
is "--exit is matched before the general dash case" \
   "$("$LANE" --shellenv | grep -c -- '--exit)')" "1"
is "an empty or dashed first word never cds" \
   "$("$LANE" --shellenv | grep -c '""|-\*)')" "1"

echo "== 9. -d/-D refuse before they destroy, and -D means everything =="
setup
"$LANE" scrap > /dev/null 2>&1
( cd "$TMP/repo/.lane/trees/scrap" && echo "fn work() {}" > src/work.rs \
  && git add -A && git commit -qm "unlanded work" > /dev/null )
SHA=$(git rev-parse scrap)
"$LANE" -d scrap > /tmp/rm.out 2>&1
is "-d exits non-zero when it kept the lane" "$?" "1"
is "-d names the commits at stake" \
   "$(grep -c 'kept lane scrap: commits main does not have' /tmp/rm.out)" "1"
is "-d names the way through" "$(grep -c -- '-D scrap' /tmp/rm.out)" "1"
is "unlanded branch survives" "$(git branch --list scrap | wc -l | tr -d ' ')" "1"
is "unlanded commit still reachable" "$(git rev-parse scrap)" "$SHA"
is "the refused lane is still on disk" \
   "$([ -f "$TMP/repo/.lane/trees/scrap/src/work.rs" ] && echo yes || echo no)" "yes"
"$LANE" -D scrap > /tmp/force.out 2>&1
is "-D exits zero" "$?" "0"
is "-D discards the branch" "$(git branch --list scrap | wc -l | tr -d ' ')" "0"
is "-D discards the worktree" \
   "$([ -d "$TMP/repo/.lane/trees/scrap" ] && echo yes || echo no)" "no"

"$LANE" edited > /dev/null 2>&1
echo "// scratch" >> "$TMP/repo/.lane/trees/edited/src/auth.rs"
"$LANE" -d edited > /tmp/edited.out 2>&1
is "-d counts uncommitted work as loss" \
   "$(grep -c 'kept lane edited: 1 uncommitted change(s)' /tmp/edited.out)" "1"
"$LANE" -D edited > /dev/null 2>&1
is "-D takes the edit too" \
   "$([ -d "$TMP/repo/.lane/trees/edited" ] && echo yes || echo no)" "no"

# A clean lane holding nothing trunk lacks costs nothing, so no -D is asked for.
"$LANE" empty > /dev/null 2>&1
"$LANE" -d empty > /tmp/empty.out 2>&1
is "-d takes a lane with nothing at stake" "$?" "0"
is "and says so" "$(grep -c 'removed lane empty' /tmp/empty.out)" "1"

# The squash merge git branch -d always refuses. Lane compares patches, not ancestry.
"$LANE" squashed > /dev/null 2>&1
( cd "$TMP/repo/.lane/trees/squashed" && echo "fn s() {}" > src/s.rs \
  && git add -A && git commit -qm "squash me" > /dev/null )
git merge --squash squashed > /dev/null 2>&1 && git commit -qm "squashed (#1)"
is "git itself refuses the squashed branch" \
   "$(git branch -d squashed > /dev/null 2>&1 && echo deleted || echo refused)" "refused"
"$LANE" -d squashed > /tmp/squash.out 2>&1
is "-d sees the squash and needs no -D" "$?" "0"
is "the squashed branch is gone" "$(git branch --list squashed | wc -l | tr -d ' ')" "0"

# A rebase merge replays each commit on its own, so a branch of several is landed by
# patch while no single collapsed diff matches.
"$LANE" replayed > /dev/null 2>&1
( cd "$TMP/repo/.lane/trees/replayed" \
  && echo "fn one() {}" > src/one.rs && git add -A && git commit -qm one > /dev/null \
  && echo "fn two() {}" > src/two.rs && git add -A && git commit -qm two > /dev/null )
git cherry-pick "$(git merge-base main replayed)..replayed" > /dev/null 2>&1
echo "// moved on" >> src/auth.rs && git add -A && git commit -qm "trunk moved" > /dev/null
is "git itself refuses the replayed branch" \
   "$(git branch -d replayed > /dev/null 2>&1 && echo deleted || echo refused)" "refused"
"$LANE" -d replayed > /tmp/replay.out 2>&1
is "-d sees a multi-commit rebase merge" "$?" "0"
is "the replayed branch is gone" "$(git branch --list replayed | wc -l | tr -d ' ')" "0"

"$LANE" -d ghost > /tmp/ghost.out 2>&1
is "-d rejects a name that is neither lane nor branch" "$?" "1"
is "-d says so plainly" "$(grep -c 'no lane ghost' /tmp/ghost.out)" "1"

# A worktree deleted by hand leaves a branch git will not remove a worktree for.
"$LANE" gone > /dev/null 2>&1
rm -rf "$TMP/repo/.lane/trees/gone"
"$LANE" -D gone > /tmp/gone.out 2>&1
is "-D cleans up after a hand-deleted worktree" "$?" "0"
is "with no git fatal" "$(grep -c 'not a working tree' /tmp/gone.out)" "0"
is "and the branch goes with it" "$(git branch --list gone | wc -l | tr -d ' ')" "0"

echo "== 9b. -d takes several names in one call =="
setup
"$LANE" alpha > /dev/null 2>&1
"$LANE" beta > /dev/null 2>&1
"$LANE" -d alpha beta > /tmp/multi.out 2>&1
is "-d with several names exits zero when all go cleanly" "$?" "0"
is "-d removed the first" "$(grep -c 'removed lane alpha' /tmp/multi.out)" "1"
is "-d removed the second" "$(grep -c 'removed lane beta' /tmp/multi.out)" "1"
is "both worktrees are gone" \
   "$([ -d .lane/trees/alpha ] && echo yes || [ -d .lane/trees/beta ] && echo yes || echo no)" "no"

"$LANE" clean-one > /dev/null 2>&1
"$LANE" dirty-one > /dev/null 2>&1
( cd "$TMP/repo/.lane/trees/dirty-one" && echo "fn work() {}" > src/work.rs \
  && git add -A && git commit -qm "unlanded work" > /dev/null )
"$LANE" -d clean-one dirty-one > /tmp/multi2.out 2>&1
is "-d with several names exits non-zero if any was kept" "$?" "1"
is "the clean one still went" "$(grep -c 'removed lane clean-one' /tmp/multi2.out)" "1"
is "the dirty one was kept" "$(grep -c 'kept lane dirty-one' /tmp/multi2.out)" "1"

echo "== 10. --prune collects a lane whose branch trunk already contains =="
setup
"$LANE" merged > /dev/null 2>&1
( cd "$TMP/repo/.lane/trees/merged" && echo "fn m() {}" > src/m.rs \
  && git add -A && git commit -qm "merged work" > /dev/null )
git merge -q --no-edit merged > /dev/null 2>&1
is "--prune collects the lane once trunk contains its work" \
   "$("$LANE" --prune 2>&1 | grep -c '^removed merged')" "1"
is "the lane is gone" "$([ -d .lane/trees/merged ] && echo yes || echo no)" "no"

echo "== 11. --prune collects a lane whose upstream was retired =="
setup
remote_setup
git push -q origin main
"$LANE" pushed-away > /dev/null 2>&1
( cd "$TMP/repo/.lane/trees/pushed-away" && echo "fn p() {}" > src/p.rs \
  && git add -A && git commit -qm "pushed work" \
  && git push -q -u origin pushed-away )
# The remote retires the branch on its own side, exactly as a merged pull request does.
git --git-dir="$TMP/origin.git" branch -q -D pushed-away
git fetch -q --prune
is "a retired upstream reads as landed" \
   "$("$LANE" --prune 2>&1 | grep -c '^removed pushed-away')" "1"
is "the lane is gone" "$([ -d .lane/trees/pushed-away ] && echo yes || echo no)" "no"

echo "== 12. --prune keeps an open lane =="
setup
"$LANE" open-work > /dev/null 2>&1
( cd "$TMP/repo/.lane/trees/open-work" && echo "fn o() {}" > src/o.rs \
  && git add -A && git commit -qm "open work" > /dev/null )
# An open lane is not a candidate, so prune passes over it without comment: every lane
# still being worked in would otherwise report itself on every run.
is "--prune keeps a lane whose work trunk does not have" \
   "$("$LANE" --prune 2>&1 | grep -c '^no landed lanes')" "1"
is "the lane survives" "$([ -d .lane/trees/open-work ] && echo yes || echo no)" "yes"

"$LANE" dry-landed > /dev/null 2>&1
( cd "$TMP/repo/.lane/trees/dry-landed" && echo x > src/x.rs && git add -A && git commit -qm x > /dev/null )
git merge -q --no-edit dry-landed > /dev/null 2>&1
is "--dry-run reports without removing" \
   "$("$LANE" --prune --dry-run 2>&1 | grep -c '^would remove dry-landed')" "1"
is "--dry-run left the lane in place" "$([ -d .lane/trees/dry-landed ] && echo yes || echo no)" "yes"

echo "== 13. lane state lives in refs, not config =="
setup
"$LANE" stateful > /dev/null 2>&1
is "the fork point is a ref" \
   "$(git for-each-ref --format='%(refname)' refs/lane/ | grep -c '^refs/lane/stateful$')" "1"
is "and nothing is left in config" "$(git config --local --list | grep -c '^lane\.stateful\.' || true)" "0"
"$LANE" -D stateful > /dev/null 2>&1
is "removing the lane removes its ref" "$(git for-each-ref refs/lane/ | wc -l | tr -d ' ')" "0"

# A config value naming a commit is opaque to git: rewrite the base and gc collects it,
# taking the marker with it. A ref keeps the commit reachable, so the marker survives.
setup
echo "fn v2() {}" > src/v2.rs && git add -A && git commit -qm "base moves on" > /dev/null
"$LANE" gc-proof > /dev/null 2>&1
fork=$(git rev-parse refs/lane/gc-proof)
git reset -q --hard HEAD~1
git reflog expire --expire=now --all && git gc -q --prune=now 2>/dev/null
is "the fork commit survives an aggressive gc" \
   "$(git cat-file -e "$fork" 2>/dev/null && echo yes || echo no)" "yes"
is "so an untouched lane is still not landed" \
   "$("$LANE" --prune --dry-run 2>&1 | grep -c '^no landed lanes')" "1"

echo "== 14. a bare lane name creates or enters, and a lane may be named ls or prune =="
setup
"$LANE" fix-login > /tmp/open-new.out 2>&1
LP="$TMP/repo/.lane/trees/fix-login"
is "a bare name creates the lane" "$([ -d "$LP" ] && echo yes)" "yes"
LP_REAL=$(cd "$LP" && pwd -P)
is "a second bare name against an existing lane prints the same path" \
   "$("$LANE" fix-login)" "$LP_REAL"
"$LANE" hotfix --base main > /tmp/open-base.out 2>&1
is "--base works on create" \
   "$([ -d "$TMP/repo/.lane/trees/hotfix" ] && echo yes)" "yes"
is "--base against an existing lane is refused" \
   "$("$LANE" hotfix --base main 2>&1 | grep -c '^error:')" "1"

# The collision this whole change removes: a lane may take the name of a former subcommand.
"$LANE" ls > /dev/null 2>&1
is "a lane named ls can be created" "$([ -d "$TMP/repo/.lane/trees/ls" ] && echo yes)" "yes"
LS_REAL=$(cd "$TMP/repo/.lane/trees/ls" && pwd -P)
is "a lane named ls can be entered" "$("$LANE" ls)" "$LS_REAL"
"$LANE" -D ls > /dev/null 2>&1
is "a lane named ls can be deleted" "$([ -d "$TMP/repo/.lane/trees/ls" ] && echo yes || echo no)" "no"

"$LANE" prune > /dev/null 2>&1
is "a lane named prune can be created" "$([ -d "$TMP/repo/.lane/trees/prune" ] && echo yes)" "yes"
PRUNE_REAL=$(cd "$TMP/repo/.lane/trees/prune" && pwd -P)
is "a lane named prune can be entered" "$("$LANE" prune)" "$PRUNE_REAL"
"$LANE" -D prune > /dev/null 2>&1
is "a lane named prune can be deleted" "$([ -d "$TMP/repo/.lane/trees/prune" ] && echo yes || echo no)" "no"

echo "== 14b. conflicting operation flags are a usage error =="
is "combining operation flags is refused" \
   "$("$LANE" --prune --init 2>&1 | grep -c '^error:')" "1"
is "and exits 2" "$("$LANE" --prune --init > /dev/null 2>&1; echo $?)" "2"
is "-d combined with --prune is refused" \
   "$("$LANE" -d somelane --prune 2>&1 | grep -c '^error:')" "1"
is "and exits 2" "$("$LANE" -d somelane --prune > /dev/null 2>&1; echo $?)" "2"

echo "== 14c. a name beginning with a dash is reachable after -- =="
setup
# git itself refuses a branch name that looks like one of its own flags, so this cannot
# succeed end to end; what matters here is that lane's own parser reads it as a name
# rather than rejecting it as a flag, and gets as far as handing it to git.
"$LANE" -- -dash-name > /tmp/dash.out 2>&1
is "the parser does not reject a dashed name after --" \
   "$(grep -c 'unexpected argument' /tmp/dash.out)" "0"
is "it fails deeper, in git, not as a usage error" \
   "$("$LANE" -- -dash-name > /dev/null 2>&1; echo $?)" "1"
"$LANE" -d -- -dash-name > /tmp/dash-d.out 2>&1
is "-d -- -dash-name is likewise read as a name, not a flag" \
   "$(grep -c 'unexpected argument' /tmp/dash-d.out)" "0"

echo "== 15. --shellenv and --completions cover fish, bash, zsh =="
is "--shellenv defaults to the posix function form" \
   "$("$LANE" --shellenv posix | grep -c '^lane() {')" "1"
is "--shellenv fish emits a fish function" \
   "$("$LANE" --shellenv fish | grep -c '^function lane')" "1"
is "--shellenv fish emits no posix esac" \
   "$("$LANE" --shellenv fish | grep -c 'esac')" "0"
is "an unknown --shellenv shell is refused" \
   "$("$LANE" --shellenv csh 2>&1 | grep -c '^error:')" "1"
for shell in fish bash zsh; do
  is "--completions $shell emits something" \
     "$([ -n "$("$LANE" --completions "$shell")" ] && echo yes)" "yes"
done
is "an unknown --completions shell is refused" \
   "$("$LANE" --completions csh 2>&1 | grep -c '^error:')" "1"

FISH="/usr/local/bin/fish"
if [ -x "$FISH" ]; then
  echo "== 16. the fish wrapper actually works under fish =="
  BINDIR="$(dirname "$LANE")"
  fishrc="set -x PATH $BINDIR \$PATH; eval (lane --shellenv fish | string collect)"
  is "the fish wrapper parses" \
     "$("$FISH" -c "$fishrc" 2>&1; echo $?)" "0"
  setup
  is "no args does not cd under fish" \
     "$("$FISH" -c "$fishrc; and lane > /dev/null; and pwd -P")" \
     "$(pwd -P)"
  is "--prune does not cd under fish" \
     "$("$FISH" -c "$fishrc; and lane --prune > /dev/null; and pwd -P")" \
     "$(pwd -P)"
  is "a bare name cds under fish" \
     "$("$FISH" -c "$fishrc; lane fish-lane > /dev/null; pwd -P")" \
     "$(cd .lane/trees/fish-lane && pwd -P)"
  is "--exit cds back under fish" \
     "$("$FISH" -c "$fishrc; lane fish-lane > /dev/null; lane --exit > /dev/null; pwd -P")" \
     "$(pwd -P)"
  is "a failing invocation does not cd under fish" \
     "$("$FISH" -c "$fishrc; lane -d ghost > /dev/null 2>&1; pwd -P")" \
     "$(pwd -P)"
else
  echo "== 16. fish not found at $FISH, skipping =="
fi

echo "== 17. -i/--info describes one lane =="
setup
"$LANE" feat-login > /dev/null 2>&1
LP="$TMP/repo/.lane/trees/feat-login"

is "-i <name> from the repo root names that lane" \
   "$("$LANE" -i feat-login | awk '$1 == "lane" { print $2 }')" "feat-login"
is "-i <name> reports the branch" \
   "$("$LANE" -i feat-login | awk '$1 == "branch" { print $2 }')" "feat-login"
is "-i <name> reports the state" \
   "$("$LANE" -i feat-login | awk '$1 == "state" { print $2 }')" "open"
is "-i <name> reports a fork point" \
   "$("$LANE" -i feat-login | grep -c '^forked from  ')" "1"
is "-i <name> reports the trunk" \
   "$("$LANE" -i feat-login | awk '$1 == "trunk" { print $2 }')" "main"

is "-i inside a lane names that lane with no argument" \
   "$(cd "$LP" && "$LANE" -i | awk '$1 == "lane" { print $2 }')" "feat-login"

is "-i with no name and no lane underfoot exits 1" \
   "$("$LANE" -i > /dev/null 2>&1; echo $?)" "1"
is "and tells the reader to name one" \
   "$("$LANE" -i 2>&1 | grep -c 'name one')" "1"

is "-i on an unknown name errors" \
   "$("$LANE" -i ghost 2>&1 | grep -c 'no lane named ghost')" "1"
is "and exits 1" "$("$LANE" -i ghost > /dev/null 2>&1; echo $?)" "1"

is "-i --json is valid JSON carrying the expected keys" \
   "$("$LANE" -i feat-login --json | python3 -c 'import json,sys
d = json.load(sys.stdin)
fields = ("lane", "branch", "path", "state", "created_at", "fork_point", "trunk",
          "trunk_ahead", "trunk_behind", "committed_at", "last_commit",
          "upstream", "upstream_ahead", "upstream_behind", "uncommitted",
          "disk_estimate_bytes")
print(int(all(f in d for f in fields)))')" "1"
is "a lane this tool created has a recorded fork point in JSON too" \
   "$("$LANE" -i feat-login --json | python3 -c 'import json,sys; print(json.load(sys.stdin)["fork_point"] is not None)')" "True"

( cd "$LP" && echo "fn work() {}" > src/work.rs && echo "fn work2() {}" > src/work2.rs )
is "-i reports the uncommitted file count" \
   "$("$LANE" -i feat-login | awk '$1 == "uncommitted" { print $2, $3 }')" "2 files"
is "and the same count in --json" \
   "$("$LANE" -i feat-login --json | python3 -c 'import json,sys; print(json.load(sys.stdin)["uncommitted"])')" "2"

is "-i with a conflicting operation flag exits 2" \
   "$("$LANE" -i feat-login --prune > /dev/null 2>&1; echo $?)" "2"
is "and reports a usage error" "$("$LANE" -i --prune 2>&1 | grep -c '^error:')" "1"

echo "== 18. a lane named after a remote branch continues it =="
setup
remote_setup
git push -q origin main
git checkout -qb published
echo published > published.txt && git add -A && git commit -qm "work already on the remote"
git push -q origin published
git checkout -q main
git branch -qD published
is "the branch is gone locally" \
   "$(git rev-parse --verify --quiet refs/heads/published > /dev/null; echo $?)" "1"
"$LANE" published > /dev/null 2>&1
cd .lane/trees/published
is "the lane sits on the published commit" \
   "$(git log --oneline -1 --format=%s)" "work already on the remote"
is "and tracks the remote branch" "$(git rev-parse --abbrev-ref '@{upstream}')" "origin/published"
cd ../../..
is "a name nothing publishes still forks from HEAD" \
   "$("$LANE" fresh > /dev/null 2>&1; git -C .lane/trees/fresh log --oneline -1 --format=%s)" "lane init"
is "and has no upstream" \
   "$(git -C .lane/trees/fresh rev-parse --abbrev-ref '@{upstream}' 2>/dev/null; echo $?)" "128"
is "--base overrides the upstream match" \
   "$("$LANE" published2 --base main > /dev/null 2>&1; echo $?)" "0"

echo "== 19. an ignored directory holding other checkouts is not cloned =="
setup
printf 'node_modules/\n.wt/\n' > .gitignore
git add -A && git commit -qm "ignore .wt"
git worktree add -q -b side .wt/side
head -c 3000000 /dev/urandom > .wt/side/big.bin
"$LANE" isolated > /dev/null 2>&1
is ".wt/ was not cloned into the lane" \
   "$([ -e .lane/trees/isolated/.wt ] && echo yes || echo no)" "no"
is "the real cache still was" \
   "$([ -f .lane/trees/isolated/node_modules/pkg/blob.bin ] && echo yes || echo no)" "yes"
"$LANE" second > /dev/null 2>&1
is "and a sibling lane is not cloned into the next one" \
   "$([ -e .lane/trees/second/.lane/trees/isolated ] && echo yes || echo no)" "no"

echo "== 20. worktrees lane did not create are not lanes =="
setup
printf 'node_modules/\n.wt/\n' > .gitignore
git add -A && git commit -qm "ignore .wt"
# A foreign worktree, on a branch already merged into trunk: exactly what --prune collects.
git worktree add -q -b foreign .wt/foreign
git -C .wt/foreign commit -q --allow-empty -m "foreign work"
git merge -q --no-edit foreign
"$LANE" mine > /dev/null 2>&1
is "--list shows only the lane" "$("$LANE" --list | wc -l | tr -d ' ')" "1"
is "and names it, not its path" "$("$LANE" --list | awk '{print $1}')" "mine"
is "-g counts only the lane" \
   "$("$LANE" -g --refresh --json | grep -c '"lane": "mine"')" "1"
is "-g does not list the foreign worktree" \
   "$("$LANE" -g --refresh --json | grep -c 'foreign')" "0"
is "--prune leaves the foreign worktree alone" \
   "$("$LANE" --prune --dry-run 2>&1 | grep -c foreign)" "0"
"$LANE" --prune > /dev/null 2>&1
is "and it is still checked out afterwards" \
   "$([ -f .wt/foreign/src/auth.rs ] && echo yes || echo no)" "yes"
is "with its branch intact" \
   "$(git rev-parse --verify --quiet refs/heads/foreign > /dev/null; echo $?)" "0"

echo
echo "$pass passed, $fail failed"
[ "$fail" -eq 0 ]
