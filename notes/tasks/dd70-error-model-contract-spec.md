title: "DD#70 (A)/(B) error-model — comprehensive contract spec"
status: done
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
- **(B) pending**: `match` as a boolean command (DD#63 increment); bare
  boolean / bare `[[ … ]]` discarded → SB4018; `-` on a (B) construct →
  SB4019.

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

### (B) must-consume  (booleans must be consumed — compiler-enforced, DG)

Legend below: [✓N]=native only (transform diagnostics not yet on c/janet).

- [✓N] B.match.bare `match abc abc` (bare statement)  → SB4018 (regex.sh 63.2e)
- [ ]  B.test.bare  `[[ a = a ]]` (bare statement)    → SB4018 (NOT YET — bare [[ ]])
- [✓] B.match.if   `if match abc abc; then y; else n`→ rc=0, out=y (regex.sh 63.1c/1d)
- [✓] B.match.or   `match abc zzz || echo miss`      → rc=0, out=miss (regex.sh 63.2b)
- [✓] B.match.and  `match abc abc && echo hit`       → rc=0, out=hit (regex.sh 63.2a)
- [✓] B.match.disc `match abc zzz || :`              → rc=0, no output (regex.sh 63.2d)
- [ ]  B.test.disc  `[[ a = a ]] || :`                → rc=0, no output (with B.test.bare)
- [✓N] B.match.dash `- match abc abc`                 → SB4019 (regex.sh 63.2f)
- [✓] B.fn.bare    bare call of `-> int` fn          → SB4016 (55.13)
- [✓] B.fn.zero    return-0-only `-> int` fn         → SB4017 (55.14)
- [ ] B.fn.disc    `f || :` for `-> int f`           → rc=0 (explicit discard consumes the return)

## Resolved (DG, 2026-06-25)

1. **Bare `[[ … ]]` / any boolean → must-consume compile error** (SB4018).
   A boolean must be consumed; enforced by the compiler. A bare test is
   NOT an allowed false→abort assertion. Consume with `if`/`||`, or
   discard explicitly with `[[ … ]] || :`.
2. **Error codes adjacent to the ignored-return family.** SB4018 =
   discarded (B) boolean (bare `match` / bare `[[ … ]]`); SB4019 = `-` on
   a (B) construct. Kept in the SB401x band next to SB4016/SB4017. (SB5012
   `$?`-staleness is a separate, status-read concern in the 50xx band.)
3. **`<argv0>` = verbatim argv[0]** (`./test_bin`), not basename. W3 ships
   this; confirmed.

## Notes

- **Finding (2026-06-25): transform diagnostics are native-only.** The
  whole SB40xx must-consume family (the existing SB4016/SB4017 and the new
  SB4018/SB4019) is surfaced only on the native path — `sash --emit-c` and
  `sash --janet` exit 0 on a bare `match` / bare `-> int` call. So all
  must-consume *compile-error* specs are `in_modes native` for now;
  surfacing transform diagnostics on the C/janet CLIs is a precondition to
  un-guarding them (relates to `c-janet-dd55-abort-channel`).
- `match`'s boolean *behavior* (if/&&/||/`|| :`) IS implemented on all
  three backends (commit bbe5816) — native (sash_regex_match), C
  (sash_regex_match), janet (sash-regex `matchq`, exit-only/no stdout).
- Spec harness already captures runtime stderr in `$rerr` (native) for the
  diagnostic assertions; `err` stays compile-only.
- The (A) diagnostic message work is `abort-diagnostics-remaining`; the
  C/janet (A)-channel is `c-janet-dd55-abort-channel`. This task is the
  unifying contract that both feed.
