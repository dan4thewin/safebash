---
title: All paths must agree
date: 2026-10-03
---

DG, reading a message of mine written entirely in braced spellings:

> I thought braceless array access was supported...

He was right. `$a[1]`, `$a[$i]`, `$m[k]` all work — his own decision B from
2026-06-21. What I had not noticed, and what measuring found, is that the
brace-less spelling supports a *different grammar* from the braced one, and
the interpolated spelling a third, narrower still.

## Three parsers, three answers

| spelling | path | index grammar |
|---|---|---|
| `${a[K]}` | `try_subscript` | the full DD#80 rule |
| `$a[K]` | `try_subscript_arm` | `[`, `$ident`, `]` — a fixed three-token shape |
| `"$a[K]"` | `try_emit_array_subscript_in_string` | an integer literal |

So `echo $a[$i]` prints `20` and `echo "$a[$i]"` is SB5064. `$a[$(( i ))]`
is SB5064 where `${a[$(( i ))]}` computes the index. A quoted key, a nested
`${…}`, a capture — none of them parse as a subscript outside braces.

The narrow paths do not *reject* a richer index. They decline to see a
subscript at all, so `$a` is left as a bare scalar read and the complaint
that surfaces is SB5064: "you read the collection with the scalar sigil."
The diagnostic is describing a mistake the author did not make.

## I filed the wrong recommendation first

I wrote the task up recommending **B** — codify the narrowing, teach SB5064
to explain it. The brace-less form is sugar for the common case; the braced
form is always available; documenting a small grammar is cheaper than
unifying three parsers. That reasoning is fine and it was wrong, because I
had only measured the *collection* rows.

Then I measured the scalar rows.

sash deliberately rejects a subscript on a scalar. SB5068 exists because
`${s[0]}` once segfaulted, in six spellings, and the fix enumerated them.
But:

```
str s=z
echo $s[$(( 0 ))]     # compiled, printed  z[0]
```

The brace-less path only reaches SB5068 for the index forms its gather
understands. For the rest it declines, `$s` becomes a plain read, `[$(( 0 ))]`
becomes literal text, and the program compiles. A safety diagnostic that one
spelling walks around is not a fix. I reversed the recommendation to A and
told DG I had reversed it, which is the part worth recording — the filing is
in the repo and someone would have read B as the considered answer.

DG:

> all paths must agree.

## Five ICEs, found on the way in

Before designing I probed the special parameters, on the theory that a
narrow gather would have odd edges. It had worse than edges:

```
$#[0]  $?[0]  $$[0]  $![0]  $1[0]
```

All five panic in codegen — `ICE: undefined array: #`. Each builds a
subscript on a collection named `#`/`?`/`$`/`!`/`1` and nothing between the
parser and `find_array_slot` objects. Every braced counterpart is a clean
rejection: SB4038, SB4034, SB4047, SB4003, SB4047.

The instructive part is SB5031 — `$@` isn't subscriptable — whose doc
comment *describes this exact crash*. It is the same bug in the same family,
fixed for the two members someone happened to try. That is what a point-fix
on a forked path looks like a few months later.

## The fix is a desugar, not a third implementation

The obvious move is to teach the two narrow paths the full grammar. That
writes the index grammar a third time, in a codebase whose standing
convention — earned three separate ways — is never to fork a shared walker.

Instead the lexer rewrites the spelling. `braceless_subscript` emits
`Tok::DollarBrace("a[K]")`; `dq_subscript_or_variable` emits
`StringPart::Expansion("a[K]")`. By the time any parser sees it, `$a[K]` *is*
`${a[K]}`. Downstream there is exactly one subscript grammar, so the
spellings cannot drift apart again — not "should not", cannot. `try_subscript`
survives; `try_subscript_arm`, `try_split_subscript_arm`,
`try_emit_array_subscript_in_string` and `subscript_expr` are deleted.

This is not a new idea here. `dollar_or_braceless_disposition` already does
it for `$x:-foo` (DD#90). The byte-level bracket matcher that both the braced
parser and the lexer now need came out as `balanced_subscript_close` — one
scanner, documented as the seam, with a note not to grow a second.

## The carve-out that nearly went unnoticed

`$(( $a[1] ))` is **deliberately** SB4035: inside arithmetic the subscript
spelling is the bare `a[i]` (DD#79 model B), and specs `dsx.1`/`dsx.2` pin
the rejection.

`collect_arith_content` re-serialises the main lexer's tokens back to text
through `tok_to_text`, which renders `DollarBrace(c)` as `${c}`. So an
un-gated desugar would have handed the arith lexer a `${a[1]}` it accepts,
and a deliberate rejection would have become an accept **silently** — the
specs would have gone red, but only after I had convinced myself the change
was a pure win. Gated on `arith_depth == 0`.

Worth raising with DG separately: the *stated reason* for that rejection —
"can't preserve a `$`/variable index cleanly, it would silently read `a[0]`"
— is dissolved by the desugar. The rejection could now be a choice rather
than a limitation.

## Two specs pinned the bug

The sweep came back with `80.34` and `cov.110` failing in both modes. Both
were specs *of the old asymmetry*:

- `80.34` asserted `$arr[$1]` → SB5064, with a comment reading "(`${arr[$1]}`
  — 80.31 — is how you spell this one.)" In other words the spec recorded,
  in prose, that the two spellings disagreed. It now asserts SB4030, and
  `80.34b` asserts the braced twin says the same thing — the pair *is* the
  agreement.
- `cov.110` covered `dynamic_subscript_var` declining on the read side. That
  path is gone; the function's decline arm is still live from the write side
  (`a[1]=…`), so the coverage claim survives, repointed.

Neither was edited to make the diff green. A spec that pins a divergence is
load-bearing evidence right up until the divergence is ruled on, and then it
is a record of what changed.

## What landed

Sections N and O of `spec/subscript.sh`, `80.70`–`80.96`: every index form
(literal, `$var`, `${var}`, `$(( … ))`, a DD#58 sexpr call, a nested
subscript, a capture, a quoted key) in the two spellings that used to
diverge; the map read; the scalar SB5068 hole in both spellings; `${@[0]}`
and `${*[0]}`; and the five ICEs.

Predicted +28 ids × 2 modes = +56. Sweep: native 3839, c 3621 + 13 skipped,
total 7473, zero failing — the prediction exactly, which is the only reason
to trust that nothing collided.

`mk test` green.
