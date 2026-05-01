# Design Decision #20: Shell options are language definition, not runtime toggles

**Date:** 2025-04-15

**Principle:** `shopt` and `set` commands don't exist in safebash. Shell options
aren't configurable — they're part of the language definition. There's nothing
to toggle.

### shopt options (all resolved at compile time)

**Always on:**
- `extglob` — strictly more expressive pattern matching, no downside
- `lastpipe` — last pipeline command in current shell, the alternative is a footgun
- `globstar` — `**` recursive matching, useful, no downside
- `inherit_errexit` — command substitutions inherit error checking
- `shift_verbose` — `shift` past end of args is an error, not silent
- `assoc_expand_once` — double expansion is a bug source

**Context-sensitive glob behavior:**
- **Iteration contexts** (`for f in *.txt`): nullglob — no matches → skip body
- **Command argument contexts** (`rm *.bak`): failglob — no matches → error
- The compiler knows the context and applies the right semantics

**Off:**
- `dotglob` — `*` skipping hidden files is the safe default
- `nocaseglob` / `nocasematch` — Linux is case-sensitive, case-insensitive matching surprises
- `localvar_inherit` — implicit inheritance is confusing
- `localvar_unset` — confusing semantics
- `xpg_echo` — implicit backslash interpretation surprises; `echo -e` is explicit

**Irrelevant:** All interactive/history options — safebash compiles scripts, not sessions.

### set options

**`set -e` (errexit): doesn't exist.**
The compiler enforces exit checking statically. Every command's exit code must
be explicitly handled — checked (`if cmd`, `cmd || handle`), ignored (`cmd || :`),
or in a propagating context (pipeline). Unchecked commands are compile-time errors.
This is strictly safer than `set -e`, which has known blind spots (conditions,
`&&`/`||` chains, function bodies called from `if`).

**`set -u` (nounset): doesn't exist.**
The type system and variable tracking enforce declarations at compile time.
Referencing an undeclared variable is a compile-time error.

**`set -o pipefail`: always on.**
Ignoring upstream pipeline failures is wrong. The practical conflict with `grep`
(exit 1 = no matches, not an error) is handled by the coerce tool (DD#8):
the coercer inserts `|| :` on non-terminal pipeline segments where nonzero exit
is informational, not fatal. The programmer reviews the diff.

**`set -x` (xtrace): compile-time `--trace` flag.**
Not a runtime toggle. `safebash --trace script.sh` emits trace instrumentation
in the compiled binary. Production builds omit it.

**`set -C` (noclobber): off.**
Too surprising as default. Handle case-by-case via safe-write patterns.

**`set -f` (noglob): off.**
Globs are safe via context-sensitive nullglob/failglob.

### Coerce tool (DD#8) for migration

The coerce tool (`--coerce`) takes a bash script and outputs the safebash-ready
version, enabling easy diffs and quick migration:

1. Parse the bash script
2. Identify unchecked commands and pipeline segments
3. Insert `|| :` where exit codes are informational, not fatal
4. Remove `set -e`, `set -u`, `shopt` calls (now implicit)
5. Output the coerced version
6. Programmer diffs, adjusts, commits

The coercer is conservative — it marks everything it's unsure about and the
programmer decides.

