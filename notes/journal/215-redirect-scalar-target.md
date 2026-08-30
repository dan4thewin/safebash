## 215 — redirect target must be a scalar (SB4049 reject + transform spill)

**On `hand-parser`.** Implements DG's ruling on the redirect-target silent-empty
bug (`bug-redirect-nonsimple-target-empty`, surfaced by the coverage grind):
**support scalars, reject complex.**

### The bug

A file-redirect whose target expr was not one of {Literal, SingleQuoted,
SimpleExpansion, Concat, DoubleQuoted} compiled, then at runtime opened an
**empty filename**. `> $x` worked; `> ${x:-f}` (ParameterExpansion) and
`> $a[i]` (Subscript) silently redirected to `""` (the `redirect_expr_zero` /
`ir_expr_to_ssa_value` `_ =>` fallthroughs the grind flagged as never-run);
`> $(cmd)` / `> $((x))` likewise.

### DG's ruling

A redirect target must be a **scalar** the compiler can evaluate to a filename:
`$x`, `${x:-default}`, `$a[i]`, or a literal/quoted/concat of those. **Reject** a
command substitution `$(…)` or arithmetic `$((…))` — the compiler can't know the
resulting filename, and a silent-empty is worse than a clear error.

### Implementation (two parts)

1. **Reject at parse — SB4049.** `parse_redirect` runs
   `reject_nonscalar_redirect_target` on every file-redirect target
   (`>`/`>>`/`<`/`<>`/`&>`). Scoping:
   - **not** `<<<` here-strings (stdin *data*, not a filename — untouched);
   - **not** `>&`/`<&` fd-dups (already numeric-only via SB4048).
   `redirect_target_is_scalar` is an allowlist (Literal / SingleQuoted /
   SimpleExpansion / ParameterExpansion / Subscript, or a Concat / DoubleQuoted
   of those) — the "compiler only accepts what it can evaluate" stance, so Array
   / AssocArray / Test targets are rejected too, not just cmdsub/arith.

2. **Support scalar reads — transform spill.** The ad-hoc redirect-target
   codegen only knows literals / `$x` / concats. Rather than duplicate the
   default/subscript lowering in codegen (which lacks the SSA version + assoc
   context to build the right `SsaValue`), `lower_redirected_stmt` now
   `spill_scalar_redirect_targets`: a target that `target_needs_spill`
   (ParameterExpansion / Subscript, or nested in a Concat/DoubleQuoted) is
   lowered via the ordinary `expr_to_value` — inheriting the exact
   default/subscript/string semantics of any other value position — bound to a
   synthetic `__redir_target_N` scalar (`state.define` + `SsaInst::Assign`), and
   the redirect rewritten to read `$__redir_target_N` (a SimpleExpansion codegen
   already handles). Applied before both the simple-attach and the
   `__redirect_push/pop` compound-bracket paths, so every redirect flavour is
   covered from one site (all redirects flow through the `Redirected` wrapper →
   `lower_redirected_stmt`; `Command` carries no redirects). No new SSA type;
   native + c inherit it (both go through the transform). janet has no file
   redirects (emits a no-op comment) — its behavior is unchanged, and the reject
   is parse-level so it applies there too.

The silent-empty `redirect_expr_zero` / `ir_expr_to_ssa_value` fallbacks are now
unreachable for redirect targets — they survive only on the here-string path.

### Specs

- `commands.sh` 14.19–14.24 (plan 33→39): reject cmdsub/arith targets (SB4049,
  all modes); support `${x:-d}` default-and-set, `$a[i]`, and `>> $a[i]` append
  (native,c — positives skip in janet).
- `taint.sh` 60.41/60.52 rerouted: a direct `$(join …)` redirect target is now
  SB4049 at parse, *before* the SB5020 taint check, so the array-join
  taint-at-redirect-target coverage moved to `x=$(join …); … > $x` (still
  SB5020). This is correct: the structural reject is a superset — a cmdsub target
  is rejected tainted or not.

### Verification

Full `SPEC_MODES="native c janet"` sweep, latest-run-per-test diff vs HEAD
(`65b4375`): **native 0 regressions, +6 new; c 0 regressions, +6 new; janet 0
regressions, +2 new** (the two parse-level rejects). fmt + clippy clean.
Complexity gate clear (`parse_redirect` cc=21 is an exempt wide-dispatcher at
delta −16; the new helpers are all ≤5/≤5).

Probed end-to-end (compile + run the a.out / gcc binary): `${x:-f}` opens the
default when unset and the set value when set; `$a[1]` opens the element's
filename; `>> $a[1]` accumulates; `> $(cmd)` / `> $((x))` reject with SB4049;
`<<< $(cmd)` is *not* rejected (data, not a filename).

### Filed

`bug-herestring-nonscalar-data-empty` (P2): the here-string DATA path has the
same silent-empty class (`<<< ${x:-f}`, `<<< $a[i]`, `<<< $(cmd)` all feed empty
stdin) plus a likely missing-trailing-newline bug for non-literal data. Left for
a DG ruling — here-strings feed stdin data (bash supports `<<< $(cmd)`), so the
"reject complex" call doesn't transfer 1:1; the spill mechanism already exists
and only skips `RedirectOp::HereString`.
