---
title: "braceless `$a[K]` accepts a narrower index grammar than braced `${a[K]}` — and fails as SB5064"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "DG rules on scope: extend the braceless read to DD#80's full index grammar, or codify the restriction and diagnose it as one"
  - "`$a[$(f)]` reports the capture rule (SB4030), not SB5064 — the sigil is not the mistake"
  - "`echo $a[$i]` and `echo \"$a[$i]\"` agree"
  - "`str s=z; echo $s[$(( 0 ))]` is SB5068, like `$s[1]` and `$s[$i]` already are"
  - "specs pin every row of the matrix below in whichever direction DG chooses"
---

# braceless `$a[K]` has a narrower index grammar than `${a[K]}`

Found 2026-10-03. DG asked "I thought braceless array access was supported"
after the capture-as-index work (DD#80 addendum, `5ac1b2b`) was described
entirely in braced spellings. It is supported — DD#33 read-side decision B
(`notes/tasks/nonbrace-dynamic-subscript.md`, DG 2026-06-21) deliberately
made `$a[$i]` read `${a[$i]}`, diverging from bash. But that decision was
scoped to one index shape, and the gap was never written down.

## Measured (2026-10-03, `target/release/sash` at `035ccb9`)

`array a=( p q r )`, `int i=1`, `map m=( [k]=v )`, `fn f { echo 0 }`,
`fn g -> int { return 1 }`.

| index `K`              | `${a[K]}` | `$a[K]`  | `"$a[K]"` |
|------------------------|-----------|----------|-----------|
| `1` (int literal)      | ok        | ok       | ok        |
| `k` / `"k"` (map key)  | ok        | ok (bare) / **SB5064** (quoted) | — |
| `$i`                   | ok        | ok       | **SB5064** |
| `${i}`                 | ok        | ok       | **SB5064** |
| `$(( i + 1 ))`         | ok        | **SB5064** | **SB5064** |
| `$(( (g) ))`           | ok        | **SB5064** | **SB5064** |
| `$(f)` (capture)       | SB4030 ✓  | **SB5064** | **SB5064** |
| `${b[1]}` (nested)     | SB4030    | **SB5064** | **SB5064** |

The **write** side has no such restriction: `a[$(( i + 1 ))]=Z` works, as do
`a[1]=Z` and `a[$i]=Z`. So the narrowing is specific to the braceless *read*.

## Why

Three different parse paths, each with its own idea of what a subscript is.

1. **Braced** `${a[K]}` — `try_subscript` (`parse/expr.rs:992`) uses
   `capture_balanced_subscript` on the raw string, so every DD#80 §3/§4 form
   (nested `${…}`, `$(…)`, `$(( … ))`, a quoted key) is captured and
   re-parsed. This is the full grammar.
2. **Braceless, unquoted** `$a[K]` — the lexer breaks a word on `$`, so the
   parser sees tokens. `try_subscript_arm` (`:549`) handles the case where the
   whole `[K]` arrived as one `Word` (an int literal, a bareword key, `${i}`);
   otherwise it falls to `try_split_subscript_arm` (`:731`), which matches a
   **fixed three-token shape** — `Word("[")`, `Dollar|DollarBrace`, `Word("]")`.
   `$((` lexes to `DollarDblParen`, `$(` to `DollarParen`, and a quoted key to
   a quote token, so none of them match.
3. **Braceless, inside `"…"`** — `try_emit_array_subscript_in_string` (`:680`)
   requires the part after `$name` to be a single `StringPart::Literal`
   beginning `[` and containing `]`. A `$i` index is its own
   `StringPart::Expansion`, so even the shape decision B sanctioned does not
   work here. **`echo $a[$i]` prints `q`; `echo "$a[$i]"` is SB5064.**

When the subscript does not parse, `$a` falls out as a bare scalar read of a
collection and `validate` reports **SB5064**.

## Why SB5064 is the wrong diagnostic here

SB5064 reads:

> `$a` reads the collection `a` with the scalar sigil, which is neither the
> collection nor element 0 — use `@a` for its values, `${a[i]}` for one
> element, or `$(len a)` / `$(join a SEP)`

For `$a[$(( i + 1 ))]` the sigil is not the mistake — the program already
*is* an element read, and `${a[$(( i + 1 ))]}` is accepted. The advice happens
to name the fix, but for the wrong reason, and the reader has to guess that
the brace is load-bearing.

For `$a[$(f)]` it is worse: the advice sends the reader to `${a[i]}`, and
`${a[$(f)]}` is **SB4030**. That is exactly the two-hop dead end
`array_index_error`'s doc comment forbids, and the reason the capture arm was
added to it last commit — the arm is simply unreachable from this spelling.

No unsafe program compiles either way; this is a diagnostic-quality and
language-surface-consistency bug, not a soundness hole.

## Options

**A. Extend the braceless read to DD#80's full index grammar.** Make all
three paths agree with the braced form and with the write side. The braceless
form becomes sugar that is *always* available, and every DD#80 rule (including
the new capture rejection) is reached from every spelling.
*Cost:* `try_split_subscript_arm` has to gather a **balanced token run** to the
matching `]` instead of a fixed three-token shape, and stay speculative
(consume nothing on failure). The in-string path needs the same treatment over
`StringPart`s. Moderate; the braced path's `capture_balanced_subscript` is the
model but works on a string, not tokens.
*Risk:* speculative multi-token lookahead is where parser bugs live. I first
wrote that the scalar base was a hazard — that extending the gather would
change the bash-compatible meaning of `$s[K]` on a string. **Measured, the
opposite is true, and it is a second symptom of the same cause:**

```
str s=z;          echo $s[1]            # SB5068   (bash: z[1])
str s=z; int i=0; echo $s[$i]           # SB5068   (bash: z[0])
str s=z;          echo $s[$(( 0 ))]     # compiles, prints z[0]
```

sash **deliberately rejects** a subscript on a scalar — SB5068, added after
`${s[0]}` was measured to segfault in six spellings and ICE in a seventh
(`validate.rs:7647`). The three rows above are the same mistake, and the third
escapes the check for the same reason the capture escapes SB4030: the
subscript never parses, so nothing is asked. The concat is harmless in itself
(no table deref), but the inconsistency is real and it is on the safety
diagnostic's own ground.

So extending the gather does not break bash parity for scalars — sash already
gave that up on purpose. It closes a hole in SB5068.

**B. Codify the restriction and diagnose it as one.** Write into DD#80 (or a
new DD) that the braceless read takes an int literal, a bareword key, or
`$var`/`${var}` only, and that every computed index requires the braced form.
Replace the SB5064 fallout with a dedicated diagnostic naming the brace as the
fix. Also fix row 3 so `"$a[$i]"` agrees with `$a[$i]` — that one is an
outright inconsistency within decision B's own scope, not a scoping choice.
*Cheaper, and defensible:* `${ }` is the explicit construct; the braceless
form is sugar for the common case.
*Against:* the asymmetry is a trap, and it is invisible until you hit it.

**C. Status quo.** Undocumented, SB5064, no spec. Not recommended — the
capture arm added in `5ac1b2b` is unreachable from half the spellings.

## Recommendation

**A**, reversing what I first wrote. I filed B on the grounds that `${ }` is
the explicit construct and a computed index is where explicitness earns its
keep — then measured the scalar rows above and the argument did not survive.
The narrowing is not a scoping decision anyone made; it is one parse path
matching a fixed three-token shape, and its fallout now reaches two separate
safety diagnostics (SB4030's capture arm is unreachable from half the
spellings, SB5068 has a hole). B would mean writing a rule to describe an
accident.

Honest estimate for A: a day of parser work plus the specs, done as one change
across all three paths — a partial fix leaves a differently-shaped version of
the same trap.

Row 3 (`"$a[$i]"` disagreeing with `$a[$i]`) should be closed either way. It
is not a scoping choice, it is a hole inside decision B's own stated scope.

## Not covered today

No spec exercises `$a[$(( … ))]`, `"$a[$i]"`, or `$m["k"]`. The braceless
specs that exist (`spec/arithmetic.sh` 565-615, 1727-1772; `spec/argv.sh`
181, 203; `spec/commands.sh` 273-334) all use an int literal or `$i`.

## Done (2026-10-03)

DG ruled: **"all paths must agree"** — option A, with the implementation
being a desugar rather than a third parser.

`Scanner::braceless_subscript` and `Scanner::dq_subscript_or_variable` emit
`Tok::DollarBrace("name[idx]")` / `StringPart::Expansion("name[idx]")`, so
`$a[K]` and `"$a[K]"` literally *become* `${a[K]}` in the lexer. The three
narrow paths are deleted: `try_subscript_arm`, `try_split_subscript_arm`,
`try_emit_array_subscript_in_string`, `subscript_expr`. `try_subscript` is
the one surviving subscript grammar. The byte-level bracket matcher both
callers need was extracted as `balanced_subscript_close`.

All three gates met. The five ICEs found while measuring (`$#[0]`, `$?[0]`,
`$$[0]`, `$![0]`, `$1[0]`) are closed by construction, and the SB5068 hole
(`str s=z; echo $s[$(( 0 ))]` printed `z[0]`) with them.

Two specs pinned the old asymmetry and were repointed, not deleted:
`80.34` (`$arr[$1]` was SB5064, now SB4030 like its braced twin — `80.34b`
is the twin) and `cov.110` (`$a[$i-]` was SB5064, now SB4041).
`dynamic_subscript_var`'s decline arm is still exercised from the write side.

Gated on `arith_depth == 0`: see the DD#80 amendment for why.

**Left for DG** (recorded in `notes/status.md`): whether `$(( $a[1] ))`
should stay SB4035 now that the desugar dissolves its stated reason, and
whether a cell takes a brace-less disposition (`$a[0]:-z` is literal concat
today, `${a[0]:-z}` is the disposition; DD#90's scope list omits cells).
