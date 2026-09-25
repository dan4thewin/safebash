# 410 — The diagnostic that could not fire

*2026-09-25. Coverage queue, continued. DG: "continue with coverage."*

## Starting point

Fresh `coverage.db` after the arithmetic fix: 519 uncovered lines, 333
uncovered branches. The by-function ranking was flat — nothing above five
lines — so ordering by count was no longer informative. I filtered the
uncovered set to lines that are *not* `unreachable!` / `panic!` guards and read
what was left.

Two of them were a user-facing error message:

```
sash/src/parse/parser.rs:1027-1031
  None => Err(parse_error(span, "SB4014",
      "return: expected a status producer (a command, `[[ … ]]`, or `(( … ))`)"))
```

An error message with zero coverage is a sharper signal than an uncovered
branch, because it is a promise to the user. Either no spec exercises it, or it
cannot fire. Those two possibilities have very different consequences.

## Establishing it could not fire

`parse_return` reaches that arm when `parse_return_producer_head` returns
`Ok(None)`. That head has four delegates. Grepping every `Ok(None)` in
`sash/src/parse/` showed three of them — `parse_test_command`,
`parse_arith_stmt`, `parse_simple_or_assignment` — contain **none at all**. The
fourth, `parse_negated`, has exactly one, and it fires only when
`dispatch_statement` returns `Ok(None)`.

`dispatch_statement` has no `Ok(None)` of its own; it routes to fourteen
subparsers, of which only `parse_negated` (again) and `parse_suppressed` can
produce one. `parse_suppressed`'s comes from `parse_statement` returning
`None`, which happens only at end of input — and `parse_suppressed` has already
established by `peek_n(1)` that a word-like token follows, so it is not at end
of input when it calls.

The recursion has no base case. `dispatch_statement` never returns `Ok(None)`,
and the coverage numbers agree line by line: `parse_statement:234` (the `None`
arm) **0**, `parse_negated:1051` **0**, `parse_suppressed:1132` **0**,
`parse_body_until:369` **0**, `parse_top_level_item:192` **0**.

Note the one that is *not* zero: `parse_statement:225`, the end-of-input
`Ok(None)`, runs **twice** — via `parse_trailing_pipeline`, which turns it into
SB4056 for `echo a |`. So the `Option` layer is not vestigial everywhere. It
is live in exactly one place and dead in the rest, which is why the cluster
looked plausible.

## The question that turned it into a bug

A diagnostic that cannot fire is only a tidiness problem if nothing needed it.
So: what *does* happen to the input it was written for?

```sh
f() {
  echo pre
  return if
  echo post
}
f; echo "rc=$?"
```

bash: `return: if: numeric argument required`, rc **2**.
sash: compiles, prints `pre`, rc **0**.

`parse_return` decides "is there a producer after `return`?" from the next
token: `[[`, `((`, `!`, or a non-numeric word. A reserved word is none of
those, so it falls through to `parse_simple_or_assignment` — the *numeric
status* path — which parses `return if` as a `return` command with an argument
and silently discards it.

Twenty of the twenty-one reserved words behave this way. (`END` escapes only
because it lexes as a `Word`, so it takes the producer path and reports
SB5011.) `Tok::Dash` and `Tok::Eq` do too:

| input | sash | bash |
|---|---|---|
| `return if` / `while` / `str` / `then` / `do` … (20) | rc 0 | rc 2 + error |
| `return =` | rc 0 | rc 2 + error |
| `return -` | rc 0 | rc 2 + error |
| `return - echo hi` | rc 0, **`echo hi` never runs** | rc 2 + error |

The last one is the worst: the command is neither run nor reported.

## DG's question, and two findings it retired

DG asked: *does all of this work/error appropriately if it's written as
`int x = …; return $x`? If so, can we reuse those underpinnings?*

Probing that turned up two more apparent divergences, and checking them against
the DDs retired **both**:

```sh
str s=abc; return $s     # sash rc 0   — bash: "numeric argument required", rc 2
int x=300; return $x     # sash rc 300 — bash: rc 44
```

**DD#89** settles both. *"A string argument coerces to int (atoi), as in every
other int context"* — so `atoi("abc") == 0` is the documented rule, not a hole.
And *"internally the value is a full i64 … at the process boundary it is 8-bit
(POSIX)"* — so `$?` holding 300 is the designed behaviour, with truncation
deferred to the boundary. I verified the boundary: the **program** exit status
is 44 in both shells. DD#89 is written about `exit`, but it states a property
of the exit-status channel, and `return` writes the same channel.

Two things that looked like bugs were the design working. Worth recording,
because the probe output alone said "divergence" three times and only one of
the three was real.

The answer to the question itself is *no, and then yes*:

- **No** to reusing the value path. `int x=3; return $x` works correctly, but
  the value machinery cannot help here, because `return if` never becomes a
  value — the token is discarded by the parser before anything evaluates it.
- **Yes** to a different reuse, and a better one. `keyword_to_str`
  (`parse/expr.rs`) is, by its own doc comment, *"the **only** list of
  reserved-word tokens in the word parser"*. Testing against it means a keyword
  added later is covered without a second list to keep in step — which is
  precisely the objection I had raised against a hand-written blacklist. The
  list already exists because it drifted once before (`Tok::Str` was missing
  from both copies).

So the fix is three lines of predicate, reusing the canonical list, and the
A-versus-B question I had queued dissolved.

## The fix

`parse_return_value_form` takes the non-producer path and rejects a reserved
word, `-`, or `=` with `return_producer_error` — the *same* helper the
(still-cold) `None` arm now uses, so the sentence exists once rather than
twice. DG chose to reject `return - CMD` outright, consistent with SB4019,
which already rejects `!` combined with `-` on one stage for the same reason:
`-` discards a status and `return` consumes one.

Extracting the helper was not only for tidiness. Inline, `parse_return` went to
cc 9 / **cog 6** against a 5.64 bar — the gate would have rejected the commit.
Split, it is cc 7 / cog 3, and the new helper is cc 3 / cog 2.

## Specs, and why they assert the message

`74.49`-`74.56`: five rejections, and three controls — a non-keyword word must
still take the *producer* path (SB5011, not the new error), and `return 7` /
`return $((1+1))` / `return "5"` must still work.

Every rejection asserts the **message**, not the code, and that is not
boilerplate caution. Error recovery emits a follow-on `SB4014 expected
expression` on the orphaned `}`, so `$err == *SB4014*` would have passed with
the guard removed entirely. Asserting `"return: expected a status producer"`
is what makes them real.

Neutering proved it in two halves, because a misplaced `&&` in the first
neuter left the `Dash`/`Eq` clause live: killing only the keyword test failed
exactly `74.49`-`74.50`, and killing both failed exactly `74.49`-`74.53`, with
the three controls passing throughout. The accident produced a sharper proof
than the one I set out to run.

## A pre-existing fault found by taking a baseline

Before accepting the follow-on diagnostic as normal, I ran the same shape
through an *existing* error to see what recovery usually does:

```
echo a |              (top level)    → SB4056 `|` has no right-hand side
f() { echo a | }      (in a body)    → SB4014 expected expression
```

The specific, correctly-spanned SB4056 is **lost** one nesting level down, and
replaced by a misleading error pointing at the closing brace. That is not
caused by this change — my new path reports both the real error *and* the
follow-on — but it is a genuine fault, and it has a second consequence worth
flagging: **a spec asserting a bare parse-error code inside a function body can
be vacuous**, satisfied by the follow-on alone. Filed as
`parse-error-in-function-body-loses-diagnostic` rather than fixed here, since
it changes diagnostic behaviour parser-wide.

## Gates

6376 (predicted exactly: 6360 + 8 × 2), zero failing, zero flips against
`9996b46`. `mk test` 106. fmt/clippy clean, complexity clear.

## What to take from it

The arithmetic bug in journal 409 was found by an execution *count* of one.
This one was found by a *message* with a count of zero — and the step that
mattered was refusing to treat "no spec covers it" and "it cannot happen" as
the same fact. They point in opposite directions: the first is a missing test,
the second is a hole in the language surface. Distinguishing them cost one
grep over `Ok(None)` and produced a bash divergence affecting twenty-three
distinct inputs.

And the DD#89 pass is the counterweight. Three probe results said
"divergence"; two of them were the design. A differential against bash is a
hypothesis, and the DDs are what test it.
