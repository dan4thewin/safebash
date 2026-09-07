# 248 — DD#93 Stage 2: content-shape tests `i?` / `p?` / `g?`

**Branch:** `hand-parser`. Follows journal 247 (Stage 1, `bless` → `attest`).

## What DG asked for

Stage 2 of the DD#93 bundle, per the tracker: the content-shape tests. Three
sash-native unary tests spelled with a `?` suffix, keeping them out of bash's
saturated `-X` one-letter namespace:

| op   | true iff the value is …                                     |
|------|-------------------------------------------------------------|
| `i?` | a strict, i64-representable integer (**hard rename of `-i`**) |
| `p?` | non-empty and every byte ASCII printable (0x20–0x7E)          |
| `g?` | non-empty and every byte ASCII graph (0x21–0x7E)              |

**General operators**, not attest-only: valid anywhere a unary test is.

## What I did

Test-first at each layer, mirroring the existing `-i`/`is_int` path.

- **Runtime** (`sash-rt`): `is_print_inner`/`is_graph_inner` beside
  `is_int_inner`, both over a shared `all_bytes_match(bytes, pred)` helper
  ("non-empty AND every byte in a class") so a later POSIX class (`a?`, `d?`,
  …) is one line. C-ABI wrappers `sash_is_print`/`sash_is_graph`.
- **Parser**: `-i` → `i?` in `is_unary_test_op`, plus `p?`/`g?`. The gate in
  `try_parse_leading_unary` tested `w.starts_with('-')`, which the `?`-suffixed
  ops fail — broadened to `starts_with('-') || ends_with('?')` (with `len >= 2`
  still excluding the bare `?` implicit-`-n` path). The lexer needed nothing:
  `?` is not a word-break char, so `i?` already lexed as one `Tok::Word`.
- **Backends ×3**: native `emit_shape_test` helper + two new `ExternFuncs`
  entries/declarations; C dispatch + the two `extern` prototypes in the emitted
  prologue (missing these was a link failure, caught by the smoke test); janet
  `_isprint`/`_isgraph` in the prelude beside `_isint`.
- **Specs**: `types.sh` 77.1–77.9 migrated to the `i?` spelling; new 93.10–93.19
  for `p?`/`g?` (space admitted by `p?` and rejected by `g?`, empty false, high
  bytes rejected, composition with `!`/`&&`, a `while` condition, and the hard
  rename — `[[ -i 42 ]]` is now SB4014).

3-mode sweep: **0 regressions, 30 new passing** (native/c/janet).

## Two bugs found on the way

**1. `sash-rt-tests/build.rs` had no `cargo:rerun-if-changed`.** `sash-rt` is a
separate cargo workspace, so nothing in `sash-rt-tests` changes when the runtime
is rebuilt — cargo therefore skipped the build script and the tests kept linking
a **stale** `libsash_rt_test.a`. My new symbols came up "undefined" even though
they were present in `libsash_rt.a`. The worse failure mode is silent: changed
runtime behavior would have tested green against the old code. Fixed by
declaring the dependency on `libsash_rt.a`.

**2. janet emits `(if true …)` for any compound `if` condition.** Spec 93.16
failed only in janet; the operators were innocent — the whole condition had been
replaced by `true`. Reproduces with pre-existing operators and is *not* about
DD#93: `if [[ -z x ]] || [[ -z y ]]`, `if [[ -n x ]] && [[ -z y ]]`, and even
`if false || false` all print the then-branch in janet and the else-branch in
native/c. A single-statement condition is fine, and `||` *inside* one `[[ ]]`
(which lowers to `TestExpr::Or`, not a statement list) is fine — so it is the
`if`-condition path for `StmtKind::List`. Silent wrong answer, filed P1 as
`notes/tasks/janet-compound-if-condition-ignored.md`. Note it means any janet
spec with a compound `if` condition expecting the then-branch currently passes
**vacuously** — `types.sh` 77.5 is one — so fixing it may surface real janet
failures it has been masking. 93.16 is written with the `||` inside the `[[ ]]`,
which is correct in all three backends.

## Environment note (rsynced host)

This tree was rsynced from another machine, and several gates were broken by
that rather than by any code change. Recorded so the next session does not
mistake them for regressions:

- The rsynced `spec.db` baseline disagreed with a **clean tree on this host** by
  61 tests (59 janet, `c:54.1`, `native:54.1`). Verified environmental by
  stashing and running the sweep on a clean tree: identical 61. The cause was
  **stale rsynced binaries**, not code:
  `target/release/sash-regex` (built against a newer glibc — `GLIBC_2.34 not
  found` — which accounts for the 59 janet regex failures), `sash-getopt`,
  `target/debug/sash` (so every cargo test that shells out to `sash` failed),
  and `sash-expand`, which had never been built here at all — 54.1 resolves it
  via `$(which sash-expand)`. After rebuilding all four, this host matches the
  source baseline **exactly**: the same 34 failing, and 4202 → **4232** passing
  with Stage 2's +30.
- `contrib/sash-grep.sash` no longer compiles (SB5025, `$(argv)` splat rule), so
  `mk test`'s `sash-grep-test` prerequisite fails and halts the target before the
  cargo suites. Pre-existing language drift in a contrib script, not this work.
- `mk lint` fails on `transform/functions.rs:598` and `validate.rs:864`
  (`collapsible_if`) under this host's much newer clippy 0.1.98 (2026-09-01).
  Both files are untouched by Stage 2.
- The cargo suites have a large pre-existing failing baseline here (84 codegen,
  48 e2e, 25 taint_spec, …). Verified identical on a clean tree; Stage 2 adds
  only the 5 new parser unit tests.

Also fixed the indexnudge `sqlite3` shim this session (concurrent shim
invocations were reading — and destroying — the code index); that is a tooling
change outside this repo, described in `notes/status.md`.

## Next

S3 (unary shorthand `attest VAR OP`), then S4 (arith-assign attest form).
