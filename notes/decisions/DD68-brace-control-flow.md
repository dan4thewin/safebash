title: DD#68 — Brace-block control flow (`if`/`while`/`until`/`for`); `then/fi/do/done` as sugar
status: accepted

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

## Resolved (DG, 2026-06-21)

- **Converter default preserves `then…fi`/`do…done`** (they are valid
  sash sugar under DD#67); only `sash-convert`'s **canonicalize mode**
  normalizes them to `{ }`. Consistent with DD#67 (default = preserve,
  canonicalize-switch = native-only). Note: there is no `--emit-source`
  flag — the canonicalize mode lives in `sash-convert` (DD#67).
- **"expected `{` body" diagnostic** → next-free SB code at implementation
  (validate/parse-level, alongside the DD#66 codes). Covers a missing body
  and a brace-group-as-condition.
- **`select`** and other heads: sash has none; nothing to do.

See task `dd68-brace-control-flow` for the spec-first implementation.

## Addendum (2026-10-26): no empty condition, and a group is never one

A header's condition is the command list the grammar above describes, so a
header with none is an error. The keyword spellings compiled one: `if then`,
`elif then`, `while do` and `until do` tested whatever status the previous
command had left (`/bin/true; if then echo y; fi` printed `y`). They are now
SB4027 "this header has no condition", at the `then` or `do`. The brace
spelling was already rejected: a `{` straight after the keyword is reported
as the `{ }` group condition it most likely opens (`if { a } { b }`).

Rejecting the keyword form is decide-and-report, not a ruling; DG may veto
it in favour of keeping the form. It answers `q-empty-keyword-condition`,
which took an empty condition to be false because its probes ran nothing,
or `/bin/false`, before the header. With nothing before it, the condition
read a status that an explicit `$?` may not read there (SB5012 "no
preceding command"). The precedent is journal 311: a missing right operand
of `&&`, `||` or `|` was silently filled in with `true`, "a default that was
never a decision", and became a syntax error (SB4056).

The ban on a `{ }` group condition was enforced only when the group was a
statement of the condition's own list. A group still got in as an `&&`/`||`
operand (`if /bin/true && { /bin/true } { echo y }` printed `y`), as a
keyword `elif { …; }; then` (printed `y`), behind `-` (batch-531 B112:
`while - { … } { … }` ran its body) and behind `!` (`if ! { … }` took its
`else`). Each is now SB4027 "a `{ }` group cannot be a condition", wherever
it stands in the condition: a statement of the list, an operand, a pipeline
stage, or under `-` or `!`. The groups that were already rejected keep their
verdict, but the anchor moves from the statement's first keyword (1:1, even
for an `elif`) to the `{`. No spec pinned the old position; cnd.22 pins the
new one for an `elif`.

Only a group the user braced counts (decide-and-report). The parser builds
groups of its own: a mutating `(( a++, a > 0 ))` split into one statement per
part (DD#31), and a C-style `for` lowered to its init followed by the loop.
Each is still the one command the user wrote, so `if (( a++, a > 0 )) { … }`
and `while (( a++, a < 3 )) { … }` stay legal.

The ban is syntactic: in a header, the first top-level `{` opens the body. It
covers header conditions only, so `! { … }` stays legal as an `&&` operand at
statement level and as a `return` producer, where no body follows. (DD#66's
2026-10-26 addendum is different: a dispatch is not a boolean, so its ban
covers every boolean position.) A negated loop is a compound, not a group:
`if ! for x in a b { … } || /bin/true { … }` stays legal. Whether a compound
may supply a condition's status at all is a separate question, filed as
`q-compound-as-condition`.

Specs: errors.sh cnd.1-cnd.7, cnd.11, cnd.12, cnd.22, bng.21-bng.24.
Journal 537.

## Addendum (2026-10-27): a stray block keyword is SB4072 wherever a command may stand

DG's rule for this work (DD#55 addendum 2026-10-26): *"I don't see reserved
words being meaningful within a pipeline or in combination with &."* A
keyword that can only close or continue a block (`then`, `do`, `in`,
`esac`, `else`, `elif`, `fi`, `done`) means nothing anywhere a command may
stand. It used to fall to the ordinary-word path there, so a stray `fi`
reached validate as a command and drew SB5011 "command 'fi' not found",
whose advice to define it as a function nobody can take: SB5092 forbids
`fn fi`. Each is now SB4072, the code of a `}` that closes nothing (journal
453), with a message that says where the keyword does belong. That holds at
a statement head and for one closer too many, after `|`, `&&` and `&`,
behind `!` and `-`, as a `return` producer, and as a capture's first word
(skw.1-skw.17). DG named pipelines and `&`; covering every command position
is decide-and-report. Behind `-` the old report was SB5011 "command '-' not
found", at the `-`. As an argument a keyword is still a word (skw.34).

The lexer gives these keywords their own tokens in every position
(`classify_word_token`), and the parser accepts all eight where it expects
a name (`expect_word`), which is how `str fi=x` reaches SB5092. DD#125 says
instead that `in` and `do` lex as ordinary words outside their one
position; correcting it is filed as `doc-keyword-lexing-claims`. A parse
error stops the passes after the parser, so `fn in { … }` followed by a
call `in` is now SB4072 at the call, where it was SB5092 at the definition.
The definition alone is still SB5092.

**The `}` message.** SB4072's `}` text said "no block is open. bash would
keep it literal; quote it for a literal brace (`'a}b'`)". That was false
wherever a block is open (`{ /bin/echo a | }`, skw.18). The text now says
when a `}` does close a block (skw.18, skw.19), and keeps the quoting
advice.

**A block that runs into a closer** (decide-and-report). A header with no
`then` or `do` reads the lines after it as its condition until it runs into
a closer (skw.21-skw.23). An `if` body can run into its loop's `done`
(skw.24), or meet a `fi` that does not stand as a statement of its own
(skw.25). Each is now reported at that closer, as bash reports it. Before,
each was reported at the `if` or `while`, as SB4014 or SB4027 ("expected a
`{ }` body or `do`", "expected 'then' after elif", "expected 'fi'").

**Clauses.** An `elif` or `else` clause's errors point at the clause, not
at its `if` (skw.26-skw.28; `diag-elif-clause-has-no-span`). SB4027 at the
`{` is in the addendum above (cnd.22).

**After a parse error** (decide-and-report). Recovery skips to the end of
the line, which can drop a block's opener and leave its `then`, `fi` or
`done` behind. Calling that keyword stray would repeat the first error. So
once the parser has recovered, a keyword is not judged and takes the
ordinary-word path, which adds nothing once a parse error exists (skw.29
keeps its lone SB4020). A `}` is still judged, and still cascades
(`bug-parse-error-in-block-cascades-sb4072`).

**Inside an unclosed `$(`** (decide-and-report). A stray `}` or keyword
inside a `$(` that is never closed is SB4057 "unterminated `$(`", at the
`$(`. The `}` was written to close the block, and is stray only because the
open `$(` took it in. For a `}` this was SB4072 at the `}` (skw.30, skw.32,
skw.33). The `fi` spelling was already SB4057, because the stray `fi`
passed the parser as a word (skw.31). A later `$( … )` or `$(( … ))` does
not close the open one. When the `$(` is closed, the keyword keeps SB4072
(skw.16, skw.17).

**Not decided here.**
- A newline between a brace body's `}` and its `else` or `elif` (skw.20) is
  SB4072 at the keyword, plus a cascade SB4072 at the `}` that closes the
  clause's own body. Before, it was a lone SB5011 at the keyword. The new
  messages state the rule the parser has always applied (the clause shares
  the `}`'s line); no DD states it, and whether the form should be legal is
  `q-clause-on-next-line`.
- A missing command before a closer (`a && fi`) is SB4072 at the closer.
  With nothing after the operator it is SB4056 at the operator (`a &&` at
  the end of the input). Which code and position both should take is
  `q-sb4072-closer-diagnostics`.

Specs: errors.sh skw.1-skw.34. Journal 537.
