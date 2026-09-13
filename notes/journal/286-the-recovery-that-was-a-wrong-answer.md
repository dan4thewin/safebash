# 286 — the recovery that was a wrong answer

Two defects, one family, found by accident. I was re-measuring
`cmdsub-followups-taint-intrinsics-c` — whose one live gate was the C
backend's Phase-4 cmdsub-exec TODO — and wrote a throwaway probe that
happened to contain `$(( $(getpid) > 0 ))`.

The C backend turned out to be fixed: zero `TODO` markers in the emitted
C, `$(/usr/bin/printf hi)` captures `hi`, and native and C agree. That
gate is closed. But the probe printed `pid=0`, and a pid is never zero.

## What it was

`$(( … ))` containing a command substitution is not something sash
supports — bash does, sash rejects it. Four of the five positions I tried
rejected it exactly as they should:

| position | before |
|---|---|
| `int n=$(( $(f) + 1 ))` | SB4020 |
| `x=$(( $(f) + 1 ))` | SB4020 |
| `echo $(( $(f) + 1 ))` | SB4020 |
| `[[ $(( $(f) > 0 )) = 1 ]]` | SB4020 |
| `echo "$(( $(f) + 1 ))"` | **0** |

The fifth — inside a double-quoted string — printed `0` and compiled
clean. Same expression, same file, two answers depending on whether it
sat inside quotes.

Pulling on that found the cause was not about command substitution at
all, and was much wider:

```rust
StringPart::ArithSub(content) => {
    let arith = super::arith::parse_arith_str(content.trim(), span)
        .unwrap_or(crate::ir::ArithExpr::Literal(0));
```

*Every* arithmetic parse error inside a double-quoted string was
swallowed into a literal `0`. Not just the unsupported form — any of
them.

And then the second defect, which the first was hiding. In
`parse_prefix_atom`, the operand position of the Pratt parser:

```rust
Some(_) => {
    // Unknown token — try to recover
    self.advance();
    Ok(ArithExpr::Literal(0))
}
None => Ok(ArithExpr::Literal(0)),
```

A token that cannot begin an operand was **consumed** and replaced with
`0`. Consumed matters: `reject_trailing_token` (DD#72's "any leftover
token is a hard error") then found nothing left to complain about, so the
one check designed to catch this was walked straight past.

The measured results:

| input | before | bash |
|---|---|---|
| `$(( 1 + ))` | **1** | syntax error |
| `$(( 1 +* 2 ))` | **0** | syntax error |
| `"$(( @@@ ))"` | **0** | syntax error |

`$(( 1 + ))` → `1` is the one that bothers me most. The EOF arm returned
`0`, so the expression folded to `1 + 0`, and the answer is *plausible*.
Nothing about it looks like a failure. This is the worst class in the bug
policy: not a crash, not a diagnostic, a wrong number that a reader would
accept.

Note the two defects are independent, and each masked the other's blast
radius. The comment `// Unknown token — try to recover` is honest about
what it does. Recovery is a reasonable thing for a parser to want — for
an *editor* integration, where you keep parsing to offer completions. In
a compiler that emits a binary, recovering to `0` means emitting code for
input it did not understand, which is the thing DG has said it must never
do.

## The fix, and the part that argued with me

The operand-position arms become hard errors. That part is small.

The guard it needs is `for (( ;; ))`. The C-style loop header reaches
`parse_arith_str` as the literal text `;;`, which is not empty, so it
tokenizes to two separators, `skip_separators` eats them both, and the
operand position sees EOF — the exact case I was turning into an error.
`parse_arith_str` already had an is-empty early return; it now also
returns `0` for a run of nothing but separators. With that, EOF in
operand position genuinely means an operand was owed. `for (( ;; ))`,
`for (( ; i<2; i++ ))` and `for (( i=0; i<2; ))` all still work, and
`$(( ))` is still `0` as in bash.

The string-context swallow was the larger job, and I spent a while
talking myself out of doing it properly. `emit_string_part` had to become
fallible, and the propagation runs through `string_parts_to_exprs`,
`parse_default_word`, `optional_splat_word`, `build_sed_sub`,
`build_one_sed_sub`, `build_sed_or_malformed`, `parse_disposition_operator`,
`parse_trusted_operator`, `parse_standard_operator`, `parse_cell_operator`,
`try_sed_op`, `try_subscript` and `parse_parameter_expansion` — the whole
parameter-expansion layer, which was infallible by construction. A doc
comment on `try_sed_op` even says so: "the `${…}` value path cannot itself
raise a parse diagnostic". That is a design statement, and I was about to
file the whole thing as a decision for DG rather than change it.

What settled it was the decision tree's first step. The parser already
threads `Result<_, Diagnostic>` everywhere else — `parse_word_parts`,
`parse_splat_op`, all of `arith.rs`. Making one more path do the same is
applying an established convention, not choosing a new one. That is step
1, "settled by a source of truth", not step 4, "a consequential fork".

So I let the compiler size it instead of guessing. Thirteen signatures,
five rounds of `cargo build`, each round's errors naming exactly the next
layer. Several `Option<T>` returns became `Result<Option<T>, Diagnostic>`,
where the `None` keeps meaning "not this syntactic form" and the `Err` is
new. The `?`-on-`Option` idiom had to become `let … else` in four places.

## The one that would have shipped silently

With the thread in place the diagnostics appeared — as `SB4014 expected
expression`, for every case, instead of the specific message I had
written.

`parse_word_parts` calls `parse_quoted_arm`, which I had just made
return `Result<(), Diagnostic>`, and the call site had no `?`. The
error was dropped, `parts` stayed empty, and `parts_to_expr` reported the
empty-word error. A generic diagnostic pointing at roughly the right
place — plausible enough that I might not have looked twice if I hadn't
known what the message was supposed to say.

Rust warns on an unused `Result`, and `cargo clippy -- -D warnings` is a
pre-commit gate, so this would have been caught before the commit landed.
It is still worth recording: this is the third time this month that
*plausible* has been the property that made something hard to see. A
wrong pid of `0`, a `1 + ` that folds to `1`, and now a diagnostic with
the wrong text. None of them looked like failures.

## Gates

Fail-first: reverted the three parser files, rebuilt, predicted 8 of the
13 new specs would fail and named them — 31.40–31.44 and 31.46–31.48 —
and got exactly that set. The other five (31.45, 31.49–31.52) are the
ones that already passed: they pin the behaviour the fix was required not
to change.

Sweep **4330 passing, 9 skipped, 0 failing**, 0 flips (4304 + 26).

## Filed, not decided

Bash evaluates `$(( $(cmd) + 1 ))`. sash now rejects it consistently in
all five positions rather than in four of five, which is strictly better,
but "consistently rejected" is not the same as "decided". Whether a
command substitution should be *allowed* inside `$(( ))` is a
language-surface question and DG's to make — filed as
`arith-cmdsub-support.md`.
