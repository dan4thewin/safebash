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

**Statement boundaries (DG, 2026-11-09; implementation pending):**
the closing `}` of a completed construct, or its `fi`/`done`, needs
no additional semicolon before the next statement. `else`/`elif` still
belong to their `if`; this is not a rule that every `}` token ends a
statement. An enclosing closer also ends its body's final statement
without a semicolon. DD71 defines the whole-statement/value distinction
and the separator required after a binding list; enforcement belongs
to `q-statement-form-residuals`. Only otherwise legal constructs qualify,
not the retired standalone command groups.

**Conditional-block shorthand (DG, 2026-11-09; implementation pending):**
`cmd && { a; b }` is `if cmd { a; b }`, and `cmd || { a; b }` is
`if ! cmd { a; b }`. DG had already chosen the latter when retiring
general command groups on 2026-10-30; the success-path spelling is now
accepted too. These braces open an `if` body, not a standalone group.
DD70 defines the source boundary, DD55 the matching scope/status/checking
rules, and `retire-command-groups` owns implementation. Function and
dispatch bodies are not command groups and are unaffected.

## Word-list `for` requires an iteration list (DG, 2026-11-09)

A word-list `for` requires **`in` and at least one source item**.
Reject all three forms:

```sash
for x { ... }                 # missing in
for x; in a b { ... }         # semicolon before in
for x in { ... }              # explicitly empty list
```

The same requirements apply to keyword bodies: `for x; do ...; done`
and `for x in; do ...; done` reject too. A newline before `in` remains
legal; a semicolon does not. The ordinary semicolon after a populated
list in `for x in a b; do ...; done` is unaffected. C-style
`for ((init; cond; incr))` is a separate form and keeps its grammar.

DG rejected the proposed empty-list exception:

> an explicitly empty list does no work - pointless code should be an
> error, as it must be an unintended mistake

This is a pointless-code error, not an implicit argument loop or a
quiet no-op. Effects written in a body that cannot execute do not
give the loop work. Body placement, marks and redirects cannot supply
the missing list or legalize an invalid header.

**Data-dependent emptiness is different.** A written list or splat can
yield no items at runtime; this header rule adds no non-emptiness check
or proof obligation on that data. An empty string is one item, not an
empty list: `for x in "" { echo "[$x]" }` still iterates once.

For argument iteration, write `for x in @args` in a function or
`for x in $(argv 1)` at top level (DD75/DD60), retaining ordinary
taint and arity rules. The converter must expand Bash's omitted-list
argument iteration to those explicit forms. A genuinely empty Bash
`in` list is not the omitted-list form and must not become an argument
loop or be silently erased to evade the refusal.

**Enforcement is pending** in the existing P2 `q-for-without-in-list`,
now implementation work covering compiler and converter. DD70 records
the pointless-code rationale; journal 598 records the ruling. No new
diagnostic code or implementation architecture is selected here.

## Statements do not produce results (DG, 2026-11-09)

DG rejected treating control flow as an expression:

> sash is not a lisp.  there's a clear distinction between an expression
> context and a statement context. I think this is valuable because it
> necessitates a procedural approach that is easy to reason about.

`if`, `while`, `until` and `for` are **statements with no result**.
They use conditions and perform actions; neither their last condition
nor their last body command becomes the construct's value or status.
A skipped `if` body and a legal zero-iteration loop do not manufacture a
success result either. Brace/keyword spellings and C-style `for`
follow the same rule.

Reject a use that requires the construct's result: a condition,
negation, a status-return producer, or an `&&`/`||` continuation
decided by that result. Marks, redirects and placement inside another
body do not create a result.

This is a **result-use rule**, not a ban on every compound appearing
near a connector. DD74's distinction applies: a left/middle logical
clause supplies a decision, as does the tail of a consumed list;
an otherwise legal final action in a statement list does not.
Thus `cmd && if other { action }` remains an ordinary unconsumed action,
while using that final `if` to decide an enclosing condition rejects.
An earlier statement in an otherwise legal sequential condition list
also remains a statement, not the list's final result producer.

The conditional-block shorthands above are `if` statements too, not
exceptions with an implicit result. Ordinary statement execution,
inner condition handling and body checks remain. `$?` inside a legal
handling branch belongs to its condition; the control-flow statement
does not grant fresh permission after completion.

An explicit `return` inside a body still returns from its enclosing
function. A function can therefore perform procedural work and return
its chosen status or value; that is the function's result, not an
implicit result of its final `if` or loop. Likewise, an otherwise legal
stdout capture may contain control flow: its value is captured output.
DD55/DD122 pipeline aggregates and reader bodies keep their contracts.
Do not confuse those enclosing operations' results with a body
statement's nonexistent result.

**Enforcement is pending** in the existing P3
`q-compound-as-condition`, now implementation work rather than an open
question. General-group retirement retains its separate owner.
Journal 597 records the ruling; no diagnostic code is allocated here.

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

This header check is syntactic: the first top-level `{` opens the body.
Its implementation left `! { … }` legal as a statement-level `&&`
operand and as a `return` producer. That is historical implementation
behavior, not a permission under the accepted command-group retirement:
those source groups must reject (`retire-command-groups`, DD70).
The conditional-block shorthands above do not turn a group into a
condition or alter this header delimiter rule.

A negated loop is a compound, not a group, but it now also requires
rejection: the statement has no result for `!` to consume. The old
`if ! for x in a b { … } || /bin/true { … }` evidence belongs to the
pending enforcement in `q-compound-as-condition`, not an open policy
choice. DD66's dispatch-as-boolean refusal remains unchanged.

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

## Addendum (2026-10-29, batch-531 port F5): a `while`/`until` condition belongs to its loop

SB5081 already treats a `break` or `continue` in a `while` or `until`
condition as inside that loop. But `lower_while` pushed the loop's context
only after lowering the condition, so a `break` there left the *enclosing*
loop. `for i in 1 2 { while break { … }; echo "after while $i" }` printed only
`done` (B101). The push now comes before the condition, so the `break`
leaves the `while` and the loop body after it runs.

Specs `control_flow.sh` cf.n22, cf.n23; journal 542.
