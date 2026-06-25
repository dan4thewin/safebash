title: "DD#70 (A)/(B) error-model — comprehensive contract spec"
status: ready
priority: P2
kind: feature
depends: []
gates:
  - "every (A) member aborts-if-unchecked, is caught by if/||, and is `-`-suppressed (the result-verdict) — verified per member"
  - "every (A) abort prints `<argv0>: <context>: <reason>` to stderr; caught/suppressed failures print NOTHING"
  - "every (B) member is a compile error when its result is discarded; `if`/`||`/`|| :` consume it; `-` on a (B) construct is a compile error"
  - "the spec contract lives in spec/errors.sh (native) and is the regression gate for DD#70"

Tracks turning DD#70 (notes/decisions/DD70-error-taxonomy-A-B.md) into a
complete, regression-gated spec contract. Much is already implemented and
tested; this task fills the gaps and consolidates the contract.

## Status of the two channels

- **(A) done**: cd / echo-write / redirect-open abort + diagnostics
  (commits 8a40ddc, f9d833b; spec 55.20-30).
- **(A) pending diagnostics**: external / pipeline / cmdsub / bg-exec /
  overflow / dynamic-regcomp — abort exists, the `<argv0>: …` message does
  not. See `abort-diagnostics-remaining.md`.
- **(B) done**: `-> int` functions (SB4016 bare call, SB4017 return-0-only;
  spec 55.13/55.14).
- **(B) pending**: `match` as a boolean command (DD#63 increment); the
  `-`-on-(B) compile error; the bare-`[[ ]]` disposition (open question).

## Candidate spec tests (spec/errors.sh, native unless noted)

Legend: [✓]=passing today  [~]=abort works, diagnostic pending  [ ]=new.

### (A) abort / catch / suppress + diagnostics

- [✓] A.ext.bare   `/bin/false` (bare)              → rc!=0, aborts        (55.1)
- [~] A.ext.diag   `/bin/false` (bare)              → rerr `*: command failed (exit 1)`   (new msg)
- [✓] A.ext.if     `if /bin/false; then…else…`      → caught, else branch
- [✓] A.ext.supp   `- /bin/false; echo ok`          → suppressed, ok       (55.4)
- [ ] A.ext.silent `if /bin/false…` / `- /bin/false`→ `-z $rerr` (no diag on caught/suppressed)
- [✓] A.cd.*       cd bare/if/`-`                    → 55.20/55.22/55.23 (+diag 55.20)
- [✓] A.echo.*     echo write bare/||/`-`            → 55.27/55.28/55.29 (+diag 55.27)
- [✓] A.rdr.*      redirect open bare/if/`-`/||      → 55.24/55.25/55.26/55.30 (+diag 55.24/55.26)
- [~] A.pipe.diag  `/bin/false | /bin/true` (bare)   → rc!=0 + `*: …: command failed (exit 1)`  (55.10 abort ✓)
- [~] A.cmdsub.diag`x=$(/bin/false)` (bare)          → rc!=0 + diag         (55.8 abort ✓)
- [~] A.bg.diag    `/nonexistent & wait` (bare)      → rc!=0 + `*: <name>: <reason>`  (55.16 abort ✓)
- [~] A.ovf.diag   `int x=MAX;(( x+=1 ));echo $x`     → rc!=0 + `*: arithmetic overflow`  (25.x abort ✓)
- [ ] A.regcomp    dynamic bad pattern (e.g. `match $s "$(getenv RE)"` w/ bad RE) → rc!=0 + `*: regexp error: …`

### (B) must-consume

- [ ] B.match.bare `match abc abc` (bare statement)  → COMPILE error (result discarded; new SBxxxx)
- [ ] B.match.if   `if match abc abc; then y; else n`→ rc=0, out=y (consumed)
- [ ] B.match.or   `match abc zzz || echo miss`      → rc=0, out=miss (consumed)
- [ ] B.match.and  `match abc abc && echo hit`       → rc=0, out=hit (consumed)
- [ ] B.match.disc `match abc abc || :`              → rc=0, no output (explicit discard)
- [ ] B.match.dash `- match abc abc`                 → COMPILE error (`-` meaningless on a (B) result)
- [✓] B.fn.bare    bare call of `-> int` fn          → SB4016 (55.13)
- [✓] B.fn.zero    return-0-only `-> int` fn         → SB4017 (55.14)
- [ ] B.fn.disc    `f || :` for `-> int f`           → rc=0 (explicit discard consumes the return)

## Open design questions (need DG)

1. **Bare `[[ … ]]` disposition.** A bare `[[ x = y ]]` produces a (B)
   boolean that is discarded. Is that a compile error (must-consume), or
   is a bare test an allowed assertion (DD#55 implicit exit check — a
   false test aborts)? These conflict; DG decides which.
2. **SB code + wording** for "(B) result discarded" on `match` (bare) and
   for "`-` on a (B) construct". Reuse SB4016's family or a new code?
3. **`<argv0>` form** — verbatim argv[0] (`./test_bin`) vs basename
   (`test_bin`). W3 ships verbatim; confirm.

## Notes

- Spec harness already captures runtime stderr in `$rerr` (native) for the
  diagnostic assertions; `err` stays compile-only.
- The (A) diagnostic message work is `abort-diagnostics-remaining`; the
  C/janet (A)-channel is `c-janet-dd55-abort-channel`. This task is the
  unifying contract that both feed.
