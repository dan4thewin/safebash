title: "DD#111 — a value intrinsic is exempt inside `$( )` only when it *is* the substitution"
status: accepted

1. A value-only intrinsic is legal in `$(…)` only as its lone top-level
   statement, where it is rewritten to a value. It has no ordinary
   statement-position meaning, including elsewhere in a capture body.
2. Capturing a statusless value leaves no result for `-` to discard.
   `getenv`'s separate lookup status must be explicitly handled or discarded;
   the string's taint is independent (DD101).

---

# DD#111 — a value intrinsic is exempt inside `$( )` only when it *is* the substitution

## The rule

`status`, `getpid`, `getcpid`, `getenv`, `len`, `join` and the rest of
`VALUE_ONLY_INTRINSICS` produce a value and have no statement-position
meaning. DD#23's machinery makes that SB5073 everywhere — except inside a
command substitution, where `$(status)` is the whole point.

That exemption is now **exactly as wide as the rewrite that justifies it**: an
intrinsic is exempt only when it is the lone top-level statement of the
substitution body. Anywhere else in the body it is SB5073.

```sh
echo "$(status)"                # fine — the intrinsic IS the substitution
echo "$(echo a; status)"        # SB5073
echo "$(status; echo a)"        # SB5073
```

<del>

```sh
x=$(if [[ 1 = 1 ]]; then status; fi)   # SB5073 — nested, so nothing rewrites it
echo "$(echo a; echo $?)"       # fine — `$?` is punctuation, legal anywhere
```

</del>

DD118 governs equality and vacuous tests; DD55 governs status permission,
including fresh permission at capture entry.

The whole-substitution exemption does not make every prefixed spelling
legal. DG rejected `$(- toupper abc)` and other value-intrinsic captures
with no separate status to discard (DD55, 2026-11-08; journal 590).
The capture consumes their value. `getenv` is different because DD101
gives it a lookup status as well. That status must be explicitly consumed
or discarded, even when the string is used; `$(- getenv NAME)` discards
only the status and leaves the value tainted. The 2026-11-12 obligation is
pending in `require-getenv-status-handling`; statusless-prefix rejection
remains with `reject-statusless-intrinsic-suppression`. Neither grants new
statement-position meaning to an intrinsic or changes the sole-statement rule.

DD132's 2026-11-09 **provisional** trial gives `sub`/`gsub` a real
match/no-match boolean alongside their text. Captures may then discard
that boolean with `$(- sub ...)` / `$(- gsub ...)`, or consume it through
DD88's regex capture-condition extension. This does not exempt other
status-less intrinsics, widen the sole-statement rule, or invent an
unrestricted bare statement form. Substitution sugar explicitly projects
text and discards the boolean; the trial remains unimplemented.

## Why

Variables are not magic in sash. `$?` is not a special parameter the way bash
has one; it is **punctuation sugar for the `status` intrinsic**, and the same
holds for `$$`/`getpid` and friends. The intrinsic is the real thing.

`bareword_shortcut_expansion` (`parse/expr.rs:677`) is what makes `$(status)`
mean anything: it recognises the substitution and rewrites it into the value.
It opens with `let [stmt] = stmts else { return None }` — **one statement
only**.

The validator meanwhile exempted a value intrinsic from SB5073 *anywhere*
inside `$( )`. The exemption was wider than the rewrite. With two statements
nothing rewrote the intrinsic, every compile-time check passed (`status` really
is a registered builtin), and it was dispatched as an external:

```
$ sash -c '/bin/true; /bin/echo "$(echo hi; status)"' -o t && ./t
sash: bug: bare command passed to sash_exec (expected absolute path)
./t: status: command failed (exit 127)
```

An internal assertion message substituted into the program's own output. This
is *verbatim* the failure `bareword_shortcut_expansion`'s own doc comment says
it exists to prevent — the original fix handled the single-statement case and
left the rest.

## Why not widen the rewrite instead

The alternative was to give a bare `status` a statement-position meaning inside
a body, so `$(echo hi; status)` would print `hi` then `0`. Rejected: **that
spelling already exists and already works.** ~~`$(echo hi; echo $?)` prints
exactly that, today, because `$?` is legal punctuation in statement position.~~
Widening would buy a second spelling for the same thing at the cost of
contradicting the rule SB5073 states — that a value intrinsic has no statement
position — everywhere else in the language.

Narrowing keeps one rule with one exception, and the exception is the shape the
parser can actually honour.

## Diagnostic

Same code, SB5073, because it is the same rule. Different wording: the
statement-position message says the intrinsic is "only valid inside a command
substitution", which would be actively misleading for a use that *is* inside
one. The substitution-body message names the real constraint and points at the
capture spelling:

```
`status` is a value intrinsic — inside `$( … )` it is recognised only when it
is the whole substitution, not one statement among several. Capture it first
(`x=$(status)`) and use `$x` here
```

## Scope

Only bodies that would not otherwise be rejected reach this. A body containing
a real external (`$(/bin/true; status)`) is SB5020 on taint first. Bodies built
from builtins are the live path, and were the silent ones.

## Notes

Found by the branch-coverage loop, probing `parse/expr.rs:678` — the
`let [stmt] = stmts` arm itself. The coverage spec for that branch (`cov.204`)
uses `"$()"`, deliberately, so no spec pins the buggy shape.

Specs `23.25`–`23.32` in `spec/commands.sh`. `23.24`, which pins the exemption,
is unchanged and still passes.
