title: "DECIDE: endorse or reject $LINENO (un-endorsed bash carryover)"
status: done
priority: P3
kind: decision
depends: []

# $LINENO — RESOLVED (DG, 2026-08-19): reject, no special handling

**DG: "reject — no magic vars; document how to do it with a preprocessor.
$LINENO shouldn't need special handling."**

Implemented: dropped the `parse_dollar_arm` special-case that lowered
`$LINENO` to a literal line number, and removed the `handled_elsewhere`
carve-out that exempted it from the DD#62 typo check. `$LINENO` now flows
through as an ordinary variable reference, so an unassigned `$LINENO`
(and the braced `${LINENO}`) is rejected as an **undefined variable**
(SB5031) — no dedicated `REJECTED_VARS` entry, no bespoke error, no magic.

Line numbers, if needed, come from a source preprocessor (e.g. `awk`/
`sed`/`m4`) that substitutes them before compilation. Documented in
`docs/SUBSET.md` (Rejected Constructs) + `docs/REVIEW.md`; DD#24's
clean-sources list dropped `$LINENO`. spec `undeclared.sh` ud.17/ud.18.
Journal 152.

---

# (original decision brief)

# $LINENO — endorse deliberately, or reject as a bash incursion?

DG flagged (2026-07-08): is `$LINENO` an *endorsed* feature or an
accidental bash carryover, like `~`→`$HOME` (rejected, SB4036) and
printf/pwd-as-builtins (fixed → external)?

## Findings (evidence)

- **No dedicated decision.** `$LINENO` appears in `notes/decisions/` only
  once, incidentally: DD#24 (taint-quarantine) lists it as an example of a
  clean "compile-time constant" (`$LINENO` alongside `$#`/`$?`/`$$`/`$!`).
  That is DD#24 rationalizing it into the taint clean-list, not a decision
  to support it.
- **A carryover.** Journal 076 shows it lived in the old (pre-hand-parser)
  `Tok::Dollar` arm; ported into the hand-written parser at `824e357`.
  Inherited from bash across both parser generations, never chosen.
- **No spec pins it.** Only `spec/` hit is `PS4='+$LINENO: '` in
  `common.sh` — bash xtrace config for the harness, not a sash test.

## The key distinction from the other incursions

`~`→`$HOME` and printf/pwd smuggled in a **runtime / environment
dependency** (getenv, execve) — that is why they were rejected/fixed.
`$LINENO` is different: `parse_dollar_arm` (`sash/src/parse/expr.rs:244`)
lowers it at **parse time** to a literal line number
(`ExprKind::Literal("N")`). It is a pure compile-time constant — no
runtime, no environment, no footgun. So the "reject un-endorsed bash-ism"
argument is weaker here than for `~`.

## Options

1. **Endorse.** Keep it; write a short DD (`$LINENO` = compile-time source
   line literal, clean per DD#24) and add a spec pinning the behavior, so
   it is a decision rather than an accident.
2. **Reject.** Treat it like `$RANDOM`/`$BASH_VERSION`: add `LINENO` to
   `REJECTED_VARS` (SB5017) and drop the `parse_dollar_arm` special-case.
   Strict-subset purity — every feature must be endorsed.
3. (default until decided) Leave as-is.

## Impact / notes

- Does NOT block `reject-undeclared-scalar-read`: `$LINENO` is already a
  `Literal` in the AST, so the scalar typo-check never sees it (no
  exclusion needed regardless of this decision).
- If rejected: one parser special-case removed + one REJECTED_VARS entry +
  a negative spec. If endorsed: a DD + a positive spec.

## Refs
- `sash/src/parse/expr.rs:244` (parse_dollar_arm LINENO → Literal)
- `notes/decisions/DD24-taint-quarantine.md:29` (the incidental mention)
- `notes/journal/076-parse-expr-extraction.md` (carryover history)
- sibling incursions fixed this cycle: `tilde-vs-dd53` (SB4036),
  `tighten-cmdsub-taint-execve` (printf/pwd external)
