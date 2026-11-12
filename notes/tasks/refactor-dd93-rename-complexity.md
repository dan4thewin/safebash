title: "Refactor complexity violators exposed by the DD#93 rename"
status: closed
closed: 2026-09-12
priority: P4
kind: refactor
depends:
gates:

# Refactor complexity violators exposed by the DD#93 rename

> **CLOSED (DG, 2026-09-12).** Four of the seven cleared on their own; the
> framing — "functions a mechanical rename happened to touch" — was never a
> meaningful unit of work. The three survivors are covered by the gate's
> grandfather-on-modify rule, which fixes them exactly when someone edits
> them. `emit_attest_all` (over by 0.42, straight-line Cranelift emission)
> is **accepted as-is**. The re-measured analysis below is kept because the
> two genuine hotspots (`check_inst_uses`, `lower_if`) and their suggested
> fixes are still accurate for whoever next touches them.
>
> Also decided: **the ratchet holds at 3×** — see the cohort table at the
> bottom and the updated note in `AGENTS.md`.

The DD#93 Stage 1 whole-tree `bless`→`attest` rename (commit — see journal
247) "touched" 7 pre-existing complexity-gate violators via grandfather-on-
modify. The rename changed **zero** actual complexity (pure identifier
substitution), so the commit was landed with `SKIP_COMPLEXITY_GATE=1` (DG
approved, 2026-09-06: "bypass-and-file").

Of the 7, **4 are wide-but-shallow dispatchers** (cog ≤ 3) that the cog/cc
rubric (`notes/journal/108-cog-cc-rubric.md`) says to *accept as-is* — they
only fall short of the delta ≤ −15 exemption. No action wanted:

    cc=15 cog=2  emit_command      sash/src/emit_janet.rs
    cc=14 cog=2  classify_attest   sash/src/ir.rs
    cc=13 cog=3  intern_inst       sash/src/codegen/setup.rs
    cc=11 cog=7  lower_if_attest   sash/src/transform/control_flow.rs  (borderline)

**Genuine targets** (cog ≈ cc or cog > cc — real nesting, not a flat table):

    cc=10 cog=10  check_inst_uses   sash/src/ssa_taint.rs:205
    cc=6  cog=9   lower_if          sash/src/transform/control_flow.rs:163
    cc=5  cog=7   emit_attest_all   sash/src/codegen/dispatch.rs:634

Task: assess and, where it genuinely reduces cognitive load, refactor the
three genuine targets (extract helpers / collapse nesting) so they clear the
gate on their own. Re-evaluate `lower_if_attest` (cc=11 cog=7) at the same
time. Leave the four dispatchers alone unless a −15 exemption is cheap.

Not blocking DD#93 Stages 2–4.

## Re-measured 2026-09-12 — 4 of the 7 self-cleared

Current gate: cc > 12.46 or cog > 6.58 (3× mean), exempt if delta ≤ −15.

| function | filed | now | verdict |
|---|---|---|---|
| `emit_command` (emit_janet.rs) | 15/2 | **2/1** | cleared |
| `classify_attest` (ir.rs) | 14/2 | **6/1** | cleared |
| `intern_inst` (codegen/setup.rs) | 13/3 | **9/3** | cleared |
| `lower_if_attest` (control_flow.rs) | 11/7 | **7/6** | cleared |
| `check_inst_uses` (ssa_taint.rs:207) | 10/10 | 10/10 | over on cog |
| `lower_if` (control_flow.rs:281) | 6/9 | 6/9 | over on cog |
| `emit_attest_all` (codegen/dispatch.rs:652) | 5/7 | 5/7 | over on cog by 0.42 |

All three survivors exceed on **cognitive only**, and the filing's original
triage was right about which three would last.

Reading the three:

- **`check_inst_uses`** — a flat 5-arm `match` over `SsaInst`, each arm
  calling `check_value_use` with a `format!` label. Its cog=10 is *entirely*
  nested `for` loops inside match arms (+2, +2, +3). Extracting
  `check_all(values, taint, errors, label)` removes three loops and reads
  better. Worth doing.
- **`lower_if`** — cog=9 is dominated by a triple-nested
  `if` / `if let` / `if` at lines 288–293 (+1+2+3 = 6) that only detects the
  DD#24 if-attest form. Collapsing it into one guard or an
  `attest_condition(i) -> Option<&Command>` helper is the rubric's textbook
  "low cc, high cog = hidden nesting" case. Worth doing.
- **`emit_attest_all`** — straight-line Cranelift block emission; the
  conditionals are just `is_map` / `if_form`. Over the bar by 0.42. Splitting
  it would obscure the header/body/after block structure. Recommend accept.

## The framing is obsolete

The filed unit of work — "the functions the DD#93 rename touched" — was
never a meaningful set; it is just what one mechanical identifier
substitution happened to brush against. The real cohort at the current 3×
gate is **80 functions** in `sash/src/`.

That also bears on the stated ratchet plan (AGENTS.md: "start at 3× mean,
tighten to 2× then 1.5× as the cohort clears"). The cohort is not clearing:

| ratchet | violators |
|---|---|
| 3× (today) | 80 |
| 2× | 357 |
| 1.5× | 518 |

Tightening to 2× would put a third of the codebase in violation. Since the
gate is grandfather-on-modify, those 80 are fixed exactly when someone
touches them — which is the mechanism working as designed, not a backlog.
