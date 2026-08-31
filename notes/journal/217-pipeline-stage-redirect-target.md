## 217 — pipeline-stage redirect targets (bug fix, grind round)

**On `hand-parser`.** Resuming the coverage grind (`mk cov-spec-only`) on a
correct measurement surfaced a cluster of never-run redirect functions. Probing
them (the grind's whole point — never-run code hides bugs) found a real bug.

### The bug (pre-existing; grind-surfaced)

A **pipeline-stage** redirect with any non-literal target silently dropped — the
stage's output went to the pipe instead of the file:

```
echo hi > $f      | cat     # $f never written; "hi" leaks to cat
echo hi > ${x:-f} | cat
echo hi > $a[i]   | cat
```

Top-level (`echo hi > $f`) and literal stage targets (`echo hi > out | cat`)
worked, so stage redirects are a supported feature that was broken for
non-literals. DG ruled: fix to support (option A) — consistent with the literal
form and the scalar-target rule (journal 215).

### Root cause

The pipeline path had its **own** ad-hoc target lowering,
`ir_expr_to_ssa_value`, that (a) hardcoded SSA **version 0** for `$f`
(`VarRef::new(v, 0)`) — so it read an uninitialised slot — and (b) returned
`Literal("")` for `${x:-f}` / `$a[i]`. The top-level path resolves the highest
version (`lower_ir_redirect_expr` → `find_highest_version`) and spills
`${…}`/`[…]` to a synthetic scalar. The two paths had diverged.

### Fix

1. **Codegen** — `emit_stage_redirects` (inst_handlers.rs) now lowers non-literal
   targets through the same `lower_ir_redirect_expr` the top-level uses, and the
   buggy `ir_expr_to_ssa_value` is **deleted**.
2. **Transform** — `push_command_stage` now runs `spill_scalar_redirect_targets`
   on the stage's redirects (threaded `block` through `lower_one_pipeline_stage`
   / `build_producer_stages` so the synthetic `__redir_target_N` assigns land in
   the block before the `Pipeline` inst). So `${x:-f}` / `$a[i]` stage targets
   become a scalar read codegen resolves — same mechanism as a top-level
   redirect. Benefits c too (its `latest_var_as_str` had no version bug, and the
   spill hands it a SimpleExpansion).

### Specs (commands.sh 14.26–14.31, plan 40→46; native,c)

- 14.29/30/31 the bug fix: stage redirect with `$f` / `${x:-f}` / `$a[i]`.
- 14.26 `&>file` (parse_ampgt_redirect), 14.27 `> '…'` (redirect_expr_literal),
  14.28 `> "$x.txt"` (redirect_expr_concat) — the never-run target forms the
  grind flagged, all probed working first.

### Verify

Full 3-mode sweep vs `5ea5ceb`: **native/c 0 regressions, +6 each; janet 0**
(file redirects are native,c). Coverage re-measured (`scan_radix_literal`=281,
valid): native-core never-run **25 → 21** — `ir_expr_to_ssa_value` (deleted),
`redirect_expr_literal`, `redirect_expr_concat`, `parse_ampgt_redirect` all
cleared (verified 0→>0 / removed).

### Grind state (21 native-core never-run remain)

Triaged for follow-up: **bucket A** (add a spec) — `find_dollar_question`
(SB5012 `$?` no preceding command), `expr_invokes_random` (SB5035
`${x:-$(random)}`), `check_arg_default_taint` (SB5020 `x=$(arg N $tainted)`);
**bucket B** — `strip_matching_quotes` (lexer-shadowed, verify+remove);
**defensive** — `redirect_expr_zero` (now only the impossible-condition
fallback), `panic_payload_message` (ICE formatter, unit-test candidate);
**CLI-only** (not `run_sash`-reachable) — `dump_sash_source`, `set_sysroot`,
`emit_manifest`, `sysroot_file_exists`; **deferred** —
`extract_from_literal`/`extract_leading_operator` (dispatch-compact-operator
task); **artifact** — `type_check::fmt` (derive).
