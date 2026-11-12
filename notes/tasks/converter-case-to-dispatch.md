title: "sash-convert: mechanically rewrite bash `case…esac` → `[[ ]]` dispatch"
status: ready
priority: P3
kind: feature
depends: [dd66-remove-case]
gates:
  - "ALL-LITERAL case (single-literal arms + literal `a|b|c` alternation arms + optional `*)`) auto-rewrites to a COMPILABLE `==` dispatch, multi-exact for alternation (`a|b|c)` → `== a b c { }`); `*)` → `|| { … }`; arm bodies get their inner conversions; emit an informational `# sash-convert:` comment"
  - "ANY arm needing REGEX SYNTHESIS (a glob metachar `*`/`?`/`[…]`) → do NOT auto-rewrite: emit the best-effort anchored-`=~` dispatch (exact→`^a$`, alt→`^(a|b)$`, `*.sh`→`^.*\\.sh$`) as a `# sash-convert:` COMMENT and leave the original `case` in place — it fails to compile (SB4029), forcing the human to review the converter-written regex"
  - "`;&`/`;;&` fall-through arms get the SAME conservative treatment (comment + original kept): dispatch has no fall-through, semantics changed"
  - "MANUAL annotation retained only for genuinely unparseable input"

## Policy resolved (2026-07-06) — split by risk (DG)

The open op-selection question is settled by combining decisions that
landed elsewhere with DG's split-by-risk ruling on converter-written
regex.

**The line is regex synthesis.** Converting an all-literal `case` is a
faithful, mechanical 1:1 mapping and is safe to auto-emit as compilable
code. Converting a glob arm requires the converter to *write regex*
(`*.sh`→`^.*\.sh$`), which DG is deliberately conservative about — an
auto-generated regex can be subtly wrong and must get human eyes. So:

- **All-literal case → auto-rewrite, compilable.** Single-literal arms
  and literal `a|b|c` alternation arms map to a `==` dispatch, using
  **multi-exact** (DD#71, commit `e9fc520`): `[[ $x == a b c { … } ]]`.
  `*)` → `|| { … }`. Arm bodies still get their inner conversions
  (`$HOME`→`$(getenv HOME)`, etc.). Emit an informational
  `# sash-convert:` comment (mechanical-rewrite convention from
  `converter-compound-postfix`). No review flag — the mapping is exact.

- **Any glob-metachar arm → comment + keep original (compile-blocker).**
  The moment one arm has `*`/`?`/`[…]`, the converter does **not** emit
  compilable dispatch. It emits the best-effort **anchored-`=~`**
  dispatch as a `# sash-convert:` **comment** (exact→`^a$`,
  alt→`^(a|b)$`, `*.sh`→`^.*\.sh$`) and leaves the original `case`
  untouched. Because DD#66 removed `case`, that original **fails to
  compile (SB4029)** — a deliberate fail-loud that forces the human to
  review and commit the synthesized regex rather than silently trust it.
  This is stricter than `converter-compound-postfix` (which emits
  *compilable* code + a comment); regex-synthesis earns the stricter
  stance.

- **`;&`/`;;&` fall-through → same conservative treatment.** Dispatch has
  no fall-through (DD#66), so the semantics change. Emit the best-effort
  dispatch as a comment and keep the original as a compile-blocker.

- **MANUAL annotation** stays only for genuinely unparseable input.

Rationale note: one dispatch states a single op, so a mixed exact+glob
`case` can't be part-`==`/part-`=~`. Under split-by-risk that's moot —
any glob presence puts the *whole* construct on the comment path anyway,
so there is never a compilable dispatch that mixes ops.

Residual (implementer's call, not a DG decision):

- **Patch strategy.** All-literal path: either surgical token patches
  (`case $x in`→`[[ $x ==`, each `pat)`→`pat {` for literals, `;;`→`}`,
  `*)`→`|| {`, `esac`→`]]`) so inner-body patches still apply, or a
  whole-span replace that converts arm bodies inline. Comment path: the
  original bytes are kept verbatim; whether the recursive walk still
  applies inner-body patches to that (doomed) original is harmless either
  way — pick whichever is simpler.
- **`glob_to_regex`** for the comment-path best-effort: a `glob_match`
  exists in sash-expand; the old `glob_to_regex` in `sash/src/parse/`
  was removed this session. Port/derive a small anchoring helper for the
  comment text (it need not be compiler-grade — it's a human-reviewed
  suggestion).


# Converter: `case…esac` → `[[ ]]` dispatch

DD#66 removed `case`; the compiler now rejects it (SB4029). sash-convert
currently emits a **MANUAL annotation** for `case_statement`
(`convert.rs`, next to the `subshell` handler) directing users to the
dispatch form. This task replaces that annotation with the split-by-risk
rewrite above: auto-convert all-literal cases to compilable dispatch;
comment-only (+ keep original) for anything needing regex synthesis or
fall-through.

## Proposed spec tests (sash-convert output assertions)

- `case $x in a) …;; b) …;; esac`
  → auto: `[[ $x == a { … } b { … } ]]` (compilable)
- `case $x in a) …;; *) …;; esac`
  → auto: `[[ $x == a { … } ]] || { … }` (compilable)
- `case $x in a|b|c) …;; esac`
  → auto: `[[ $x == a b c { … } ]]` (compilable, multi-exact)
- `case $x in *.sh) …;; esac`
  → comment: `# sash-convert: [[ $x =~ ^.*\.sh$ { … } ]]` + original
    `case` kept (won't compile)
- `case $x in a) …;; *.log) …;; esac` (one glob arm)
  → comment path for the WHOLE construct + original kept
- `case $x in a) …;& b) …;; esac` (fall-through)
  → comment path + original kept
- an all-literal arm body containing `$HOME` still converts to
  `$(getenv HOME)`
- genuinely unparseable → MANUAL annotation retained

## Notes

- Keep the MANUAL fallback: the converter must never emit syntactically
  wrong *compilable* dispatch. The auto path only fires when every arm is
  literal; everything else is comment-only with the original preserved.
- D1 (journal 535): DD66's addendum of 2026-10-25 makes a whole-int dispatch
  subject or pattern SB4030. When the `case` subject is an int (`int n=1;
  case $n in 1) …`), the auto path must write `[[ "$n" == 1 { … } ]]`.
  Decide that with `is_int_expansion` (sash-convert/src/convert.rs), which
  already makes the same choice for converted `[[ ]]` tests, rather than a
  second test.
