title: DD#68 — Brace-block control flow (`if`/`while`/`until`/`for`); `then/fi/do/done` as sugar
status: candidate

`if`, `while`, `until`, and `for` (incl. C-style `for ((;;))`) gain
**brace `{ }` bodies**, matching `fn name { … }` (already brace-bodied)
and the DD#66 dispatch form. The bash `then…fi` / `do…done` keywords are
kept as **1-to-1 sugar** (DD#67). From the 2026-06-19 discussion. This is
a **parser-only** change: the brace and sugar forms lower to the *same*
existing IR, so all semantics (catching-construct `$?`, `break`/`continue`,
redirects) are unchanged.

## Forms

```
if COND { … } [elif COND { … }]* [else { … }]
while COND { … }
until COND { … }
for NAME in LIST { … }
for ((init; cond; incr)) { … }
```

`else if COND { … }` is accepted as sugar for `else { if COND { … } }`
(it is literally `else` + a nested `if`, so it costs the parser nothing).
The classic `if COND; then …; [elif …; then …;] [else …;] fi` and
`while/until/for …; do …; done` remain accepted (DD#67 sugar).

## The condition grammar (the disambiguation)

In the **brace form**, the condition is a **command list** (pipelines,
`&&`/`||`, `;`/newline-separated) and the **first top-level `{` ends the
condition and opens the body.** A `{ }` group **may not** appear as an
`if`/`while`/`until` condition. `for`'s `LIST` is words, likewise ending
at the first top-level `{`.

This is unambiguous because of three existing choices:
- **`{` is reserved** everywhere (`echo {`, `echo a{` are SB4014; a literal
  `{` is quoted, `'{'`), so a condition word can never *be* a bare `{`.
- sash has **no `{a,b}` brace-expansion** — no mid-word `{`.
- **DD#66(b)**: a dispatch `[[ … { } … ]]` is statement-only, so it can't
  be a condition; the only `[[ ]]` condition is the boolean form, which is
  `{`-free and self-delimits at `]]`.

Banning a brace-group *condition* costs nothing: a group condition is
exactly a `;`-list, which is still allowed —
`if { a; b; }; then c; fi` ≡ `if a; b { c }`. All real conditions work:
`if cmd { }`, `if [[ x ]] { }`, `if a && b { }`, `if a | b { }`,
`if a; b { }`, `if ! cmd { }`, `if (( x )) { }`.

**Newlines:** the condition runs *to* the `{` (newlines separate condition
commands, as before `then`). A missing body is an error that consumes
following lines until a `{`/EOF — the same failure mode as a missing
`then`, so not new; the diagnostic should special-case "expected `{` body
after the condition."

**No mixing within one construct:** the form is fixed by the delimiter
after the condition — `then` ⇒ sugar (expect `fi`), `{` ⇒ brace (expect
`}`). `if c; then a }` and `if c { a; fi` are errors.

## What does NOT change

Parser-only. Both forms build the existing `If`/`While`/`For` IR. The
catching-construct `$?` reset (`is_catching_construct_exit`,
`codegen/entry.rs:263` + `emit_c.rs`), `break`/`continue`, redirects on the
head or body, and `elif`/`else` chaining are all unchanged.

## Relations

- **DD#67** (sugar policy): `then/fi`/`do/done` are *the* worked example of
  a 1-to-1 keep; default preserves them, the convert switch normalizes to
  braces.
- **DD#66** (dispatch `[[ ]]`): supplies the `{ }`-block aesthetic and,
  via (b), removes the dispatch-as-condition ambiguity.
- `fn name { }` already established `keyword … { block }`; this completes
  the family. (`name() { }` is re-added as 1-to-1 sugar — task
  `dd67-bash-sugar-sweep`.)

## Open / to confirm

- The "expected `{` body" diagnostic (new SB code) for a missing body and
  for a brace-group-as-condition.
- Whether the converter/`--emit-source` default round-trips `then/do` or
  normalizes to braces (DD#67 says default = preserve; confirm for these).
- `select` / other heads: sash has no `select`; nothing to do.

See task `dd68-brace-control-flow` for the spec-first implementation.
