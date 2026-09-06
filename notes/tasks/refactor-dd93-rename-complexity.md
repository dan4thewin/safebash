status: open
depends:
gates:

# Refactor complexity violators exposed by the DD#93 rename

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
