title: "BUG — a here-string `<<<` with non-simple data (`<<< ${x:-f}`, `<<< $a[i]`, `<<< $(cmd)`) silently feeds EMPTY stdin"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "`cat <<< ${miss:-hi}` feeds `hi` (or rejects) — never silently feeds empty stdin"
  - "here-string data forms are consistent with bash's trailing-newline behavior, or the divergence is a documented decision"

> **Superseded by DD#83 (2026-08-30): sash has no here-strings.** DG ruled that
> `<<<` is dropped entirely (rejected at parse, SB4050) rather than fixed — it's
> exact sugar over `producer | consumer` in sash (no subshell, provenance-based
> taint, no word-splitting). The silent-empty data path is gone with the
> construct. See `notes/decisions/DD83-no-here-strings.md` and journal 216.


## Symptom (native + c)

A here-string whose data expression is not one of {Literal, SingleQuoted,
DoubleQuoted, SimpleExpansion, Concat} silently feeds an **empty** stdin:

```
cat <<< $v                 # v=hello → "hello"     (works — SimpleExpansion)
cat <<< ${miss:-fallback}  # → ""  (should feed "fallback")   BUG
cat <<< $a[1]              # a=(zero hs) → ""  (should feed "hs")  BUG
cat <<< $(echo hi)         # → ""  (bash: "hi")                BUG
```

Same silent-empty class as the redirect-*target* bug
(`bug-redirect-nonsimple-target-empty`, fixed 2026-08-30), but on the here-string
DATA path, which was deliberately left untouched by that fix.

## Cause

`codegen/redirects.rs::emit_herestring_redirect` lowers non-literal here-string
data via `lower_ir_redirect_expr`, whose `_ =>` arm is `redirect_expr_zero`
(zero-ptr, zero-len). ParameterExpansion / Subscript / CommandSubstitution /
Arithmetic data all fall through to empty.

Note also a probable **second** bug on the same path: `parse_redirect`'s
HereString arm only appends the bash trailing `\n` for Literal / SingleQuoted /
DoubleQuoted data (`_ => target_expr` adds none), so even the working
`<<< $v` may be missing the trailing newline bash always adds.

## Needs a DG decision (why this was filed, not fixed inline)

The redirect-target ruling (support scalars, reject complex) does **not**
transfer 1:1 — a here-string feeds stdin *data*, not a filename, and bash
legitimately supports `<<< $(cmd)` (command output as stdin). Open questions:

1. **Scalar data** (`<<< ${x:-f}`, `<<< $a[i]`): spill like redirect targets
   (the mechanism already exists — `spill_scalar_redirect_targets` just skips
   `RedirectOp::HereString` today; dropping that skip fixes these two forms and
   leaves `$(cmd)` untouched, since the spill only targets ParameterExpansion /
   Subscript).
2. **Command-substitution / arithmetic data** (`<<< $(cmd)`, `<<< $((x))`):
   support (bash does) or reject (consistency with the redirect-target SB4049)?
3. **Trailing newline**: make all data forms match bash's `<<<` (append `\n`),
   or is the current literal-only newline intentional?

## Found

2026-08-30, while implementing the redirect-target scalar fix — probing whether
the same silent-empty fallthrough reached the here-string path (it does).

## Closed 2026-09-10 (backlog prune)

Re-verified at HEAD: `<<<` is now **rejected outright** (SB4050) in every form
tested — `${x:-f}`, `$(cmd)`, `${a[i]}` and a plain `$x` alike. The
"silently feeds EMPTY stdin" behaviour cannot occur, because no here-string
compiles. The construct was dropped (DD#83; the converter task
`converter-herestring-to-pipe` rewrites bash's `<<<` to a pipe).
