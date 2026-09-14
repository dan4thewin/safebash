# 307 — Disposition of the uncovered remainder

DG's directive after the DD#109 commit: full line **and branch** coverage;
anything unreachable gets evaluated — dead code removed, should-never-run
assertions kept and given a unit test that proves they catch what they claim.

This entry is the first disposition pass. Three of the nine zero-coverage
functions turned out to be reachable-and-untested and now have specs; four are
dead; two need DG.

## Where the numbers stand

| run | lines | regions | fns | branches |
|-----|------:|--------:|----:|---------:|
| pre-DD#109 (journal 306) | 93.31% | 91.63% | 95.44% | 83.77% |
| post-DD#109 | 94.66% | 93.79% | 97.11% | 83.97% |
| + vm.bl16–18 (this entry) | **94.79%** | **93.92%** | **97.25%** | **84.21%** |

Sweep 4568 passing / 9 skipped / 0 failing.

## Two bugs in the measurement path, found by measuring

### `mk build` never built `sash-expand`

`build:V:` builds `sash`, `sash-convert`, the runtime and the two C helpers.
It does not build `sash-expand`, even though `sash-expand` is a workspace
member and spec `54.1` locates it with `which sash-expand` on the PATH that
`spec/run.sh` extends with `target/release`.

Nothing noticed because `mk test` runs `(cd sash-expand && cargo test
--release)`, which builds it as a side effect, and nothing else ever removed
`target/release`. `mk cov` and `mk cov-reset` both `rm -rf target/release` by
design — so the coverage work turned a latent gap into a reproducible failure:
after any coverage run, `54.1` failed in both modes until something ran the
Rust tests again.

The failure mode is nasty out of proportion to the cause. `which` prints
nothing and exits 1, `EXPAND=` becomes empty, and the spec body then compiles
`s=$( /tmp/sash_test_config.conf)` — so the diagnostic is a *compile* error
about a config file, with no mention of the missing binary.

I hit this twice and misread it the first time as a build race, because the
sweep happened to run while `mk test` was rebuilding. Fixed by adding
`$EXPAND` to the `build` target.

### `contrib/coverage.sh` never emitted lcov

The script printed the summary table and stopped there. The table names which
*files* are thin; only lcov names which *lines*, and it cannot be regenerated
afterwards because `mk cov-reset` drops `target/release` and with it the
covmap. So every triage so far has been done from an lcov in `/tmp` that the
next reboot would take, and journal 306's rebuild script lived in `/tmp` too.

Now `--lcov FILE`, wired to `target/coverage/{base,branch,unit}.lcov` (which
survives `cov-reset`, since that only removes `target/release`).
`contrib/covdb.py` is the former `/tmp/mkcovdb.py`, promoted, argument-taking,
and extended to parse `BRDA:` branch records; `mk cov-db` runs it.

## The disposition

### Reachable and untested → specs (done)

`parse/lexer.rs` had three zero-coverage helpers, all on the brace-less
disposition operand path (DD#90):

- `skip_braced_to_close` — `${…}` nested in a brace-less operand
- `skip_simple_quote` — a quote inside a `$( )` inside a brace-less operand
- `skip_backtick_to_close` — a `` `…` `` run in a brace-less operand

All three are live, correct, and were simply never exercised: the existing
vm.bl13/bl14 cover a *bare* `$( )` and `$(( ))` operand, and nothing nested
anything inside one. Added vm.bl16–bl18. `parse/lexer.rs` function coverage
is now **100%** (88/88).

vm.bl17 is the one that earns its keep: `$x:-$(echo "a)b")`. The paren scan
counts depth, so without `skip_simple_quote` the `)` inside the quotes closes
the substitution early and the operand is silently truncated to `$(echo "a`.

vm.bl18 asserts brace-less ≡ braced rather than a literal value, deliberately
— see the open question below.

### Two stale doc comments and a stale DD clause

Chasing the backtick case turned up documentation that contradicts the code:

- `parse/expr.rs:parse_default_word` claimed `` `…` `` **interpolates** in a
  `${var:-WORD}` default. It does not, and deliberately: the default body *is*
  a `"…"` body, and a backtick inside quotes is a literal character (DG
  2026-08-28). So `` ${x:-`cmd`} `` yields the backticks verbatim.
- `parse/lexer.rs:scan_braceless_operand` claimed it returns `None` for a
  `$( )` / `$(( ))` / backtick operand. It consumes them; `None` is only for an
  *unterminated* run.
- DD#90 rule 4 ("those require braces") was superseded by the DD#91 follow-up
  (DG 2026-09-04) that vm.bl13 records. The DD file never got the note.

All three now say what the code does, and DD#90 rule 4 is marked SUPERSEDED
with a pointer to the specs.

### Dead → proposed for removal (needs DG)

**The compound-redirect machinery.** `bracket_compound_with_redirects`
(transform) is the *only* producer of `BuiltinCmd::RedirectPush` /
`RedirectPop`, and it cannot run:

- `Redirected` has exactly two construction sites, both in the parser
  (`parse_command`, which wraps a `Command`, and `wrap_with_redirects`, reached
  only from `wrap_pipeline_part`).
- `wrap_pipeline_part` is guarded by `if is_compound(&stmt.kind) { … } else`,
  so a compound never reaches it — it gets SB4021 instead.
- The one kind in transform's `is_compound_stmt` but not the parser's
  `is_compound` is `Function`, and a redirect on a function definition is
  SB4046.

So `is_compound_stmt(&r.body.kind)` in `lower_redirected_stmt` is always
false. Measured, not inferred: `{ …; } > f`, `while …; done > f` and
`if …; fi > f` all fail with SB4021.

Dead as a consequence: `bracket_compound_with_redirects`, `is_compound_stmt`,
the `needs_bracket` branch, `emit_redirect_push` / `emit_redirect_pop` and
their two dispatch arms, `emit_c`'s `Emit.redirect_stack` and
`emit_compound_redirect`, the `RedirectPush`/`RedirectPop` variants with their
`from_name` / `sigil_str` arms, and the corresponding `ice_unreachable_command`
arm.

This was live once. Journal 160 implemented the emit_c half for `mapfile -t a
< file`, which used to lower to a `RedirectPush … Mapfile … RedirectPop`
triple. Journal 268 gave `ReadArray` its own redirect list (so it could get the
DD#55 open(2) guard), which removed the last real producer — and journal 281's
trace, which recorded the pair as "intercepted by `emit_compound_redirect`",
predates that change.

**`lower_negated_stmt`.** `StmtKind::Negated` is constructed in exactly one
place — `append_pipeline_stage`, for a negated non-leading pipeline stage
(`a | ! b`) — and is peeled in exactly one place, `push_pipeline_stage`. It
therefore never reaches statement dispatch, which is what its own doc comment
suspects ("the defensive fallback for a `Negated` reaching statement
dispatch").

Confirmed empirically rather than by reading: a temporary `eprintln!` in the
function, then eleven negation spellings — `! false`, `- ! false`, `if !`,
`while !`, `! … || :`, `! … && :`, a function body, `! !`, and three negated
pipeline stages including a 3-stage one. Zero hits. `! false` alone is SB4018.

It is worth noting *what* the fallback does: it lowers the inner statement and
flips the test result — plausible code, silently. The house rule from journal
281 is that an unreachable arm is a compiler bug and should ICE, not produce
something reasonable.

### Assertions → unit tests (proposed)

`emit_c::ice_unreachable_command` and `codegen/redirects::redirect_target_ice`
are explicit ICE terminals, 0% under the spec suite by construction.
`codegen/entry::panic_payload_message` is the same shape and already has unit
tests — the pattern to copy.

### A second dead cluster, not yet dispositioned

`emit_ir::ar_incdec` and `ar_subscript_assign` survived DD#109 uncovered, which
is surprising given the IR is now rendered for every compile. The reason is
that `parse/arith.rs:lower_arith_mutation_structured` rewrites `PostIncDec`,
`PreIncDec` and `SubscriptAssign` into other statement forms **inside the
parser**, so those three `ArithExpr` variants never appear in the `Program` that
`compile_to_object` hands to `emit_ir`.

Checked: `int array a=(7 13 21); (( a[0] = 99 ))` is spec-covered
(`arithmetic.sh:482`) and emits only `:arith-literal` — no
`:arith-subscript-assign`. `(( i++ ))` emits `:arith-binop :arith-literal
:arith-var`. `$(( i++ ))` is SB4020.

`transform/expr_handlers.rs:376` already has an `unreachable!` for exactly
these three variants, which corroborates it. But `transform/statements.rs`
`hoist_mutation_node` (28 uncovered lines, the second-largest single gap) and
`hoist_arith_mutations` (11) also match on them, so the cluster spans more
than emit_ir and deserves the same trace before anything is removed. Filed
rather than guessed at.

## Open for DG

1. **Remove the compound-redirect machinery?** It is unambiguously dead and
   spans parser→transform→both backends. Against removal: a future DD could
   allow compound redirects, and this is the implementation. For removal: the
   direction of travel is the opposite — SB4021 is deliberate, and the task
   `converter-compound-postfix` has `sash-convert` *rewriting* compounds into
   functions to satisfy it.

2. **`lower_negated_stmt`: delete the arm, or ICE it?** Recommend ICE, per
   journal 281's rule. Either way it stops silently producing code on a path
   that cannot happen.

3. **The backtick/`$( )` asymmetry in a default word.** DD#67 makes flat
   `` `…` `` 1-to-1 sugar for `$(…)`. DG 2026-08-28 makes a backtick literal
   inside quotes. A `${var:-WORD}` default is a quotes body (DD#65). The
   intersection is consistent but means `` ${x:-`cmd`} `` and `${x:-$(cmd)}`
   are *not* interchangeable — the first yields the backticks verbatim. Both
   rules are DG's; the consequence may not have been intended. vm.bl18 is
   written as an equivalence so it stays correct either way.
