---
title: "sash-convert: mechanically rewrite bash `case…esac` → `[[ ]]` dispatch"
status: needs-decision
priority: P3
kind: feature
depends: [dd66-remove-case]
gates:
  - "all-exact `case` (single-literal arms + optional `*)`) auto-rewrites to a `==` dispatch"
  - "alternation/glob arms rewrite to `=~` with anchored-regex patterns"
  - "`*)` default → `|| { … }`; arm bodies still get their inner conversions"
  - "mixed/ambiguous arms fall back to the MANUAL annotation (never emit wrong code)"
---

# Converter: `case…esac` → `[[ ]]` dispatch

DD#66 removed `case`; the compiler now rejects it (SB4029). sash-convert
currently emits a **MANUAL annotation** for `case_statement`
(`convert.rs`, next to the `subshell` handler) directing users to the
dispatch form — the same convention subshell/select/coproc use. This
task is the *mechanical auto-rewrite* that replaces that annotation for
the cases it can convert safely.

## Why this is not done yet (decision needed from DG)

The dispatch form states **one** match operator for the whole construct
(`==` or `=~`), but bash `case` mixes exact and glob patterns
**per-arm**. So a faithful rewrite has to pick a whole-construct policy,
and there is a readability tradeoff (the same one that shaped the janet
backend — DG asked there for *readable* output, not maximal fidelity):

1. **Op-selection policy for mixed arms.** Options:
   - **(A) all-exact ⇒ `==`, otherwise ⇒ `=~`** (convert every pattern to
     anchored regex: `foo`→`^foo$`, `a|b`→`^(a|b)$`, `*.sh`→`^.*\.sh$`,
     `h*`→`^h`, `?`→`.`, `[0-9]`→`[0-9]`). Deterministic, but turns a
     clean mostly-exact `case` into all-regex the moment one glob arm
     appears — less readable.
   - **(B) all-exact ⇒ `==`; all-glob/alternation ⇒ `=~`; genuinely
     mixed ⇒ MANUAL** (don't guess). Most readable, but converts fewer
     scripts automatically.
   - **(C)** something else DG prefers.
2. **Nested arm-body patches.** The converter is patch-based (replace a
   CST node's byte span). If the rewrite replaces the whole
   `case_statement` span it will **conflict** with the inner-body patches
   the recursive walk produces (e.g. `$HOME`→`$(getenv HOME)` inside an
   arm). Two implementation strategies:
   - **surgical token patches** — patch only `case $x in`→`[[ $x OP`,
     each `pat)`→`pat {`, each `;;`→`}`, `*)`→`|| {`, `esac`→`]]` — so
     inner-body patches still apply. Requires the op decided up-front
     (all arms share it), which suits policy (A)/(B).
   - **whole-span replace** — generate the entire dispatch text and skip
     recursion into the case subtree (convert arm bodies inline in the
     generator). More control, but re-implements body conversion.

DG should pick the op-selection policy (1) and confirm the patch
strategy (2) before implementation.

## Proposed spec tests (once decided)

- `case $x in a) …;; b) …;; esac` → `[[ $x == a { … } b { … } ]]`
- `case $x in a) …;; *) …;; esac` → `… || { … }`
- `case $x in a|b) …;; esac` → `[[ $x =~ ^(a|b)$ { … } ]]`
- `case $x in *.sh) …;; esac` → `[[ $x =~ ^.*\.sh$ { … } ]]`
- an arm body containing `$HOME` still converts to `$(getenv HOME)`
- mixed exact+glob arms (if policy B) → MANUAL annotation retained

## Notes

- A `glob_to_regex` already exists in `sash/src/parse/expr.rs` (and a
  `glob_match` in sash-expand) — reuse/port rather than re-derive.
- Keep the MANUAL fallback: the converter must never emit syntactically
  wrong dispatch. Anything it can't prove convertible stays annotated.
