# 385 — The integers that became zero

`exit 1x` exited **0**. Not 2, not 1 — zero, the success code, from a
script whose author wrote a failure.

This came out of a question DG asked about the previous entry's work:
*is substring the only `var:` type operator that requires ints?* The
answer was yes — and finding it meant enumerating every position in the
language where a user writes an integer, which is how the second one
turned up.

## The question, answered

`try_substring_op` (`parse/expr.rs:1282`) accepts a `${var:…}` operand
when the character after the colon is a digit, `-`, or `(`. It strips
parens and stores the rest as a `Literal` **string**. The disposition
family (`:-`, `:+`, `:=`, `:?`) is excluded before that gate, so it never
competes.

So: yes. Among `${var:…}` operators, substring is the only one that takes
an integer. Every other integer position in the language is already loud
about a non-integer:

| position | code |
|---|---|
| subscript `a[i]` | SB4041 |
| array slice | SB5033 / SB4038 |
| `return N` | SB5011 |
| `signal N` | SB4059 |
| fd redirect `N>` | matches bash (`4294967296` becomes an argument) |

Two positions were not: the substring operand itself, and `exit`.

## Nine fallbacks, and the one that mattered

Both traced to the same shape — `parse::<i64>().unwrap_or(0)` at codegen,
with nothing upstream checking. There are nine such sites. The
interesting question is which are *reachable* with garbage, and reading
the call sites does not answer it. Instrumenting does.

I put a distinct `panic!` at all six candidates **in one build** — the
rebuild is three to four minutes and doing it six times is most of an
hour — then probed:

```
exit 1x        → SITE-LVAI     (codegen/values.rs:1345)
${s:1 2}       → SITE-SUBOFF   (codegen/value_handlers.rs:403)
${s:1x}        → SITE-SUBOFF
${s:1+1}       → SITE-SUBOFF
${s:0:3x}      → SITE-SUBLEN   (codegen/value_handlers.rs:406)
```

Three reachable, in two functions. The fd one at `parser.rs:2236` is
**provably** unreachable: both call sites guard with `is_fd_number`, which
requires one or two ASCII digits.

## The discriminator came from the sweep, not from thinking

Before touching anything I ran the **whole spec suite** under that
instrumentation. Eight specs failed — `86.5`, `ud.6`, `94.1`, `94.2`,
`94.4`-`94.7` — and every one of them is a bare `int n` declaration.

That is the empty literal reaching `lower_value_as_int`, where
`unwrap_or(0)` is not a fallback at all. It is *the zero-init*. It is what
makes `int n; echo $n` print `0`.

So the two cases the old code conflated are:

| literal | means | verdict |
|---|---|---|
| `""` | a bare `int n`'s zero-init | **a value** — keep |
| `"1x"` | something the compiler could not read | **a bug** — reject |

I would not have found that by inspection. The temptation was to make the
fallback loud and be done; that would have broken eight passing specs and
the zero-init of every integer declaration in the language.

`eprintln!` would not have found it either. The spec harness captures the
compiler's stderr into `$err`, so an `eprintln!` probe prints nothing and
the sweep stays green. `panic!` is the instrument that works here.

## What bash does, which is the part that makes it a bug

```
$ bash -c 's=abcdef; echo ${s:1+1}'
cdef
$ sash                                   # before
abcdef
```

sash does not evaluate arithmetic in a substring offset. That is a
limitation, and a defensible one. What is not defensible is *silently
producing a different string*. The offset became 0 and the whole value
came back, with no diagnostic, looking exactly like a correct answer.

Same for `${s:1x}` and `${s:1 2}`, which bash rejects outright, and for
`${s:1:2:3}`, whose third part was silently discarded.

## DG's ruling

> the compiler should reject anything not fully understood, so
> unparseable means reject

A principle, so it applies to all its consequences rather than to the one
case that prompted it. Both positions are now **SB4030** — the code that
already carries exactly this meaning elsewhere in `type_check.rs`
(`:438`, *"silently became `0` — reject it instead"*; `:1207`, *"silently
bound 0"*). No new code was needed; the convention existed.

```
exit 1x        → SB4030   exit status `1x` is not a decimal integer
${s:1+1}       → SB4030   substring offset/length must be a decimal integer
```

`${s:(-2)}` still works, and has to: bare `${s:-2}` is the `:-`
disposition, so the parenthesised form is the *only* way to write an
offset from the end. `try_substring_op` strips the parens, so validation
sees `-2`. `${s:1:}` still works too — an omitted length is 0 in bash.

## The fallbacks are now loud, in both backends

Per DD#116 property 1, a fallback that degrades silently wrong is the
thing to remove. `int_literal_or_zero_init` (`codegen/values.rs`) and
`substring_part_int` (`ssa.rs`) both return 0 for empty and **panic** for
anything else, naming SB4030 in the message. Reaching them means a check
was missed, and a silent 0 is precisely what would hide that.

`substring_part_int` lives in `ssa.rs` rather than in either backend
because `emit_substring` and `emit_substring_c` both need it, and a second
copy would be a DD#116 rival — the same mistake the previous entry's
addendum records me making. The C backend agreed with the native one on
the wrong answer before, which is why `--emit-c` could not see this: a
differential oracle is blind to a bug both sides share.

## Numbers

13 specs — `sub.1`-`sub.10` in `types.sh`, `89.9`-`89.11` in
`exit_arg.sh`. Both files are dual-mode, so **+26**. Predicted 5909, swept
5909, 0 failing, **0 flips**. Units 51/27/20.

`bad_substring_part` first came out at cc 6 / cog **7**, over the 5.84
bar and not dispatcher-exempt. Rewritten as a slice `match` with a
`not_an_int` helper it is cc 5 / cog 1. The gate was right to object —
the loop-with-index-special-case really was the harder thing to read.
