# Session Handoff — 2026-04-25/26/27

## What happened

Two marathon sessions: DD#47-52, adversarial audit run-2, SSA taint
backstop, 17 codegen/runtime improvements, Brush compatibility suite.

**Start:** 457 tests, 45 design decisions
**End:** 590 tests, 52 design decisions, 20 commits

## Commits (newest first)

```
6d4ec30 codegen: [[ -v var ]], pipeline exit status, nested return, div-by-zero guard
c827964 codegen: func redirect, ${#arr[N]}, ${x:(-5)}, regex quantifier scan
8b1e233 codegen: $RANDOM, unset, command -v builtins, SB4015 @-transforms
7489d4c sash-rt: command -v returns builtin names for shell builtins
db84b0d brush_compat: add SHOULD_FAIL tests for @-transforms (SB4015) and bare array (SC2128)
3f51abb validator: SB4015 reject ${var@X} parameter transformations (DD#52)
096ca65 codegen: divide-by-zero guard in arithmetic + taint/transform fixes
8919c7a codegen: cd updates $PWD/$OLDPWD, cd - prints and swaps
381dd71 codegen: compound command redirects (for/while/if/group > file)
ac4a28e brush compat: 44 brush tests, 15 e2e tests, taint fixes for builtins/let/numeric literals
a41188e taint: fix cmdsub with comments, SSA builtin whitelist
37dc7aa taint: fix false positives for builtins, known env vars, let, numeric literals
905ad7a audit: Brush test suite checklist — 66 actionable cases triaged
9a42cc0 subset: remove ${var@...} parameter transforms (DD#52)
3fb3b02 validator: SB4014 reject tree-sitter ERROR nodes
b53ad2a codegen: parameter transformations (@U/@u/@L/@Q/@E), break/continue N, brush triage update
f750191 codegen: implement -ot/-ef/-v test ops, ICE-prefix all panics, Brush triage
1ce52fc docs: consolidate TODO.md, remove stale GAPS doc
d1bcf9f DD#51: SSA taint verification pass — defense-in-depth backstop
d6b6c45 DD#49-50: audit run-2 remediation — 12 taint/bless/positional fixes
```

## Design decisions made

- **DD#47:** Bless runtime enforcement — `bless` evaluates test at runtime, aborts on failure
- **DD#48:** Out-of-range positional detection — compile error for `$2` in 1-arg function
- **DD#49:** Tautological bless rejection — `bless x [[ 1 == 1 ]]` rejected (SB4012)
- **DD#50:** Function free-read tracking — globals checked at call site for taint
- **DD#51:** SSA taint verification pass — defense-in-depth behind CST taint
- **DD#52:** Remove `${var@...}` transforms — cleaner subset, use `^^`/`,,` instead

## New diagnostics

- **SB4012:** Tautological bless (test doesn't reference blessed variable)
- **SB4013:** Invalid bless syntax (not a `[[ ]]` expression)
- **SB4014:** Tree-sitter parse error (ERROR nodes in CST)
- **SB4015:** `${var@X}` parameter transforms rejected
- **SB5003:** `declare -n` / `local -n` namerefs rejected
- **SB5004:** Process substitution `<(...)` / `>(...)` rejected

## Architecture: taint model is defense-in-depth

The "re-platform taint on SSA" TODO item is resolved. The plan is
**both** passes, not replacement:

1. **CST pass** (`taint.rs`, ~850 lines) — primary enforcer, runs before
   lowering, gives source-precise diagnostics (SB4020)
2. **SSA pass** (`ssa_taint.rs`, ~340 lines) — backstop verifier, runs
   after SSA construction, emits ICE if anything tainted reached codegen

If the SSA pass fires, that's a bug in the CST pass. The SSA pass is
the airbag, not the seatbelt.

## Adversarial audit run-2

Full pipeline completed: 5 models (gpt-4.1, claude-sonnet-4, claude-sonnet-4.5,
gpt-5.2, gpt-5.3-codex), 12 deduplicated findings (T1-T12), all fixed.
Artifacts in `audit/run-2/`.

Key fixes from audit:
- T1: Herestring of tainted data → now checks subtree for taint
- T2: Tautological bless → SB4012
- T3: Invalid bless syntax → SB4013
- T4: Arithmetic expansion taint propagation
- T5: Parameter expansion operand recursion
- T6: Function free-read tracking at call site
- T9: shift + positional range detection

## Key design decision: DD#31 — arithmetic side effects

`(( x = 1 ))` and `$((x = 10))` do NOT update the variable. DD#31
says mutations and comparisons in `(( ))` are disjoint. Arithmetic
assignment is a statement (`x=$((...))` or `(( x = expr ))` as a
standalone command), never an expression with side effects. This is
deliberate — do not implement `$((x=10))` updating `x`.

**See `docs/DECISIONS.md`** for the full design decisions reference.
DG pointed to this file specifically — it's the authoritative source
for what's intentional vs. what's a gap. Read it before changing any
behavior that might be deliberate.

## Regex `{n,m}` quantifiers — BLOCKED

Plan 9 regex engine (`regcomp9`/`regexec9` in `sash-rt/csrc/`) does
not support counted repetition (`{3}`, `{2,4}`). Tests with
`[[ "abc123" =~ ^[a-z]{3}[0-9]{3}$ ]]` fail silently (no match).

Options:
1. Add `{n,m}` support to the Plan 9 C code
2. Switch to a different regex engine (rust regex crate, but no_std constraint)
3. Accept the limitation and document it

This needs DG's decision.

## Brush test suite status

622 total cases, ~300 NA (features sash rejects by design).

Estimated (not re-run since taint fixes):
- PASS: ~175-180
- FAIL_COMPILE: ~55-60 (mostly correct rejections)
- FAIL_OUTPUT: ~50-55
- Effective pass rate: ~67%

Detailed checklist: `audit/run-2/BRUSH_CHECKLIST.md`

The `tests/brush_compat_tests.rs` file has 52 tests in 3 categories:
- `brush_pass_*` — compile + run, output matches bash
- `brush_rejected_*` — correctly rejected with specific diagnostic
- `brush_native_*` — sash-native rewrite compiles, output matches bash original

## Unstaged work from conversation context

The conversation explored but did NOT commit:
- `printf %q` inline implementation — deferred (fork to system printf works)
- Cranelift API changes (`BlockArg::Value` wrapper, `jump`/`brif` signature
  changes) — this was explored when testing newer cranelift versions but
  the current codebase uses cranelift 0.130 and works

## Observations from late exploration (not acted on)

- **`$((x=10))` arithmetic assignment:** Confirmed does NOT update `x` in SSA.
  DD#31 says this is intentional — mutations and comparisons in `(( ))` are
  disjoint. The transform parses `x=10` and returns the value `10` but doesn't
  call `state.define("x")`. This is correct per DD#31. Do not "fix" this.

- **`printf %q`** difference: sash forks to system `printf` which uses
  bash-style quoting (`hello\ world`). The dead `sash_shell_quote` runtime
  function does POSIX-style (`'hello world'`). If we ever inline printf %q,
  need to match bash's backslash-escaping style, not POSIX quoting.

- **`command -v echo`** returns `/usr/bin/echo` from sash but `echo` from
  bash. FIXED in commit 7489d4c — runtime now checks a builtin list first.

- **Tilde expansion in quotes:** `"~/projects"` correctly does NOT expand
  tilde (bash behavior). Unquoted `~` expands to `$HOME`. Working correctly.

- **Div-by-zero constant folding:** The transform's constant folder was
  silently folding `1/0` → `0`. Fixed to emit symbolic expression instead,
  so codegen's runtime guard catches it. Both div and mod by zero now print
  "division by 0" to stderr and set `$?=1` without aborting.

- **Regex `{n,m}` deep dive:** The Plan 9 regex engine in
  `sash-rt/csrc/regcomp.c` has no concept of counted repetition. The
  `regcomp9` function treats `{` as a literal character. Options discussed:
  1. Patch the Plan 9 C source to add `{n,m}` support
  2. Switch to a Rust regex crate (constraint: `#![no_std]`)
  3. Accept the limitation and document it
  DG needs to decide.

- **`[[ -v var ]]` implementation:** The initial implementation treated `-v`
  as equivalent to `-n` (non-empty), which was wrong for unset variables.
  Fixed to scan the `vars` HashMap for the variable name, finding the highest
  SSA version and checking its string length. This is still imperfect — in
  sash, all variables are initialized to empty string at version 0, so `-v`
  for a never-assigned variable returns false (correct) but `-v` for a
  variable set to empty string also returns false (differs from bash where
  `-v x` is true after `x=""`). This edge case is noted but not fixed.

- **Arithmetic comma operator** (`$((x=10, y=20))`): Not implemented. The
  transform doesn't parse comma-separated expressions. Variables used in
  the expression get flagged as SB4020 (tainted) because the transform
  doesn't recognize them as assignments. Low priority — DD#31 says
  arithmetic assignments are statements, and comma expressions are rare
  in real scripts.

- **`printf %q`** works correctly via fork to system printf. Output matches
  bash. No need to inline unless performance matters.

- **`command -v echo`** returns `/usr/bin/echo` (PATH lookup) not `echo`
  (builtin name). This is actually correct for sash — sash's `echo` compiles
  inline, but the binary it produces isn't a shell, so `command -v` checking
  PATH is reasonable. The runtime fix in 7489d4c only applies when the
  command name matches a known builtin. `echo` IS in the builtin list, so
  this should print `echo`. If a test shows `/usr/bin/echo`, that's the
  runtime checking PATH before the builtin list — verify order in
  `sash_command_v()`.

- **Array per-element substitution** (`${arr[@]/%0/9}`) confirmed not
  working. The codegen handles `${var/pat/rep}` for scalars but doesn't
  loop over array elements. This is the #4 priority item — 13 Brush cases
  depend on it.

## Test suite: 590 tests

| File | Count |
|------|-------|
| codegen_tests | 268 |
| e2e_tests | 131 |
| oracle_tests | 59 |
| brush_compat_tests | 52 |
| taint_spec_tests | 43 |
| janet_tests | 37 |

## Priority work remaining (from TODO.md + session notes)

1. **Pipe-free user-function capture** — user func cmdsub uses pipe
   unnecessarily; should accumulate to buffer directly
2. **Per-command env vars** — `FOO=bar cmd` syntax not compiled
3. **Compound redirect edge cases** — pipeline redirects (`cmd | cmd > file`)
4. **Array per-element ops** — `${arr[@]/pat/rep}`, `${arr[@]#prefix}` (13 brush cases)
5. **Convert remaining codegen panics** — 32 ICE-prefixed, several are
   unimplemented features rather than true bugs
6. **Regex `{n,m}`** — needs DG decision on approach

## Files changed this session (key ones)

- `sash/src/taint.rs` — ~850 lines, major rework: bless validation,
  herestring taint checking, function free-read tracking, builtin cmdsub
  classification, known env vars, `let` handling
- `sash/src/ssa_taint.rs` — NEW, ~340 lines, SSA taint verification pass
- `sash/src/codegen.rs` — ~4500 lines, compound redirects, cd/PWD,
  div-by-zero, [[-v]], pipeline exit status, $RANDOM, ${#arr[N]}
- `sash/src/transform.rs` — ~2600 lines, bless SSA versioning, function
  redirects, break/continue N, $RANDOM lowering
- `sash/src/lower.rs` — bless lowering with test expr, ${#arr[N]},
  ${x:(-5)} negative offset, @-transform rejection
- `sash/src/rules/` — new: `parse_error.rs`, `nameref_rejected.rs`,
  `process_substitution.rs`, `param_transform.rs`
- `sash-rt/src/lib.rs` — sash_random, sash_file_same_dev, command -v
  builtins, dead code marking on shell_quote/expand_escapes
- `sash/tests/brush_compat_tests.rs` — NEW, 52 tests
- `sash/tests/taint_spec_tests.rs` — NEW, 43 tests
