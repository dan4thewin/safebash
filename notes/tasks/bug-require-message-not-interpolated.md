---
title: "`${x:?MESSAGE}` — the message is literal text, not an interpolating context"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "`y=\"\"; str z=${y:?$m}` with `str m=boom` aborts with `<prog>: boom`, not `<prog>: $m`"
  - "a tainted message (`str h=$(getenv HOME); ${y:?$h}`) is rejected SB5020, as DD#78 requires — today it is SB5050 (dead variable), i.e. not walked as a read at all"
  - "a spec pins native == c == bash for an interpolated `:?` message"
---

## Symptom

The `:?` abort message is emitted as literal source text. bash expands it.

```
$ sash -c 'f() { echo hi; }
str y=""
str z=${y:?$(f)}'      &&  ./a.out
./a.out: $(f)                      # <-- literal

$ bash -c 'f() { echo hi; }; y=""; z=${y:?$(f)}'
bash: y: hi                        # <-- expanded
```

Same for a plain variable: sash prints `$m`, bash prints `boom`.

## The DD#78 clause this contradicts

DD#78 ("disposition-default discipline") scopes itself to **every** disposition
default and names this one explicitly:

> - `${x:?MESSAGE}` — the abort message (printed to stderr; constrained for
>   uniformity and because it is eagerly materialized).

and requires the operand to be a checked **read** position — that is what
closed the `:-` taint hole. For `:?` the checker never walks the operand, so
the rule is unimplemented:

| program                                              | today    | DD#78 wants |
|------------------------------------------------------|----------|-------------|
| `str h=$(getenv HOME); str z=${y:-$h}`               | SB5020   | SB5020 ✓    |
| `str h=$(getenv HOME); str z=${y:?$h}`               | SB5050   | SB5020      |

`SB5050` is the *dead-variable* diagnostic — `h` is written and, as far as the
compiler is concerned, never read. That is the tell: the message operand is not
part of the value graph at all.

## Not a taint leak

Worth stating plainly so this is not mis-triaged as urgent: because the message
is *never expanded*, no tainted value can escape through it. The bug is an
oracle divergence from bash plus an unimplemented DD#78 clause, not a
confidentiality hole. That is why it is filed rather than fixed on the spot.

## Found by

Journal 432, while checking whether `emit_c.rs`'s "DD#77" citation on
`RequireValue` was wrong. It is **correct** — DD#77 §"`:?` un-prune scope"
re-added scalar `:?` everywhere after DD#65 pruned it, so the many `DD#77`
comments on `RequireValue` should be left alone. (DD#65's "`:?` is rejected for
SCALAR `${var:?…}` only" line is the stale one; it predates DD#77.)

## Resolved 2026-09-30

Two one-line changes, in two passes, and the second was mandatory — fixing
only the first turned a wrong rejection into a **taint escape**.

**Parse.** `parse_disposition_operator` (`parse/expr.rs`) ran
`parse_default_word` for `:-` and built an `ExprKind::Literal` for everything
else, `:?` included. Now `:-` and `:?` share the interpolating branch, which is
what DD#78 says: the message is a disposition default like any other.

**Taint.** `check_param_expansion_taint` gated its operand walk on `:-`, with a
doc comment asserting that "`:?` … carr[ies] a *literal* operand, DD#65, which
is clean by construction". True before the parse change, false after — so on
the first build `${y:?$h}` with tainted `h` **compiled clean**. Before the fix
it was rejected SB5050 (dead variable); a fix that made it compile would have
been a regression, and a confidentiality hole, since the message is written to
stderr. The gate is now `matches!(…, Some(":-") | Some(":?"))` and the doc
records why.

`pexp_uses_tainted` (propagation) needed no change — it already walked
`pe.operand` unconditionally.

Verified against bash: `boom`, `count 5`, `pre boom post` all match. `:!?`
vouches for the subject only and still owes an attest for the operand, exactly
as `${x:!-$y}` does.

The array form `@{a:?MSG}` already interpolated. Its tainted case is rejected
by the SSA backstop with no SB code — that is
`bug-taint-escape-in-cmdsub-body`'s territory, not a separate gap.

Specs `vm.req4`-`vm.req8` (var_modifiers.sh, plan 151 -> 156) and
`dd78.8`-`dd78.12` (taint.sh, plan 264 -> 269). Sweep 7007.
