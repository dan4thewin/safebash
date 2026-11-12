title: "sash-convert emits bash brace forms verbatim — a literal `a{b}` now fails to compile, and a redirected group always has"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "bash `echo a{b}` converts to something `sash` accepts and that still prints `a{b}`"
  - "a redirected or piped group either converts to an accepted form or is reported as unconvertible — it must not emit output that fails to compile"
  - "a plain `{ …; }` group keeps converting (it is still a sash surface) and keeps its meaning"

## What happens

`sash-convert` has no handling for either brace form — both survive because
neither is recognised as needing a rewrite.

| bash input | bash means | converter emits | `sash` verdict |
|---|---|---|---|
| `echo a{b}` | literal `a{b}` | `echo a{b}` | **SB4071** — new, see below |
| `echo a{echo z}` | literal `a{echo z}` | unchanged | **SB4071** — new |
| `{ echo one; echo two; }` | group | unchanged | accepted |
| `{ echo one; } > f.txt` | redirected group | unchanged | SB4021 — pre-existing |
| `{ echo a; } \| wc -l` | piped group | unchanged | SB4022 — pre-existing |

Measured 2026-09-30 against `target/release/sash-convert`.

## The word case is new, and is fallout from the SB4071 fix

`bug-brace-breaks-unquoted-word` was closed on 2026-09-30 by making a `{`
**glued to the preceding word** a parse error: sash used to end the command's
word list there and open an anonymous block, so `echo a{echo z}` printed `a`
then `z` where bash prints the literal.

bash keeps a brace-word literal whenever it is not a valid expansion — and
`a{b}` has no comma and no `..`, so it is literal. The converter passes it
through, and sash now rejects it. **A faithful conversion has to quote it**:
`echo 'a{b}'`.

Scope check needed: bash brace *expansion* (`a{b,c}` → `ab ac`, `{1..3}` →
`1 2 3`) is a separate and larger gap — sash has no brace expansion at all,
and the converter does not expand either. Decide whether that belongs here or
in its own filing before starting.

## The group cases are pre-existing

A plain `{ …; }` group still converts fine — the group surface was
deliberately **kept** (it is the only way to put more than one statement on
the right of `||`/`&&`). Only the redirect and pipe forms fail, and they have
always failed: sash answers both with "wrap the compound in a function"
(SB4021/SB4022).

`{ a; b; } > f` has no brace-free equivalent, so the mechanical conversion is
to synthesise a function:

```
fn __group_1 { a; b }
__group_1 > f
```

That is a real rewrite with a naming question attached (collision-safety with
user functions, readability of generated names, whether a converter should
invent identifiers at all). **This part is a design call — do not pick a
scheme without DG.** The alternative is to report the construct as
unconvertible and let the author write the function: honest, no naming
problem, but loses automation on a common bash idiom.
