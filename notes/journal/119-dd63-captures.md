# 119 — DD#63 captures: out-array form + replacement backrefs

Implemented the two DD#63 capture surfaces, all three backends, after
resolving the open design decisions with DG.

## Decisions (DG, this session)

- **D1 — out-array shape: bash-parity.** `match S P ARR` fills `ARR` with
  `[whole, g1..gN]` where `N` = the pattern's capturing-group count
  (array length `N+1`, like `BASH_REMATCH`). An interior group that exists
  but did not participate is the empty string (counted, not skipped). The
  engine can't report the group count and a non-participating group is
  indistinguishable from an absent one (both null `sp`), so this required
  surfacing the count from the engine.
- **D2 — the 3-arg form is still must-consume (B).** A bare `match S P ARR`
  is SB4018 / `- match …` is SB4019, exactly like the 2-arg boolean. (Task
  A3's draft wrote it bare; that predated DD#70. A3 rewritten.)
- **D3 — clear-on-entry.** Each call replaces `ARR`; a no-match empties it.
- **D4 — no taint extension.** A tainted subject can never reach `match`:
  reading a tainted value is rejected at the read site (SB5020) before
  `match` evaluates it. So captures are always clean — DD#61/SB5021
  unchanged, no new tainted-array source. Verified: `match "$x" P g` with a
  `getenv`/`argv`-tainted `$x` → SB5020.
- **D6 — SB4033** for an invalid `\`-escape in a `sub`/`gsub` replacement
  (only `\0`–`\9` and `\\` are valid; DD#64 removed shell backslash, so
  sash rejects the ambiguity rather than guessing, diverging from sed).

Also caught two **DD#64-staleness bugs in the task draft**: B2's
"single-quote → literal `.`" annotation is wrong (single-quote disables
*interpolation* only; `.` stays the regex any-char and matched by
coincidence — and DD#63 has no literal-regex mode); and C3's `"\\\\"`
source assumes the pre-DD#64 double-quote collapse (the DD#64-correct form
is `"\\"`).

## Engine change

`Reprog` gained an `int nsub` field (`regexp9.h`); `regcomp1` sets
`pp->nsub = cursubid` after `optimize()` (the group count it already
tracked but discarded). Shared by native, C, and the janet CLI.

**Build gotcha:** the `cc` crate didn't recompile `regcomp.c` after the
edit and `cargo clean -p sash-rt` didn't clear its OUT_DIR — the stale
object read `nsub` as 0, silently dropping capture groups. Fixed the root
cause: `sash-rt/build.rs` now emits `cargo:rerun-if-changed` for every
csrc file *and header*. (`rm -rf sash-rt/target/release/build/sash-rt-*`
forces it manually.)

## Implementation

- **Runtime** (`sash-rt/src/lib.rs`): `sash_regex_match_into` (clear table,
  read `nsub`, size `Resub` to `nsub+1`, fill `[0..nsub]` with heap-copied
  substrings, null group → `""`); `sash_table_clear`; `sash_regex_replace`
  now runs a backref template pass (`expand_replacement`).
- **SSA/transform**: new `SsaInst::MatchInto { subject, pattern, array }`,
  built by `try_handle_match_into` (3-arg form only; 2-arg and `$(match)`
  unchanged). Must-consume fires by kind at the statement level, so it
  covers the 3-arg form for free. Wired into the inst walkers (intern,
  var-scan, array-slot, ssa_taint non-defining).
- **Codegen** (native): `handle_match_into` resolves the array table
  (`find_array_slot`) and calls `sash_regex_match_into`; sets test
  result / `$?`.
- **Validate**: `check_regex_replacements` (SB4033) walks `$(sub/gsub …)`
  cmdsubs (all-modes, front-end) and rejects invalid replacement escapes.
- **C backend**: `emit_match_into_c` (shares the runtime; backrefs free via
  `sash_regex_replace`).
- **Janet backend**: new `sash-regex captures` CLI op (NUL-joined
  `[whole, g1..gN]`); `emit_janet_match_into_expr` spawns it,
  `string/split`s into the array, yields the boolean. Backref template
  added to the CLI's `sub`/`gsub`. Also fixed a pre-existing janet gap:
  `$(len ARR)` was exec'ing a nonexistent `len` command → now `(length …)`
  (improved janet_tests by 1).

## Spec / tests

`spec/regex.sh` 63.cap1–cap12 (out-array, bash-parity length, non-
participating groups, clear-on-entry, must-consume SB4018/SB4019, backrefs
`\1`/`\0`/`\\`, SB4033). Native 28/28; c & janet 24 + 4 native-only skips.
Runtime unit tests for `sash_regex_match_into` / backrefs were drafted but
the `sash-rt-tests` harness is **pre-existing broken** (references removed
`sash_streq`/`sash_arith_eval`; not on `mk test`'s path) — reverted; end-
to-end spec is the verification.

Full suite: native **0 failing**; c=58 / janet=106 unchanged (zero
regressions, authoritative spec.db diff). Cargo core suites: no new
failures, janet_tests +1.

## Left for later

Sugar desugaring (`[[ =~ ]]`→match, `s///`→sub/gsub), `\xnn`, field
interpolation, bare `[[ ]]` SB4018, and surfacing transform diagnostics on
`--emit-c`/`--janet` (SB4018/SB4019 still native-only).
